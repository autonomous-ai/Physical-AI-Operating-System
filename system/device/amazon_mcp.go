package device

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"net/url"
	"os"
	"os/exec"
	"regexp"
	"strings"
	"sync"
	"time"

	"github.com/creack/pty"
)

// Amazon's Selling Partner connector is a remote MCP server behind Amazon's own sign-in (Seller Central consent,
// OAuth with PKCE). The Hermes agent runtime already knows how to do that sign-in, so the device does not
// re-implement OAuth: it runs `hermes mcp add amazon --auth oauth` on a pseudo-terminal, shows the owner the
// sign-in link Hermes prints, and hands the address the browser lands on back to Hermes (a headless Hermes asks
// for exactly that paste). Hermes stores and refreshes the tokens itself.
const (
	amazonMCPName       = "amazon"
	amazonMCPURL        = "https://sellingpartner-ai.amazon.com/mcp"
	amazonSessionWindow = 10 * time.Minute // how long the owner has to sign in
	amazonURLWait       = 60 * time.Second // how long Hermes gets to print the sign-in link
	amazonFinishWait    = 150 * time.Second
	amazonOutputCap     = 64 << 10
	amazonCallbackMax   = 4096
	hermesGatewayUnit   = "hermes-gateway.service"
)

var (
	ansiEscape       = regexp.MustCompile(`\x1b\[[0-9;?]*[A-Za-z]|\r`)
	amazonSignInURL  = regexp.MustCompile(`https://\S+/authorize\?\S+`)
	promptYesDefault = regexp.MustCompile(`\[Y/n\]\s*:?\s*$`)
	promptNoDefault  = regexp.MustCompile(`\[y/N\]\s*:?\s*$`)
	urlInText        = regexp.MustCompile(`https?://\S+`)
)

// AmazonStatus is what the MCP Tools page shows on the Amazon card.
type AmazonStatus struct {
	// Supported is false when this device cannot run the sign-in itself (another agent runtime).
	Supported bool   `json:"supported"`
	Reason    string `json:"reason,omitempty"`
	Connected bool   `json:"connected"`
	// Pending means a sign-in link was issued and the device is waiting for the pasted address.
	Pending bool `json:"pending"`
}

// hermesCLI is the part of the Hermes command line the Amazon connection needs. Tests replace it.
type hermesCLI interface {
	// Run runs a command to completion without a terminal and returns its combined output.
	Run(ctx context.Context, args ...string) ([]byte, error)
	// StartInteractive starts a command on a pseudo-terminal.
	StartInteractive(args ...string) (interactiveProc, error)
}

type interactiveProc interface {
	io.Reader
	io.Writer
	Kill()
	Done() <-chan struct{}
}

type realHermesCLI struct{}

func (realHermesCLI) Run(ctx context.Context, args ...string) ([]byte, error) {
	return exec.CommandContext(ctx, "hermes", args...).CombinedOutput()
}

type ptyProc struct {
	f    *os.File
	cmd  *exec.Cmd
	done chan struct{}
}

func (realHermesCLI) StartInteractive(args ...string) (interactiveProc, error) {
	cmd := exec.Command("hermes", args...)
	cmd.Env = append(os.Environ(), "TERM=dumb", "NO_COLOR=1")
	f, err := pty.Start(cmd)
	if err != nil {
		return nil, err
	}
	p := &ptyProc{f: f, cmd: cmd, done: make(chan struct{})}
	go func() {
		_ = cmd.Wait()
		close(p.done)
	}()
	return p, nil
}

func (p *ptyProc) Read(b []byte) (int, error)  { return p.f.Read(b) }
func (p *ptyProc) Write(b []byte) (int, error) { return p.f.Write(b) }
func (p *ptyProc) Done() <-chan struct{}       { return p.done }
func (p *ptyProc) Kill() {
	if p.cmd.Process != nil {
		_ = p.cmd.Process.Kill()
	}
	_ = p.f.Close()
}

// amazonSession is one running `hermes mcp add` and the output seen so far.
type amazonSession struct {
	proc      interactiveProc
	mu        sync.Mutex
	out       bytes.Buffer // ANSI-stripped, capped
	scanned   int          // output already checked for prompts
	finishing bool         // the pasted address was sent: answer Hermes's yes/no prompts
	timer     *time.Timer
}

func newAmazonSession(p interactiveProc) *amazonSession {
	s := &amazonSession{proc: p}
	go s.pump()
	return s
}

func (s *amazonSession) pump() {
	buf := make([]byte, 4096)
	for {
		n, err := s.proc.Read(buf)
		if n > 0 {
			s.mu.Lock()
			s.out.WriteString(ansiEscape.ReplaceAllString(string(buf[:n]), ""))
			if s.out.Len() > amazonOutputCap {
				keep := append([]byte(nil), s.out.Bytes()[s.out.Len()-amazonOutputCap/2:]...)
				s.out.Reset()
				s.out.Write(keep)
				s.scanned = 0
			}
			s.answerPromptsLocked()
			s.mu.Unlock()
		}
		if err != nil {
			return
		}
	}
}

// answerPromptsLocked answers Hermes's prompts once the owner has pasted the address: the default for "[Y/n]",
// "no" for "[y/N]" (that one only appears when the connection failed: do not save a broken entry).
func (s *amazonSession) answerPromptsLocked() {
	if !s.finishing {
		return
	}
	tail := s.out.String()[s.scanned:]
	switch {
	case promptYesDefault.MatchString(tail):
		_, _ = s.proc.Write([]byte("\n"))
	case promptNoDefault.MatchString(tail):
		_, _ = s.proc.Write([]byte("n\n"))
	default:
		return
	}
	s.scanned = s.out.Len()
}

func (s *amazonSession) text() string {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.out.String()
}

func (s *amazonSession) signInURL() string {
	return amazonSignInURL.FindString(s.text())
}

// refusedOAuth is true when Hermes fell back to asking whether to continue without authentication.
func (s *amazonSession) refusedOAuth() bool {
	return strings.Contains(s.text(), "Continue without authentication")
}

func (s *amazonSession) startFinishing(line string) error {
	s.mu.Lock()
	s.finishing = true
	s.scanned = s.out.Len()
	s.mu.Unlock()
	_, err := s.proc.Write([]byte(line + "\n"))
	return err
}

func (s *amazonSession) stop() {
	if s.timer != nil {
		s.timer.Stop()
	}
	s.proc.Kill()
}

func (s *Service) amazonHermes() hermesCLI {
	if s.amazonCLI != nil {
		return s.amazonCLI
	}
	return realHermesCLI{}
}

// amazonUnsupported returns why this device cannot run the Amazon sign-in itself, or "".
func (s *Service) amazonUnsupported() string {
	if s.config.AgentRuntime != "hermes" {
		return "The Amazon sign-in from the device needs the Hermes agent runtime. Connect Amazon from your autonomous.ai account instead."
	}
	if s.amazonCLI == nil {
		if _, err := exec.LookPath("hermes"); err != nil {
			return "Hermes is not installed on this device."
		}
	}
	return ""
}

func (s *Service) currentAmazonSession() *amazonSession {
	s.amazonSessMu.Lock()
	defer s.amazonSessMu.Unlock()
	return s.amazonSess
}

func (s *Service) setAmazonSession(sess *amazonSession) {
	s.amazonSessMu.Lock()
	old := s.amazonSess
	s.amazonSess = sess
	s.amazonSessMu.Unlock()
	if old != nil && old != sess {
		old.stop()
	}
}

// expireAmazonSession drops a sign-in the owner never finished.
func (s *Service) expireAmazonSession(sess *amazonSession) {
	s.amazonSessMu.Lock()
	if s.amazonSess == sess {
		s.amazonSess = nil
	}
	s.amazonSessMu.Unlock()
	sess.stop()
	slog.Info("amazon sign-in expired", "component", "device")
}

// amazonConnected asks Hermes whether the amazon server is listed and enabled.
func (s *Service) amazonConnected(ctx context.Context) bool {
	out, err := s.amazonHermes().Run(ctx, "mcp", "list")
	if err != nil {
		return false
	}
	for _, line := range strings.Split(string(ansiEscape.ReplaceAllString(string(out), "")), "\n") {
		f := strings.Fields(line)
		if len(f) > 0 && f[0] == amazonMCPName {
			l := strings.ToLower(line)
			return strings.Contains(l, "enabled") && !strings.Contains(l, "disabled")
		}
	}
	return false
}

// AmazonStatus reports whether the Amazon sign-in can run here, is connected, or is waiting for the paste.
func (s *Service) AmazonStatus() AmazonStatus {
	if reason := s.amazonUnsupported(); reason != "" {
		return AmazonStatus{Reason: reason}
	}
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()
	return AmazonStatus{Supported: true, Connected: s.amazonConnected(ctx), Pending: s.currentAmazonSession() != nil}
}

// AmazonConnectStart starts Hermes's Amazon sign-in and returns the link to open in the owner's browser.
func (s *Service) AmazonConnectStart() (string, error) {
	if reason := s.amazonUnsupported(); reason != "" {
		return "", errors.New(reason)
	}
	s.amazonMu.Lock()
	defer s.amazonMu.Unlock()
	s.setAmazonSession(nil)

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	// A leftover entry would make Hermes ask about overwriting it; an absent one is fine.
	_, _ = s.amazonHermes().Run(ctx, "mcp", "remove", amazonMCPName)

	proc, err := s.amazonHermes().StartInteractive("mcp", "add", amazonMCPName, "--url", amazonMCPURL,
		"--auth", "oauth", "--connect-timeout", fmt.Sprint(int(amazonSessionWindow.Seconds())))
	if err != nil {
		return "", fmt.Errorf("could not start the Amazon sign-in: %w", err)
	}
	sess := newAmazonSession(proc)

	deadline := time.Now().Add(amazonURLWait)
	for time.Now().Before(deadline) {
		if u := sess.signInURL(); u != "" {
			sess.timer = time.AfterFunc(amazonSessionWindow, func() { s.expireAmazonSession(sess) })
			s.setAmazonSession(sess)
			return u, nil
		}
		if sess.refusedOAuth() {
			sess.stop()
			return "", errors.New("Amazon's server did not offer a sign-in this device can use. Connect Amazon from your autonomous.ai account instead.")
		}
		select {
		case <-proc.Done():
			sess.stop()
			return "", fmt.Errorf("the Amazon sign-in stopped before it gave a link: %s", tailForUser(sess.text()))
		case <-time.After(100 * time.Millisecond):
		}
	}
	sess.stop()
	return "", errors.New("Amazon did not answer in time. Try again in a minute.")
}

// validateAmazonCallback accepts the address the browser ended up on after Amazon's sign-in (a loopback URL
// carrying ?code=…&state=… or ?error=…), or just that query part, and nothing else: it is typed into a
// terminal, so it must be one short line.
func validateAmazonCallback(raw string) (string, error) {
	v := strings.TrimSpace(raw)
	if v == "" || len(v) > amazonCallbackMax || strings.ContainsAny(v, "\r\n\x00") {
		return "", errors.New("paste the full address from your browser's address bar after you signed in to Amazon")
	}
	query := ""
	switch {
	case strings.HasPrefix(v, "?"):
		query = strings.TrimPrefix(v, "?")
	case strings.HasPrefix(v, "code=") || strings.HasPrefix(v, "error="):
		query = v
	default:
		u, err := url.Parse(v)
		host := ""
		if err == nil {
			host = u.Hostname()
		}
		if err != nil || u.Scheme != "http" || (host != "127.0.0.1" && host != "localhost" && host != "::1") {
			return "", errors.New("that is not the address Amazon sent you back to: it starts with http://127.0.0.1 and the page itself does not load, which is expected")
		}
		query = u.RawQuery
	}
	q, err := url.ParseQuery(query)
	if err != nil || (q.Get("code") == "" && q.Get("error") == "") {
		return "", errors.New("the address has no sign-in code in it; copy it again right after Amazon's page, or press Connect Amazon again")
	}
	return v, nil
}

// AmazonConnectFinish hands the pasted address to Hermes, checks the connection, and restarts the gateway once
// so the agent loads Amazon's tools.
func (s *Service) AmazonConnectFinish(callback string) error {
	line, err := validateAmazonCallback(callback)
	if err != nil {
		return err
	}
	s.amazonMu.Lock()
	defer s.amazonMu.Unlock()
	sess := s.currentAmazonSession()
	if sess == nil {
		return errors.New("there is no Amazon sign-in in progress; press Connect Amazon again")
	}
	if err := sess.startFinishing(line); err != nil {
		s.setAmazonSession(nil)
		return errors.New("the Amazon sign-in had already ended; press Connect Amazon again")
	}
	select {
	case <-sess.proc.Done():
	case <-time.After(amazonFinishWait):
		s.setAmazonSession(nil)
		return errors.New("Amazon did not finish connecting in time; press Connect Amazon and try again")
	}
	outcome := sess.text()
	s.setAmazonSession(nil)

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	if !s.amazonConnected(ctx) {
		_, _ = s.amazonHermes().Run(ctx, "mcp", "remove", amazonMCPName)
		return fmt.Errorf("Amazon did not accept the sign-in: %s", tailForUser(outcome))
	}
	if err := s.restartAmazonGateway(); err != nil {
		slog.Warn("amazon connect: gateway restart failed", "component", "device", "error", err)
	}
	slog.Info("amazon connected", "component", "device")
	return nil
}

// AmazonConnectCancel stops a sign-in the owner gave up on, so Hermes does not wait for the full window.
func (s *Service) AmazonConnectCancel() {
	s.setAmazonSession(nil)
}

// AmazonDisconnect removes the amazon server (and its saved tokens) from Hermes and reloads the gateway.
func (s *Service) AmazonDisconnect() error {
	if reason := s.amazonUnsupported(); reason != "" {
		return errors.New(reason)
	}
	s.amazonMu.Lock()
	defer s.amazonMu.Unlock()
	s.setAmazonSession(nil)
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	if out, err := s.amazonHermes().Run(ctx, "mcp", "remove", amazonMCPName); err != nil && s.amazonConnected(ctx) {
		return fmt.Errorf("could not remove Amazon: %s", tailForUser(string(out)))
	}
	if err := s.restartAmazonGateway(); err != nil {
		slog.Warn("amazon disconnect: gateway restart failed", "component", "device", "error", err)
	}
	return nil
}

func (s *Service) restartAmazonGateway() error {
	if s.amazonRestart != nil {
		return s.amazonRestart()
	}
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Minute)
	defer cancel()
	if out, err := exec.CommandContext(ctx, "systemctl", "restart", hermesGatewayUnit).CombinedOutput(); err != nil {
		return fmt.Errorf("systemctl restart %s: %s: %w", hermesGatewayUnit, strings.TrimSpace(string(out)), err)
	}
	return nil
}

// tailForUser is the last few lines of Hermes's output with links (and the codes inside them) removed, capped.
func tailForUser(out string) string {
	lines := strings.Split(strings.TrimSpace(urlInText.ReplaceAllString(out, "<link>")), "\n")
	var keep []string
	for i := len(lines) - 1; i >= 0 && len(keep) < 3; i-- {
		if l := strings.TrimSpace(lines[i]); l != "" {
			keep = append([]string{l}, keep...)
		}
	}
	msg := strings.Join(keep, " ")
	if len(msg) > 240 {
		msg = msg[:240]
	}
	if msg == "" {
		return "no details"
	}
	return msg
}
