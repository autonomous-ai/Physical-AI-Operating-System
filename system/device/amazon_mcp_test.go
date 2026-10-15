package device

import (
	"context"
	"io"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"go.autonomous.ai/os/system/server/config"
)

const (
	testSignIn = "https://sellercentral.amazon.com/apps/agents/authorize?response_type=code&client_id=c&state=s1&resource=x"
	testSecret = "AUTHCODE-SECRET-123"
)

// fakeProc is a scripted pseudo-terminal: the test writes Hermes's output with emit, and reads what the
// code typed with typed.
type fakeProc struct {
	pr     *io.PipeReader
	pw     *io.PipeWriter
	in     chan string
	done   chan struct{}
	once   sync.Once
	killed atomic.Bool
}

func newFakeProc() *fakeProc {
	pr, pw := io.Pipe()
	return &fakeProc{pr: pr, pw: pw, in: make(chan string, 16), done: make(chan struct{})}
}

func (p *fakeProc) Read(b []byte) (int, error) { return p.pr.Read(b) }
func (p *fakeProc) Write(b []byte) (int, error) {
	p.in <- string(b)
	return len(b), nil
}
func (p *fakeProc) Done() <-chan struct{} { return p.done }
func (p *fakeProc) Kill()                 { p.killed.Store(true); p.exit() }
func (p *fakeProc) exit()                 { p.once.Do(func() { close(p.done); _ = p.pw.Close() }) }
func (p *fakeProc) emit(s string)         { _, _ = p.pw.Write([]byte(s)) }
func (p *fakeProc) typed(t *testing.T) string {
	t.Helper()
	select {
	case s := <-p.in:
		return s
	case <-time.After(3 * time.Second):
		t.Fatal("the code typed nothing")
		return ""
	}
}

type fakeHermes struct {
	mu       sync.Mutex
	runs     [][]string
	listOut  string
	proc     *fakeProc
	startErr error
	started  [][]string
}

func (f *fakeHermes) Run(_ context.Context, args ...string) ([]byte, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.runs = append(f.runs, args)
	if len(args) >= 2 && args[1] == "list" {
		return []byte(f.listOut), nil
	}
	return nil, nil
}

func (f *fakeHermes) StartInteractive(args ...string) (interactiveProc, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.started = append(f.started, args)
	if f.startErr != nil {
		return nil, f.startErr
	}
	f.proc = newFakeProc()
	return f.proc, nil
}

func (f *fakeHermes) ran(sub string) int {
	f.mu.Lock()
	defer f.mu.Unlock()
	n := 0
	for _, r := range f.runs {
		if strings.Contains(strings.Join(r, " "), sub) {
			n++
		}
	}
	return n
}

func newAmazonService(f *fakeHermes) (*Service, *int32) {
	var restarts int32
	return &Service{
		config:        &config.Config{AgentRuntime: "hermes"},
		amazonCLI:     f,
		amazonRestart: func() error { atomic.AddInt32(&restarts, 1); return nil },
	}, &restarts
}

const signInOutput = "\n  Starting OAuth flow for 'amazon'...\n  MCP OAuth: authorization required.\n  Open this URL in your browser:\n\n    " + testSignIn + "\n\n  Or paste the redirect URL here:\n"

// startWith runs AmazonConnectStart while the fake emits `out` as soon as the process exists.
func startWith(t *testing.T, s *Service, f *fakeHermes, out string) (string, error) {
	t.Helper()
	type res struct {
		u   string
		err error
	}
	ch := make(chan res, 1)
	go func() { u, err := s.AmazonConnectStart(); ch <- res{u, err} }()
	deadline := time.Now().Add(3 * time.Second)
	for {
		f.mu.Lock()
		p := f.proc
		f.mu.Unlock()
		if p != nil {
			if out != "" {
				p.emit(out)
			}
			break
		}
		if time.Now().After(deadline) {
			t.Fatal("process never started")
		}
		time.Sleep(5 * time.Millisecond)
	}
	select {
	case r := <-ch:
		return r.u, r.err
	case <-time.After(5 * time.Second):
		t.Fatal("AmazonConnectStart did not return")
		return "", nil
	}
}

func TestAmazonStart_ReturnsTheSignInLink(t *testing.T) {
	f := &fakeHermes{}
	s, _ := newAmazonService(f)
	u, err := startWith(t, s, f, signInOutput)
	if err != nil || u != testSignIn {
		t.Fatalf("got %q, %v", u, err)
	}
	if got := strings.Join(f.started[0], " "); got != "mcp add amazon --url https://sellingpartner-ai.amazon.com/mcp --auth oauth --connect-timeout 600" {
		t.Fatalf("unexpected command: %s", got)
	}
	if f.ran("mcp remove amazon") != 1 {
		t.Fatal("a leftover entry must be removed first")
	}
	if !s.AmazonStatus().Pending {
		t.Fatal("status must show the sign-in as pending")
	}
	s.setAmazonSession(nil)
}

func TestAmazonStart_RefusedOAuthIsExplainedAndStopped(t *testing.T) {
	f := &fakeHermes{}
	s, _ := newAmazonService(f)
	_, err := startWith(t, s, f, "\n  OAuth error: x\n  Continue without authentication? [Y/n]: ")
	if err == nil || !strings.Contains(err.Error(), "autonomous.ai") {
		t.Fatalf("want a pointer to autonomous.ai, got %v", err)
	}
	if !f.proc.killed.Load() {
		t.Fatal("the process must be stopped")
	}
}

func TestAmazonStart_EarlyExitHidesLinks(t *testing.T) {
	f := &fakeHermes{}
	s, _ := newAmazonService(f)
	go func() {
		time.Sleep(50 * time.Millisecond)
		f.mu.Lock()
		p := f.proc
		f.mu.Unlock()
		p.emit("boom: see https://example.test/x?code=" + testSecret + "\n")
		time.Sleep(20 * time.Millisecond)
		p.exit()
	}()
	_, err := s.AmazonConnectStart()
	if err == nil || strings.Contains(err.Error(), testSecret) || !strings.Contains(err.Error(), "<link>") {
		t.Fatalf("want a message without the code, got %v", err)
	}
}

func TestAmazonFinish_HappyPathAnswersPromptsAndRestartsOnce(t *testing.T) {
	f := &fakeHermes{}
	s, restarts := newAmazonService(f)
	if _, err := startWith(t, s, f, signInOutput); err != nil {
		t.Fatal(err)
	}
	p := f.proc
	f.listOut = "  amazon   https://sellingpartner-ai.amazon.com/mcp   all   ✓ enabled\n"

	errc := make(chan error, 1)
	go func() {
		errc <- s.AmazonConnectFinish("http://127.0.0.1:27890/callback?code=" + testSecret + "&state=s1")
	}()

	if got := p.typed(t); !strings.Contains(got, "code="+testSecret) || !strings.HasSuffix(got, "\n") {
		t.Fatalf("the pasted address must be typed as one line, got %q", got)
	}
	p.emit("  Connected. Enable all 12 tools? [Y/n]: ")
	if got := p.typed(t); got != "\n" {
		t.Fatalf("a [Y/n] prompt must get the default, got %q", got)
	}
	p.emit("  ✓ Saved 'amazon'\n")
	p.exit()

	if err := <-errc; err != nil {
		t.Fatalf("finish: %v", err)
	}
	if atomic.LoadInt32(restarts) != 1 {
		t.Fatalf("the gateway must restart exactly once, restarted %d", *restarts)
	}
	if s.currentAmazonSession() != nil {
		t.Fatal("the session must be cleared")
	}
}

func TestAmazonFinish_FailureIsCleanedUpAndNeverEchoesTheCode(t *testing.T) {
	f := &fakeHermes{}
	s, restarts := newAmazonService(f)
	if _, err := startWith(t, s, f, signInOutput); err != nil {
		t.Fatal(err)
	}
	p := f.proc
	f.listOut = "No MCP servers configured.\n"

	errc := make(chan error, 1)
	go func() {
		errc <- s.AmazonConnectFinish("http://127.0.0.1:27890/callback?code=" + testSecret + "&state=s1")
	}()
	_ = p.typed(t)
	p.emit("  invalid_grant for " + testSignIn + "&code=" + testSecret + "\n  Save config anyway? [y/N]: ")
	if got := p.typed(t); got != "n\n" {
		t.Fatalf("a [y/N] save prompt must get no, got %q", got)
	}
	p.exit()

	err := <-errc
	if err == nil || strings.Contains(err.Error(), testSecret) || !strings.Contains(err.Error(), "did not accept") {
		t.Fatalf("want a clean failure message, got %v", err)
	}
	if f.ran("mcp remove amazon") < 2 {
		t.Fatal("a failed sign-in must remove the half-made entry")
	}
	if atomic.LoadInt32(restarts) != 0 {
		t.Fatal("no restart for a failed sign-in")
	}
}

func TestAmazonFinish_NeedsASession(t *testing.T) {
	s, _ := newAmazonService(&fakeHermes{})
	err := s.AmazonConnectFinish("http://127.0.0.1:27890/callback?code=a&state=b")
	if err == nil || !strings.Contains(err.Error(), "no Amazon sign-in in progress") {
		t.Fatalf("got %v", err)
	}
}

func TestValidateAmazonCallback(t *testing.T) {
	good := []string{
		"http://127.0.0.1:27890/callback?code=a&state=b",
		"http://localhost:5000/cb?code=a&state=b",
		"http://[::1]:1/x?error=access_denied",
		"?code=a&state=b",
		"code=a&state=b",
		"  http://127.0.0.1:27890/callback?code=a&state=b  ",
	}
	for _, v := range good {
		if _, err := validateAmazonCallback(v); err != nil {
			t.Errorf("%q should be accepted: %v", v, err)
		}
	}
	bad := []string{
		"", "   ", "https://evil.example/callback?code=a", "http://evil.example/callback?code=a",
		"http://127.0.0.1.evil.example/cb?code=a", "http://127.0.0.1:1/cb", "ftp://127.0.0.1/x?code=a",
		"http://127.0.0.1:1/cb?code=a\nrm -rf /", "code=a\rn", "http://127.0.0.1:1/cb?code=" + strings.Repeat("a", amazonCallbackMax),
		"just some text",
	}
	for _, v := range bad {
		if _, err := validateAmazonCallback(v); err == nil {
			t.Errorf("%q must be rejected", v)
		}
	}
}

func TestAmazonUnsupportedRuntime(t *testing.T) {
	s := &Service{config: &config.Config{AgentRuntime: "openclaw"}, amazonCLI: &fakeHermes{}}
	st := s.AmazonStatus()
	if st.Supported || !strings.Contains(st.Reason, "Hermes") {
		t.Fatalf("status: %+v", st)
	}
	if _, err := s.AmazonConnectStart(); err == nil {
		t.Fatal("start must refuse on another runtime")
	}
	if err := s.AmazonDisconnect(); err == nil {
		t.Fatal("disconnect must refuse on another runtime")
	}
}

func TestAmazonStatus_ParsesTheServerList(t *testing.T) {
	cases := map[string]bool{
		"  amazon   https://x   all   ✓ enabled\n":                       true,
		"  amazon   https://x   all   ✗ disabled\n":                      false,
		"  notion   https://n   all   ✓ enabled\n":                       false,
		"  notion   https://n   ✓ enabled\n  amazon  https://x  enabled": true,
		"No MCP servers configured.\n":                                   false,
	}
	for out, want := range cases {
		f := &fakeHermes{listOut: out}
		s, _ := newAmazonService(f)
		if got := s.AmazonStatus(); !got.Supported || got.Connected != want {
			t.Errorf("%q: connected=%v want %v", out, got.Connected, want)
		}
	}
}

func TestAmazonDisconnect_RemovesAndRestarts(t *testing.T) {
	f := &fakeHermes{}
	s, restarts := newAmazonService(f)
	if err := s.AmazonDisconnect(); err != nil {
		t.Fatal(err)
	}
	if f.ran("mcp remove amazon") != 1 || atomic.LoadInt32(restarts) != 1 {
		t.Fatalf("remove=%d restarts=%d", f.ran("mcp remove amazon"), *restarts)
	}
}

func TestAmazonSession_ExpiresAndASecondStartReplacesTheFirst(t *testing.T) {
	f := &fakeHermes{}
	s, _ := newAmazonService(f)
	if _, err := startWith(t, s, f, signInOutput); err != nil {
		t.Fatal(err)
	}
	first := f.proc
	f.mu.Lock()
	f.proc = nil // so the helper waits for the second process
	f.mu.Unlock()
	if _, err := startWith(t, s, f, signInOutput); err != nil {
		t.Fatal(err)
	}
	if !first.killed.Load() {
		t.Fatal("a new Connect must stop the previous sign-in")
	}
	sess := s.currentAmazonSession()
	s.expireAmazonSession(sess)
	if s.currentAmazonSession() != nil || !f.proc.killed.Load() {
		t.Fatal("an expired sign-in must be dropped and its process stopped")
	}
}

func TestAmazonCancel_StopsThePendingSignIn(t *testing.T) {
	f := &fakeHermes{}
	s, _ := newAmazonService(f)
	if _, err := startWith(t, s, f, signInOutput); err != nil {
		t.Fatal(err)
	}
	s.AmazonConnectCancel()
	if s.currentAmazonSession() != nil || !f.proc.killed.Load() {
		t.Fatal("cancel must drop the session and stop the process")
	}
	s.AmazonConnectCancel() // nothing pending: must be harmless
}

func TestAnsiAndLinksAreStrippedFromUserMessages(t *testing.T) {
	got := tailForUser("a\nb https://x.test/y?code=SECRET c\nd\ne")
	if strings.Contains(got, "SECRET") || !strings.Contains(got, "e") {
		t.Fatalf("got %q", got)
	}
	if tailForUser("  \n ") != "no details" {
		t.Fatal("empty output must say so")
	}
}
