package mqtthandler

import (
	"context"
	"encoding/json"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/gin-gonic/gin"

	"go.autonomous.ai/os/system/server/config"
)

const (
	spSecret  = "amzn-client-secret-VALUE-123"
	spRefresh = "Atzr|refresh-token-VALUE-456"
	spToken   = "Atza|minted-access-token-789"
)

func newAmazonSPTestEnv(t *testing.T, rt *recordingTransport) *shopifyTestEnv {
	t.Helper()
	gin.SetMode(gin.TestMode)
	dir := t.TempDir()
	var pubs []published
	h := &DeviceMQTTHandler{
		config:          &config.Config{},
		connectorWriter: newConnectorWriter(dir, &fakeMCPGateway{}, nil),
		amazonSPHTTP: &http.Client{
			Transport:     rt,
			CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse },
		},
		publishConnectorResult: func(kind, status, _ string, data interface{}) error {
			m, _ := data.(map[string]any)
			pubs = append(pubs, published{kind: kind, status: status, data: m})
			return nil
		},
	}
	r := gin.New()
	g := r.Group("/api/device/connectors")
	g.GET("amazon_sp", h.GetAmazonSP)
	g.PUT("amazon_sp", h.SetAmazonSP)
	g.DELETE("amazon_sp", h.RemoveAmazonSP)
	g.GET(":code", h.GetConnector)
	return &shopifyTestEnv{h: h, router: r, dir: dir, rt: rt, pubs: &pubs}
}

func okLWA() *recordingTransport {
	return &recordingTransport{status: 200, body: `{"access_token":"` + spToken + `","token_type":"bearer","expires_in":3600}`}
}

func spBody() map[string]string {
	return map[string]string{"region": "na", "client_id": "amzn1.application-oa2-client.abc", "client_secret": spSecret, "refresh_token": spRefresh}
}

func noSPSecrets(t *testing.T, what, s string) {
	t.Helper()
	for _, secret := range []string{spSecret, spRefresh, spToken} {
		if strings.Contains(s, secret) {
			t.Errorf("%s leaks a secret: %s", what, s)
		}
	}
}

func TestSetAmazonSP_HappyPath(t *testing.T) {
	e := newAmazonSPTestEnv(t, okLWA())
	cache := filepath.Join(e.dir, amazonSPTokenCacheFile)
	if err := os.WriteFile(cache, []byte(`{"stale":true}`), 0o600); err != nil {
		t.Fatal(err)
	}
	w := e.do(http.MethodPut, "/api/device/connectors/amazon_sp", spBody())
	if w.Code != http.StatusOK {
		t.Fatalf("status %d: %s", w.Code, w.Body.String())
	}
	noSPSecrets(t, "response", w.Body.String())

	if len(e.rt.urls) != 1 || e.rt.urls[0] != amazonLWATokenURL {
		t.Fatalf("the check must go only to Login with Amazon, went to %v", e.rt.urls)
	}
	f := e.rt.forms[0]
	if f.Get("grant_type") != "refresh_token" || f.Get("refresh_token") != spRefresh || f.Get("client_secret") != spSecret || f.Get("client_id") == "" {
		t.Errorf("form = %v", f)
	}
	creds, ok, err := e.h.connectorWriter.loadEntry("amazon_sp")
	if err != nil || !ok {
		t.Fatalf("loadEntry: ok=%v err=%v", ok, err)
	}
	if creds.AuthType != "pat" || creds.Refresh || creds.AccessToken != spToken || creds.ExpiresAt == 0 {
		t.Errorf("entry = %+v", creds)
	}
	c := creds.Credentials
	if c["api_host"] != "sellingpartnerapi-na.amazon.com" || c["region"] != "na" || c["client_secret"] != spSecret || c["refresh_token"] != spRefresh {
		t.Errorf("credentials = %+v", c)
	}
	if _, err := os.Stat(cache); !os.IsNotExist(err) {
		t.Errorf("stale token cache should be removed, stat err = %v", err)
	}
	if len(*e.pubs) != 1 || (*e.pubs)[0].kind != "connector.set.amazon_sp" {
		t.Fatalf("published = %+v", *e.pubs)
	}
	raw, _ := json.Marshal((*e.pubs)[0].data)
	noSPSecrets(t, "published data", string(raw))
}

func TestSetAmazonSP_RegionsMapToTheirOwnHost(t *testing.T) {
	for region, host := range map[string]string{"na": "sellingpartnerapi-na.amazon.com", "eu": "sellingpartnerapi-eu.amazon.com", "fe": "sellingpartnerapi-fe.amazon.com"} {
		e := newAmazonSPTestEnv(t, okLWA())
		b := spBody()
		b["region"] = " " + strings.ToUpper(region) + " "
		if w := e.do(http.MethodPut, "/api/device/connectors/amazon_sp", b); w.Code != http.StatusOK {
			t.Fatalf("%s: %d %s", region, w.Code, w.Body.String())
		}
		creds, _, _ := e.h.connectorWriter.loadEntry("amazon_sp")
		if creds.Credentials["api_host"] != host {
			t.Errorf("%s -> %q, want %q", region, creds.Credentials["api_host"], host)
		}
	}
}

func TestSetAmazonSP_InvalidInputSendsNothing(t *testing.T) {
	bad := []map[string]string{}
	for _, mutate := range []func(map[string]string){
		func(b map[string]string) { b["region"] = "" },
		func(b map[string]string) { b["region"] = "us-east-1" },
		func(b map[string]string) { b["region"] = "evil.example" },
		func(b map[string]string) { b["client_id"] = "" },
		func(b map[string]string) { b["client_secret"] = "has space" },
		func(b map[string]string) { b["refresh_token"] = "" },
		func(b map[string]string) { b["refresh_token"] = "line\nbreak" },
		func(b map[string]string) { b["client_secret"] = strings.Repeat("a", maxAmazonSPFieldLen+1) },
	} {
		b := spBody()
		mutate(b)
		bad = append(bad, b)
	}
	e := newAmazonSPTestEnv(t, okLWA())
	for _, b := range bad {
		w := e.do(http.MethodPut, "/api/device/connectors/amazon_sp", b)
		if w.Code != http.StatusBadRequest {
			t.Errorf("%v -> %d, want 400", b, w.Code)
		}
		noSPSecrets(t, "error body", w.Body.String())
	}
	if len(e.rt.urls) != 0 {
		t.Fatalf("nothing must be sent for invalid input, sent %v", e.rt.urls)
	}
	if _, ok, _ := e.h.connectorWriter.loadEntry("amazon_sp"); ok {
		t.Fatal("nothing must be saved for invalid input")
	}
}

func TestSetAmazonSP_RejectedAndUnreachableAreDistinctAndNeverSave(t *testing.T) {
	for _, tc := range []struct {
		name   string
		status int
		code   int
		want   string
	}{
		{"invalid_grant", 400, http.StatusBadRequest, "rejected the client ID"},
		{"invalid_client", 401, http.StatusBadRequest, "rejected the client ID"},
		{"server error", 503, http.StatusBadGateway, "Couldn't reach Amazon"},
		{"redirect", 302, http.StatusBadGateway, "Couldn't reach Amazon"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			e := newAmazonSPTestEnv(t, &recordingTransport{status: tc.status, body: `{"error":"x","error_description":"` + spSecret + `"}`})
			w := e.do(http.MethodPut, "/api/device/connectors/amazon_sp", spBody())
			if w.Code != tc.code || !strings.Contains(w.Body.String(), tc.want) {
				t.Fatalf("got %d %s", w.Code, w.Body.String())
			}
			noSPSecrets(t, "error body", w.Body.String())
			if _, ok, _ := e.h.connectorWriter.loadEntry("amazon_sp"); ok {
				t.Fatal("a failed check must not save anything")
			}
		})
	}
}

func TestSetAmazonSP_UnusableTokenResponseIsNotSaved(t *testing.T) {
	for _, body := range []string{`{}`, `not json`, `{"access_token":""}`} {
		e := newAmazonSPTestEnv(t, &recordingTransport{status: 200, body: body})
		if w := e.do(http.MethodPut, "/api/device/connectors/amazon_sp", spBody()); w.Code != http.StatusBadGateway {
			t.Errorf("%q -> %d", body, w.Code)
		}
	}
}

func TestMintAmazonSPToken_LifetimeIsBounded(t *testing.T) {
	for in, want := range map[string]int{`0`: amazonSPDefaultLifetime, `5`: amazonSPMinLifetime, `999999999`: amazonSPMaxLifetime, `1800`: 1800} {
		rt := &recordingTransport{status: 200, body: `{"access_token":"t","expires_in":` + in + `}`}
		tok, err := mintAmazonSPToken(httptestCtx(t), &http.Client{Transport: rt}, "id", "s", "r")
		if err != nil || tok.ExpiresIn != want {
			t.Errorf("expires_in %s -> %d (%v), want %d", in, tok.ExpiresIn, err, want)
		}
	}
}

func TestGetAndRemoveAmazonSP_NeverReturnSecrets(t *testing.T) {
	e := newAmazonSPTestEnv(t, okLWA())
	if w := e.do(http.MethodGet, "/api/device/connectors/amazon_sp", nil); !strings.Contains(w.Body.String(), `"connected":false`) {
		t.Fatalf("not linked: %s", w.Body.String())
	}
	e.do(http.MethodPut, "/api/device/connectors/amazon_sp", spBody())

	w := e.do(http.MethodGet, "/api/device/connectors/amazon_sp", nil)
	body := w.Body.String()
	noSPSecrets(t, "status", body)
	for _, want := range []string{`"connected":true`, `"has_secret":true`, `"has_refresh_token":true`, `"region":"na"`} {
		if !strings.Contains(body, want) {
			t.Errorf("status misses %s: %s", want, body)
		}
	}
	// The generic route redacts the secret fields too.
	w = e.do(http.MethodGet, "/api/device/connectors/amazon_sp", nil)
	noSPSecrets(t, "generic status", w.Body.String())

	cache := filepath.Join(e.dir, amazonSPTokenCacheFile)
	_ = os.WriteFile(cache, []byte(`{}`), 0o600)
	if w := e.do(http.MethodDelete, "/api/device/connectors/amazon_sp", nil); w.Code != http.StatusOK {
		t.Fatalf("delete: %d %s", w.Code, w.Body.String())
	}
	if _, ok, _ := e.h.connectorWriter.loadEntry("amazon_sp"); ok {
		t.Fatal("the entry must be gone")
	}
	if _, err := os.Stat(cache); !os.IsNotExist(err) {
		t.Error("the helper's token cache must be removed with the connection")
	}
	if last := (*e.pubs)[len(*e.pubs)-1]; last.kind != "connector.remove.amazon_sp" {
		t.Errorf("published %+v", last)
	}
}

func TestAmazonSP_ReservedAndDoesNotCollideWithTheMCPConnector(t *testing.T) {
	// "amazon" (MCP) and "amazon_sp" (direct) are separate connectors with separate files.
	dir := t.TempDir()
	w := newConnectorWriter(dir, &fakeMCPGateway{}, nil)
	a, _ := w.pathFor("amazon")
	b, _ := w.pathFor("amazon_sp")
	if a == b || filepath.Base(b) != "amazon_sp_access_tokens.json" {
		t.Fatalf("paths: %s %s", a, b)
	}
}

func httptestCtx(t *testing.T) context.Context {
	t.Helper()
	return context.Background()
}

func TestNormalizeAmazonSellerID(t *testing.T) {
	for in, want := range map[string]string{"": "", "  a1b2c3d4e5f6g7 ": "A1B2C3D4E5F6G7", "A3XYZ12345678": "A3XYZ12345678"} {
		got, err := normalizeAmazonSellerID(in)
		if err != nil || got != want {
			t.Errorf("%q -> %q (%v), want %q", in, got, err, want)
		}
	}
	for _, bad := range []string{"short", "has space in it1", "A1B2C3D4-E5F6", "https://sellercentral.amazon.com", strings.Repeat("A", 21), "A1B2C3D4\nE5F6G7"} {
		if _, err := normalizeAmazonSellerID(bad); err == nil {
			t.Errorf("%q must be rejected", bad)
		}
	}
}

func TestSetAmazonSP_SavesTheSellerID(t *testing.T) {
	e := newAmazonSPTestEnv(t, okLWA())
	b := spBody()
	b["seller_id"] = " a1b2c3d4e5f6g7 "
	w := e.do(http.MethodPut, "/api/device/connectors/amazon_sp", b)
	if w.Code != http.StatusOK || !strings.Contains(w.Body.String(), `"seller_id":"A1B2C3D4E5F6G7"`) {
		t.Fatalf("%d %s", w.Code, w.Body.String())
	}
	creds, _, _ := e.h.connectorWriter.loadEntry("amazon_sp")
	if creds.Credentials["seller_id"] != "A1B2C3D4E5F6G7" {
		t.Errorf("saved %q", creds.Credentials["seller_id"])
	}
	if g := e.do(http.MethodGet, "/api/device/connectors/amazon_sp", nil); !strings.Contains(g.Body.String(), `"seller_id":"A1B2C3D4E5F6G7"`) {
		t.Errorf("GET misses the seller id: %s", g.Body.String())
	}
}

func TestSetAmazonSP_InvalidSellerIDSendsNothing(t *testing.T) {
	e := newAmazonSPTestEnv(t, okLWA())
	b := spBody()
	b["seller_id"] = "not valid!"
	if w := e.do(http.MethodPut, "/api/device/connectors/amazon_sp", b); w.Code != http.StatusBadRequest {
		t.Fatalf("got %d", w.Code)
	}
	if len(e.rt.urls) != 0 {
		t.Fatal("nothing may be sent for an invalid Seller ID")
	}
}

func TestSetAmazonSP_SellerIDAloneUpdatesWithoutTouchingTheSecrets(t *testing.T) {
	e := newAmazonSPTestEnv(t, okLWA())
	e.do(http.MethodPut, "/api/device/connectors/amazon_sp", spBody())
	before := len(e.rt.urls)

	w := e.do(http.MethodPut, "/api/device/connectors/amazon_sp", map[string]string{"seller_id": "a1b2c3d4e5f6g7"})
	if w.Code != http.StatusOK {
		t.Fatalf("%d %s", w.Code, w.Body.String())
	}
	noSPSecrets(t, "update response", w.Body.String())
	if len(e.rt.urls) != before {
		t.Fatal("updating only the Seller ID must not call Amazon again")
	}
	c, _, _ := e.h.connectorWriter.loadEntry("amazon_sp")
	if c.Credentials["seller_id"] != "A1B2C3D4E5F6G7" || c.Credentials["client_secret"] != spSecret || c.Credentials["refresh_token"] != spRefresh || c.Credentials["region"] != "na" {
		t.Errorf("credentials after the update: %+v", c.Credentials)
	}
	// Reconnecting without a Seller ID keeps no stale one.
	e.do(http.MethodPut, "/api/device/connectors/amazon_sp", spBody())
	if c2, _, _ := e.h.connectorWriter.loadEntry("amazon_sp"); c2.Credentials["seller_id"] != "" {
		t.Errorf("a reconnect without a Seller ID should not keep the old one, got %q", c2.Credentials["seller_id"])
	}
}

func TestSetAmazonSP_SellerIDAloneNeedsAConnection(t *testing.T) {
	e := newAmazonSPTestEnv(t, okLWA())
	w := e.do(http.MethodPut, "/api/device/connectors/amazon_sp", map[string]string{"seller_id": "A1B2C3D4E5F6G7"})
	if w.Code != http.StatusBadRequest || !strings.Contains(w.Body.String(), "Connect Amazon first") {
		t.Fatalf("%d %s", w.Code, w.Body.String())
	}
}
