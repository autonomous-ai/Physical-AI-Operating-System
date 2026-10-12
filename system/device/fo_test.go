package device

import (
	"io"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"testing"
	"time"

	"go.autonomous.ai/os/system/domain"
	"go.autonomous.ai/os/system/server/config"
)

// mcpGateway records MCP entry writes; other calls panic.
type mcpGateway struct {
	domain.AgentGateway
	written map[string]map[string]any
	removed []string
}

func (g *mcpGateway) WriteMCPEntry(name string, entry map[string]any) error {
	g.written[name] = entry
	return nil
}

func (g *mcpGateway) RemoveMCPEntry(name string) (bool, error) {
	g.removed = append(g.removed, name)
	return true, nil
}

// fakeWajo is Wajo's OAuth + Fo MCP endpoints: refresh tokens rotate, and the
// MCP endpoint echoes the Authorization header it received.
func fakeWajo(t *testing.T) (*httptest.Server, *int) {
	t.Helper()
	refreshes := 0
	mux := http.NewServeMux()
	mux.HandleFunc("/oauth/register", func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusCreated)
		_, _ = io.WriteString(w, `{"client_id":"client-1"}`)
	})
	mux.HandleFunc("/oauth/token", func(w http.ResponseWriter, r *http.Request) {
		_ = r.ParseForm()
		switch r.Form.Get("grant_type") {
		case "authorization_code":
			if r.Form.Get("code") != "code-1" || r.Form.Get("code_verifier") == "" {
				http.Error(w, `{"error":"invalid_grant"}`, http.StatusBadRequest)
				return
			}
			_, _ = io.WriteString(w, `{"access_token":"at-1","refresh_token":"rt-1","expires_in":3600}`)
		case "refresh_token":
			if r.Form.Get("refresh_token") != "rt-1" {
				http.Error(w, `{"error":"invalid_grant"}`, http.StatusBadRequest)
				return
			}
			refreshes++
			_, _ = io.WriteString(w, `{"access_token":"at-2","refresh_token":"rt-2","expires_in":3600}`)
		}
	})
	mux.HandleFunc("/fo", func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get(foProxyKeyHeader) != "" {
			t.Errorf("proxy key leaked upstream")
		}
		_, _ = io.WriteString(w, r.Header.Get("Authorization"))
	})
	srv := httptest.NewServer(mux)
	t.Cleanup(srv.Close)
	origMCP, origOAuth := foMCPURL, foOAuthBase
	foMCPURL, foOAuthBase = srv.URL+"/fo", srv.URL+"/oauth"
	t.Cleanup(func() { foMCPURL, foOAuthBase = origMCP, origOAuth })
	return srv, &refreshes
}

func proxyCall(s *Service, key string) (int, string) {
	req := httptest.NewRequest(http.MethodPost, "/api/fo-mcp", strings.NewReader(`{}`))
	if key != "" {
		req.Header.Set(foProxyKeyHeader, key)
	}
	rec := httptest.NewRecorder()
	s.ServeFoMCP(rec, req)
	return rec.Code, rec.Body.String()
}

func TestFoConnectProxyRefreshAndRemove(t *testing.T) {
	t.Chdir(t.TempDir()) // WithLockSave writes config/config.json
	_, refreshes := fakeWajo(t)
	gw := &mcpGateway{written: map[string]map[string]any{}}
	s := &Service{config: &config.Config{HttpPort: 5000}, agentGateway: gw}

	authURL, err := s.FoConnectStart()
	if err != nil {
		t.Fatal(err)
	}
	q, _ := url.ParseQuery(strings.SplitN(authURL, "?", 2)[1])
	if q.Get("client_id") != "client-1" || q.Get("code_challenge") == "" || q.Get("redirect_uri") != foRedirectURI {
		t.Fatalf("auth url: %s", authURL)
	}

	if err := s.FoConnectFinish(foRedirectURI + "?code=code-1&state=wrong"); err == nil {
		t.Fatal("state mismatch accepted")
	}
	if err := s.FoConnectFinish(foRedirectURI + "?code=code-1&state=" + q.Get("state")); err != nil {
		t.Fatal(err)
	}
	key := s.config.FoAuth.ProxyKey
	entry := gw.written["tool_fo"]
	if entry["url"] != "http://127.0.0.1:5000/api/fo-mcp" || entry["headers"].(map[string]any)[foProxyKeyHeader] != key {
		t.Fatalf("mcp entry: %v", entry)
	}

	if code, _ := proxyCall(s, ""); code != http.StatusNotFound {
		t.Fatalf("no key: %d", code)
	}
	if code, _ := proxyCall(s, "wrong"); code != http.StatusNotFound {
		t.Fatalf("wrong key: %d", code)
	}
	if code, body := proxyCall(s, key); code != http.StatusOK || body != "Bearer at-1" {
		t.Fatalf("proxy: %d %q", code, body)
	}

	// Near expiry: refresh once, store the rotated refresh token, no MCP rewrite.
	s.config.FoAuth.ExpiresAt = time.Now().Add(time.Minute).Unix()
	writes := len(gw.written)
	if _, body := proxyCall(s, key); body != "Bearer at-2" {
		t.Fatalf("after refresh: %q", body)
	}
	if *refreshes != 1 || s.config.FoAuth.RefreshToken != "rt-2" || len(gw.written) != writes {
		t.Fatalf("refresh: n=%d rt=%s writes=%d", *refreshes, s.config.FoAuth.RefreshToken, len(gw.written))
	}

	// Reconnect keeps the single tool entry.
	authURL, _ = s.FoConnectStart()
	q, _ = url.ParseQuery(strings.SplitN(authURL, "?", 2)[1])
	if err := s.FoConnectFinish(foRedirectURI + "?code=code-1&state=" + q.Get("state")); err != nil {
		t.Fatal(err)
	}
	if len(s.config.MCPTools) != 1 {
		t.Fatalf("tools after reconnect: %v", s.config.MCPTools)
	}

	if err := s.RemoveMCPTool(foToolName); err != nil {
		t.Fatal(err)
	}
	if s.config.FoAuth.AccessToken != "" || s.config.FoAuth.ClientID != "client-1" {
		t.Fatalf("after remove: %+v", s.config.FoAuth)
	}
	if code, _ := proxyCall(s, key); code != http.StatusNotFound {
		t.Fatalf("proxy after remove: %d", code)
	}
}

func TestFoRefreshRejectedNeedsReconnect(t *testing.T) {
	t.Chdir(t.TempDir())
	fakeWajo(t)
	gw := &mcpGateway{written: map[string]map[string]any{}}
	cfg := &config.Config{
		MCPTools: []config.MCPTool{{Name: foToolName, URL: "http://127.0.0.1:5000/api/fo-mcp"}},
		FoAuth: config.FoAuth{ClientID: "client-1", AccessToken: "at-old", RefreshToken: "rt-dead",
			ExpiresAt: time.Now().Add(time.Minute).Unix(), ProxyKey: "k"},
	}
	s := &Service{config: cfg, agentGateway: gw}

	if st := s.GetFoStatus(); !st.Connected || st.NeedsReconnect {
		t.Fatalf("before: %+v", st)
	}
	// Wajo answers invalid_grant (400): the sign-in is dead.
	if code, _ := proxyCall(s, "k"); code != http.StatusUnauthorized {
		t.Fatalf("dead refresh: %d", code)
	}
	if st := s.GetFoStatus(); !st.Connected || !st.NeedsReconnect {
		t.Fatalf("after: %+v", st)
	}
	if cfg.FoAuth.RefreshToken != "" || cfg.FoAuth.ProxyKey != "k" {
		t.Fatalf("tokens: %+v", cfg.FoAuth)
	}

	// Reconnect renews the tokens without rewriting the MCP entry (no runtime restart).
	authURL, err := s.FoConnectStart()
	if err != nil {
		t.Fatal(err)
	}
	q, _ := url.ParseQuery(strings.SplitN(authURL, "?", 2)[1])
	if err := s.FoConnectFinish(foRedirectURI + "?code=code-1&state=" + q.Get("state")); err != nil {
		t.Fatal(err)
	}
	if st := s.GetFoStatus(); st.NeedsReconnect || len(gw.written) != 0 {
		t.Fatalf("reconnect: %+v writes=%d", st, len(gw.written))
	}
}

func TestFoRefreshServerErrorKeepsSignIn(t *testing.T) {
	t.Chdir(t.TempDir())
	down := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Error(w, "upstream down", http.StatusBadGateway)
	}))
	t.Cleanup(down.Close)
	orig := foOAuthBase
	foOAuthBase = down.URL
	t.Cleanup(func() { foOAuthBase = orig })

	cfg := &config.Config{
		MCPTools: []config.MCPTool{{Name: foToolName}},
		FoAuth: config.FoAuth{ClientID: "c", AccessToken: "at", RefreshToken: "rt",
			ExpiresAt: time.Now().Add(time.Minute).Unix(), ProxyKey: "k"},
	}
	s := &Service{config: cfg}
	if _, err := s.foAccessToken(); err == nil {
		t.Fatal("refresh against a 502 succeeded")
	}
	if cfg.FoAuth.RefreshToken != "rt" || s.GetFoStatus().NeedsReconnect {
		t.Fatalf("transient error dropped the sign-in: %+v", cfg.FoAuth)
	}
}
