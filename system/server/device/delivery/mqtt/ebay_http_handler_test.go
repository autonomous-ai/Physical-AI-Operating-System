package mqtthandler

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/gin-gonic/gin"

	"go.autonomous.ai/os/system/server/config"
)

const (
	testEbaySecret  = "SBX-cert-secret-value"
	testEbayRefresh = "v^1.1#i^1#refresh-token-value"
	testEbayAccess  = "v^1.1#i^1#access-token-value"
	testEbayCode    = "v^1.1#i^1#f^0#code+value=="
)

func newEbayTestEnv(t *testing.T, rt *recordingTransport) (*gin.Engine, string) {
	t.Helper()
	gin.SetMode(gin.TestMode)
	dir := t.TempDir()
	h := &DeviceMQTTHandler{
		config:          &config.Config{},
		connectorWriter: newConnectorWriter(dir, &fakeMCPGateway{}, nil),
		ebayHTTP: &http.Client{
			Transport:     rt,
			CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse },
		},
		publishConnectorResult: func(string, string, string, interface{}) error { return nil },
	}
	r := gin.New()
	g := r.Group("/api/device/connectors")
	g.PUT("ebay", h.SetEbay)
	g.GET(":code", h.GetConnector)
	return r, dir
}

func putEbay(r *gin.Engine, body map[string]string) *httptest.ResponseRecorder {
	var buf bytes.Buffer
	_ = json.NewEncoder(&buf).Encode(body)
	req := httptest.NewRequest(http.MethodPut, "/api/device/connectors/ebay", &buf)
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	r.ServeHTTP(w, req)
	return w
}

func ebayCodeBody() map[string]string {
	return map[string]string{
		"environment": "sandbox", "marketplace_id": "EBAY_US",
		"client_id": "Me-App-SBX-123", "client_secret": testEbaySecret, "ru_name": "Me-App-SBX-ru",
		"code": "https://autonomous.ai/?state=x&code=" + strings.ReplaceAll(strings.ReplaceAll(testEbayCode, "#", "%23"), "+", "%2B") + "&expires_in=299",
	}
}

func readEbayEntry(t *testing.T, dir string) map[string]any {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join(dir, "ebay_access_tokens.json"))
	if err != nil {
		t.Fatalf("read entry: %v", err)
	}
	var file struct {
		Connectors map[string]map[string]any `json:"connectors"`
	}
	if err := json.Unmarshal(raw, &file); err != nil {
		t.Fatalf("parse entry: %v", err)
	}
	return file.Connectors["ebay"]
}

func TestEbayCodeFrom(t *testing.T) {
	cases := map[string]string{
		"https://autonomous.ai/?code=v%5E1.1%23abc&expires_in=299": "v^1.1#abc",
		"autonomous.ai?code=abc":                                   "abc",
		"  v%5E1.1%23abc  ":                                        "v^1.1#abc",
		"v^1.1#abc":                                                "v^1.1#abc",
		"https://autonomous.ai/?state=x":                           "",
	}
	for in, want := range cases {
		if got := ebayCodeFrom(in); got != want {
			t.Errorf("ebayCodeFrom(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestSetEbay_ExchangesCodeOnSandboxAndSavesRefreshToken(t *testing.T) {
	rt := &recordingTransport{status: 200, body: `{"access_token":"` + testEbayAccess + `","expires_in":7200,"refresh_token":"` + testEbayRefresh + `","refresh_token_expires_in":47304000}`}
	r, dir := newEbayTestEnv(t, rt)
	if err := os.WriteFile(filepath.Join(dir, ebayTokenCacheFile), []byte(`{}`), 0o600); err != nil {
		t.Fatal(err)
	}

	w := putEbay(r, ebayCodeBody())
	if w.Code != http.StatusOK {
		t.Fatalf("status %d: %s", w.Code, w.Body.String())
	}
	if len(rt.urls) != 1 || rt.urls[0] != "https://api.sandbox.ebay.com/identity/v1/oauth2/token" {
		t.Fatalf("token request went to %v", rt.urls)
	}
	form := rt.forms[0]
	if form.Get("grant_type") != "authorization_code" || form.Get("code") != testEbayCode || form.Get("redirect_uri") != "Me-App-SBX-ru" {
		t.Fatalf("unexpected exchange form: %v", form)
	}
	entry := readEbayEntry(t, dir)
	if entry["api_key"] != testEbayRefresh {
		t.Fatalf("refresh token not saved as api_key: %v", entry["api_key"])
	}
	creds, _ := entry["credentials"].(map[string]any)
	if creds["environment"] != "sandbox" || creds["client_secret"] != testEbaySecret || creds["ru_name"] != "Me-App-SBX-ru" {
		t.Fatalf("unexpected credentials: %v", creds)
	}
	if _, err := os.Stat(filepath.Join(dir, ebayTokenCacheFile)); !os.IsNotExist(err) {
		t.Fatalf("token cache not dropped: %v", err)
	}
	for _, secret := range []string{testEbaySecret, testEbayRefresh, testEbayAccess} {
		if strings.Contains(w.Body.String(), secret) {
			t.Errorf("response leaks a secret: %s", w.Body.String())
		}
	}

	get := httptest.NewRecorder()
	r.ServeHTTP(get, httptest.NewRequest(http.MethodGet, "/api/device/connectors/ebay", nil))
	if strings.Contains(get.Body.String(), testEbaySecret) || !strings.Contains(get.Body.String(), "Me-App-SBX-123") {
		t.Fatalf("GET should show the client ID but never the secret: %s", get.Body.String())
	}
}

func TestSetEbay_PastedRefreshTokenIsChecked(t *testing.T) {
	rt := &recordingTransport{status: 200, body: `{"access_token":"` + testEbayAccess + `","expires_in":7200}`}
	r, dir := newEbayTestEnv(t, rt)
	body := ebayCodeBody()
	delete(body, "code")
	body["environment"] = "production"
	body["refresh_token"] = testEbayRefresh

	if w := putEbay(r, body); w.Code != http.StatusOK {
		t.Fatalf("status %d: %s", w.Code, w.Body.String())
	}
	if rt.urls[0] != "https://api.ebay.com/identity/v1/oauth2/token" || rt.forms[0].Get("grant_type") != "refresh_token" {
		t.Fatalf("unexpected check: %v %v", rt.urls, rt.forms)
	}
	if readEbayEntry(t, dir)["api_key"] != testEbayRefresh {
		t.Fatal("pasted refresh token not saved")
	}
}

func TestSetEbay_RejectedCodeSavesNothing(t *testing.T) {
	rt := &recordingTransport{status: 400, body: `{"error":"invalid_grant"}`}
	r, dir := newEbayTestEnv(t, rt)
	w := putEbay(r, ebayCodeBody())
	if w.Code != http.StatusBadRequest || !strings.Contains(w.Body.String(), "five minutes") {
		t.Fatalf("status %d: %s", w.Code, w.Body.String())
	}
	if _, err := os.Stat(filepath.Join(dir, "ebay_access_tokens.json")); !os.IsNotExist(err) {
		t.Fatal("a rejected sign-in must not save anything")
	}
}

func TestSetEbay_ValidatesBeforeCallingEbay(t *testing.T) {
	cases := map[string]func(map[string]string){
		"bad environment": func(b map[string]string) { b["environment"] = "staging" },
		"no secret":       func(b map[string]string) { b["client_secret"] = "" },
		"no RuName":       func(b map[string]string) { b["ru_name"] = "" },
		"no code":         func(b map[string]string) { b["code"] = "https://autonomous.ai/?state=x" },
		"nothing to use":  func(b map[string]string) { delete(b, "code") },
	}
	for name, mutate := range cases {
		rt := &recordingTransport{status: 200, body: `{}`}
		r, _ := newEbayTestEnv(t, rt)
		body := ebayCodeBody()
		mutate(body)
		if w := putEbay(r, body); w.Code != http.StatusBadRequest {
			t.Errorf("%s: status %d, want 400", name, w.Code)
		}
		if len(rt.urls) != 0 {
			t.Errorf("%s: called eBay before validating", name)
		}
	}
}
