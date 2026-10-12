package device

import (
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"net/http/httputil"
	"net/url"
	"strings"
	"time"

	"go.autonomous.ai/os/system/server/config"
)

// Fo (Wajo's errand agent) is an MCP tool behind Wajo OAuth: PKCE, dynamic
// client registration, 1 h access tokens and a refresh token that rotates on
// every refresh. Wajo only accepts https or loopback redirect URIs, so the
// owner signs in on their own browser, lands on a localhost URL that fails to
// load, and pastes that URL back into the web UI.
//
// The agent never holds the tokens. Its MCP entry points at os-server's local
// proxy (ServeFoMCP), which adds a live token. Writing an MCP entry restarts
// the runtime, so rewriting it on every hourly refresh would cut turns short.
var (
	foMCPURL    = "https://mcp.api.wajo.ai/fo"
	foOAuthBase = "https://mcp.api.wajo.ai/oauth"
)

const (
	foToolName       = "fo"
	foRedirectURI    = "http://localhost:5000/oauth/fo/callback"
	foProxyKeyHeader = "X-Fo-Proxy-Key"
	foRefreshSkew    = 5 * time.Minute
)

var foHTTP = &http.Client{Timeout: 20 * time.Second}

// errFoNotConnected means there is no Wajo sign-in to use.
var errFoNotConnected = errors.New("fo is not connected")

// foHTTPError is a non-2xx answer from Wajo. A 4xx from the token endpoint
// (invalid_grant) means the sign-in is dead; 5xx and network errors are not.
type foHTTPError struct {
	status int
	msg    string
}

func (e *foHTTPError) Error() string { return e.msg }

// FoStatus is what the web UI shows on the Fo card.
type FoStatus struct {
	Connected bool `json:"connected"`
	// NeedsReconnect: the tool exists but Wajo rejected the refresh token.
	NeedsReconnect bool `json:"needs_reconnect"`
}

type foTokenResponse struct {
	AccessToken  string `json:"access_token"`
	RefreshToken string `json:"refresh_token"`
	ExpiresIn    int    `json:"expires_in"`
}

func randomHex(n int) string {
	b := make([]byte, n)
	_, _ = rand.Read(b)
	return hex.EncodeToString(b)
}

func foPost(endpoint string, contentType string, body io.Reader, out any) error {
	resp, err := foHTTP.Post(endpoint, contentType, body)
	if err != nil {
		return fmt.Errorf("wajo %s: %w", endpoint, err)
	}
	defer resp.Body.Close()
	raw, _ := io.ReadAll(io.LimitReader(resp.Body, 64<<10))
	if resp.StatusCode/100 != 2 {
		return &foHTTPError{resp.StatusCode, fmt.Sprintf("wajo %s: HTTP %d: %s", endpoint, resp.StatusCode, strings.TrimSpace(string(raw)))}
	}
	if err := json.Unmarshal(raw, out); err != nil {
		return fmt.Errorf("wajo %s: decode: %w", endpoint, err)
	}
	return nil
}

func foRegisterClient() (string, error) {
	body, _ := json.Marshal(map[string]any{
		"client_name":                "Autonomous OS",
		"redirect_uris":              []string{foRedirectURI},
		"grant_types":                []string{"authorization_code", "refresh_token"},
		"response_types":             []string{"code"},
		"token_endpoint_auth_method": "none",
		"scope":                      "mcp",
	})
	var out struct {
		ClientID string `json:"client_id"`
	}
	if err := foPost(foOAuthBase+"/register", "application/json", strings.NewReader(string(body)), &out); err != nil {
		return "", err
	}
	if out.ClientID == "" {
		return "", errors.New("wajo register: no client_id")
	}
	return out.ClientID, nil
}

func foTokenRequest(form url.Values) (foTokenResponse, error) {
	form.Set("resource", foMCPURL)
	var tok foTokenResponse
	if err := foPost(foOAuthBase+"/token", "application/x-www-form-urlencoded", strings.NewReader(form.Encode()), &tok); err != nil {
		return tok, err
	}
	if tok.AccessToken == "" {
		return tok, errors.New("wajo token: no access_token")
	}
	return tok, nil
}

// FoConnectStart registers this device with Wajo on first use and returns the
// sign-in URL to open in the owner's browser.
func (s *Service) FoConnectStart() (string, error) {
	clientID := s.config.FoAuth.ClientID
	if clientID == "" {
		id, err := foRegisterClient()
		if err != nil {
			return "", err
		}
		clientID = id
	}
	verifier := randomHex(32)
	state := randomHex(8)
	sum := sha256.Sum256([]byte(verifier))
	if err := s.config.WithLockSave(func(c *config.Config) {
		c.FoAuth.ClientID = clientID
		c.FoAuth.PendingVerifier = verifier
		c.FoAuth.PendingState = state
	}); err != nil {
		return "", fmt.Errorf("save fo sign-in: %w", err)
	}
	q := url.Values{
		"response_type":         {"code"},
		"client_id":             {clientID},
		"redirect_uri":          {foRedirectURI},
		"scope":                 {"mcp"},
		"state":                 {state},
		"code_challenge":        {base64.RawURLEncoding.EncodeToString(sum[:])},
		"code_challenge_method": {"S256"},
		"resource":              {foMCPURL},
	}
	return foOAuthBase + "/authorize?" + q.Encode(), nil
}

// FoConnectFinish takes the localhost URL the browser landed on after sign-in,
// stores the tokens and adds the "fo" MCP tool pointing at the local proxy.
func (s *Service) FoConnectFinish(callbackURL string) error {
	u, err := url.Parse(strings.TrimSpace(callbackURL))
	if err != nil {
		return fmt.Errorf("callback url: %w", err)
	}
	q := u.Query()
	if e := q.Get("error"); e != "" {
		return fmt.Errorf("wajo sign-in failed: %s %s", e, q.Get("error_description"))
	}
	auth := s.config.FoAuth
	if auth.PendingState == "" || q.Get("state") != auth.PendingState {
		return errors.New("this link is not from the latest Connect Fo; press Connect Fo again")
	}
	code := q.Get("code")
	if code == "" {
		return errors.New("callback url has no code")
	}
	tok, err := foTokenRequest(url.Values{
		"grant_type":    {"authorization_code"},
		"code":          {code},
		"client_id":     {auth.ClientID},
		"redirect_uri":  {foRedirectURI},
		"code_verifier": {auth.PendingVerifier},
	})
	if err != nil {
		return err
	}
	key := auth.ProxyKey
	if key == "" {
		key = randomHex(32)
	}
	if err := s.config.WithLockSave(func(c *config.Config) {
		c.FoAuth.AccessToken = tok.AccessToken
		c.FoAuth.RefreshToken = tok.RefreshToken
		c.FoAuth.ExpiresAt = time.Now().Add(time.Duration(tok.ExpiresIn) * time.Second).Unix()
		c.FoAuth.ProxyKey = key
		c.FoAuth.PendingVerifier = ""
		c.FoAuth.PendingState = ""
	}); err != nil {
		return fmt.Errorf("save fo tokens: %w", err)
	}
	slog.Info("[fo] signed in", "component", "device")

	// Reconnecting only renews the tokens; the existing entry already points at the proxy.
	for _, t := range s.config.MCPTools {
		if t.Name == foToolName {
			return nil
		}
	}
	return s.AddMCPTool(config.MCPTool{
		Name:    foToolName,
		URL:     fmt.Sprintf("http://127.0.0.1:%d/api/fo-mcp", s.config.HttpPort),
		Headers: map[string]string{foProxyKeyHeader: key},
	})
}

// foAccessToken returns a live access token, refreshing it near expiry.
func (s *Service) foAccessToken() (string, error) {
	s.foMu.Lock()
	defer s.foMu.Unlock()
	auth := s.config.FoAuth
	if auth.AccessToken == "" {
		return "", errFoNotConnected
	}
	if time.Now().Add(foRefreshSkew).Unix() < auth.ExpiresAt {
		return auth.AccessToken, nil
	}
	if auth.RefreshToken == "" {
		return "", errFoNotConnected
	}
	tok, err := foTokenRequest(url.Values{
		"grant_type":    {"refresh_token"},
		"refresh_token": {auth.RefreshToken},
		"client_id":     {auth.ClientID},
	})
	var httpErr *foHTTPError
	if errors.As(err, &httpErr) && httpErr.status >= 400 && httpErr.status < 500 {
		// The refresh token is spent or revoked: only a new sign-in helps.
		slog.Warn("[fo] refresh rejected, sign-in needed", "component", "device", "err", err)
		if saveErr := s.config.WithLockSave(func(c *config.Config) {
			c.FoAuth.AccessToken = ""
			c.FoAuth.RefreshToken = ""
			c.FoAuth.ExpiresAt = 0
		}); saveErr != nil {
			slog.Error("[fo] clear dead tokens failed", "component", "device", "err", saveErr)
		}
		return "", errFoNotConnected
	}
	if err != nil {
		return "", err
	}
	// Wajo rotates the refresh token on every refresh: the old one is spent.
	if tok.RefreshToken == "" {
		tok.RefreshToken = auth.RefreshToken
	}
	if err := s.config.WithLockSave(func(c *config.Config) {
		c.FoAuth.AccessToken = tok.AccessToken
		c.FoAuth.RefreshToken = tok.RefreshToken
		c.FoAuth.ExpiresAt = time.Now().Add(time.Duration(tok.ExpiresIn) * time.Second).Unix()
	}); err != nil {
		// The new refresh token lives only in memory now; a restart would need a new sign-in.
		slog.Error("[fo] save refreshed tokens failed", "component", "device", "err", err)
	}
	return tok.AccessToken, nil
}

// GetFoStatus reports whether Fo is connected and whether its sign-in has died.
func (s *Service) GetFoStatus() FoStatus {
	for _, t := range s.config.MCPTools {
		if t.Name == foToolName {
			return FoStatus{Connected: true, NeedsReconnect: s.config.FoAuth.AccessToken == ""}
		}
	}
	return FoStatus{}
}

// ServeFoMCP forwards the agent's MCP traffic to Wajo with a live token. The
// proxy key, not admin auth, guards it: the agent's MCP client sends it as a header.
func (s *Service) ServeFoMCP(w http.ResponseWriter, r *http.Request) {
	want := s.config.FoAuth.ProxyKey
	got := r.Header.Get(foProxyKeyHeader)
	if want == "" || subtle.ConstantTimeCompare([]byte(got), []byte(want)) != 1 {
		http.NotFound(w, r)
		return
	}
	tok, err := s.foAccessToken()
	if err != nil {
		slog.Warn("[fo] no access token", "component", "device", "err", err)
		http.Error(w, "Fo's Wajo sign-in has expired or is missing. Ask the owner to press Reconnect on the Fo card in Settings > MCP Tools.", http.StatusUnauthorized)
		return
	}
	upstream, _ := url.Parse(foMCPURL)
	proxy := &httputil.ReverseProxy{
		Rewrite: func(pr *httputil.ProxyRequest) {
			pr.Out.URL.Scheme = upstream.Scheme
			pr.Out.URL.Host = upstream.Host
			pr.Out.URL.Path = upstream.Path
			pr.Out.URL.RawPath = ""
			pr.Out.Host = upstream.Host
			pr.Out.Header.Del(foProxyKeyHeader)
			pr.Out.Header.Set("Authorization", "Bearer "+tok)
		},
		// MCP replies may be an event stream: pass each event through at once.
		FlushInterval: -1,
	}
	proxy.ServeHTTP(w, r)
}
