package mqtthandler

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/gin-gonic/gin"

	"go.autonomous.ai/os/system/server/config"
)

type amazonTestEnv struct {
	h      *DeviceMQTTHandler
	router *gin.Engine
	calls  *[][]string
}

// newAmazonTestEnv wires the same routes as the server, with a fake helper that answers from `state`.
func newAmazonTestEnv(t *testing.T, state *[]amazonToolkitSkill, fail map[string]error) *amazonTestEnv {
	t.Helper()
	gin.SetMode(gin.TestMode)
	var calls [][]string
	var mu sync.Mutex
	h := &DeviceMQTTHandler{
		config:          &config.Config{},
		connectorWriter: newConnectorWriter(t.TempDir(), &fakeMCPGateway{}, nil),
	}
	h.amazonToolkitRun = func(_ context.Context, args ...string) ([]byte, error) {
		mu.Lock()
		defer mu.Unlock()
		calls = append(calls, args)
		if err, ok := fail[args[0]]; ok {
			return nil, err
		}
		switch args[0] {
		case "install":
			*state = []amazonToolkitSkill{{Skill: "amazon-seller-analytics", Commit: "abc"}, {Skill: "amazon-listing-issues", Commit: "abc"}}
		case "remove":
			*state = nil
		}
		return json.Marshal(map[string]any{"installed": *state, "pinned_commit": "abc"})
	}
	r := gin.New()
	g := r.Group("/api/device/connectors")
	g.GET("amazon/toolkit", h.GetAmazonToolkit)
	g.POST("amazon/toolkit", h.SetAmazonToolkit)
	g.GET(":code", h.GetConnector)
	g.DELETE(":code", h.RemoveConnector)
	return &amazonTestEnv{h: h, router: r, calls: &calls}
}

func (e *amazonTestEnv) do(method, path string, body any) *httptest.ResponseRecorder {
	var buf bytes.Buffer
	if body != nil {
		_ = json.NewEncoder(&buf).Encode(body)
	}
	req := httptest.NewRequest(method, path, &buf)
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	e.router.ServeHTTP(w, req)
	return w
}

func installedFrom(t *testing.T, w *httptest.ResponseRecorder) amazonToolkitResponse {
	t.Helper()
	var env struct {
		Data amazonToolkitResponse `json:"data"`
	}
	if err := json.Unmarshal(w.Body.Bytes(), &env); err != nil {
		t.Fatalf("decode %q: %v", w.Body.String(), err)
	}
	return env.Data
}

func TestAmazonToolkit_GetListsWhatIsInstalled(t *testing.T) {
	state := []amazonToolkitSkill{{Skill: "amazon-seller-analytics", Commit: "abc"}}
	e := newAmazonTestEnv(t, &state, nil)
	w := e.do(http.MethodGet, "/api/device/connectors/amazon/toolkit", nil)
	if w.Code != http.StatusOK {
		t.Fatalf("status = %d body=%s", w.Code, w.Body.String())
	}
	res := installedFrom(t, w)
	if !res.Available || len(res.Installed) != 1 || res.Installed[0].Skill != "amazon-seller-analytics" || res.PinnedCommit != "abc" {
		t.Fatalf("unexpected response: %+v", res)
	}
}

func TestAmazonToolkit_NothingInstalledIsAnEmptyListNotNull(t *testing.T) {
	var state []amazonToolkitSkill
	e := newAmazonTestEnv(t, &state, nil)
	w := e.do(http.MethodGet, "/api/device/connectors/amazon/toolkit", nil)
	if !strings.Contains(w.Body.String(), `"installed":[]`) {
		t.Fatalf("installed must be [] for the page, got %s", w.Body.String())
	}
}

func TestAmazonToolkit_MissingHelperIsReportedNotAnError(t *testing.T) {
	var state []amazonToolkitSkill
	e := newAmazonTestEnv(t, &state, map[string]error{"status": errAmazonToolkitUnavailable, "install": errAmazonToolkitUnavailable})
	w := e.do(http.MethodGet, "/api/device/connectors/amazon/toolkit", nil)
	if w.Code != http.StatusOK || installedFrom(t, w).Available {
		t.Fatalf("want 200 with available=false, got %d %s", w.Code, w.Body.String())
	}
	w = e.do(http.MethodPost, "/api/device/connectors/amazon/toolkit", map[string]string{"action": "install"})
	if w.Code != http.StatusServiceUnavailable {
		t.Fatalf("install without the helper = %d, want 503", w.Code)
	}
}

func TestAmazonToolkit_PostValidatesTheAction(t *testing.T) {
	var state []amazonToolkitSkill
	e := newAmazonTestEnv(t, &state, nil)
	for _, body := range []any{map[string]string{"action": ""}, map[string]string{"action": "reinstall"}, map[string]string{"action": "install; rm -rf /"}, "not an object"} {
		w := e.do(http.MethodPost, "/api/device/connectors/amazon/toolkit", body)
		if w.Code != http.StatusBadRequest {
			t.Fatalf("body %v -> %d, want 400", body, w.Code)
		}
	}
	if len(*e.calls) != 0 {
		t.Fatalf("the helper must not run for a bad request, ran %v", *e.calls)
	}
}

func TestAmazonToolkit_InstallThenRemove(t *testing.T) {
	var state []amazonToolkitSkill
	e := newAmazonTestEnv(t, &state, nil)

	w := e.do(http.MethodPost, "/api/device/connectors/amazon/toolkit", map[string]string{"action": "install"})
	if w.Code != http.StatusOK || len(installedFrom(t, w).Installed) != 2 {
		t.Fatalf("install: %d %s", w.Code, w.Body.String())
	}
	if got := strings.Join((*e.calls)[0], " "); got != "install --confirmed" {
		t.Fatalf("install must pass --confirmed (the click is the confirmation), ran %q", got)
	}

	w = e.do(http.MethodPost, "/api/device/connectors/amazon/toolkit", map[string]string{"action": "remove"})
	if w.Code != http.StatusOK || len(installedFrom(t, w).Installed) != 0 {
		t.Fatalf("remove: %d %s", w.Code, w.Body.String())
	}
}

func TestAmazonToolkit_HelperFailureIsSurfacedAndCapped(t *testing.T) {
	var state []amazonToolkitSkill
	e := newAmazonTestEnv(t, &state, map[string]error{"install": errors.New("download failed: network unreachable")})
	w := e.do(http.MethodPost, "/api/device/connectors/amazon/toolkit", map[string]string{"action": "install"})
	if w.Code != http.StatusBadGateway || !strings.Contains(w.Body.String(), "network unreachable") {
		t.Fatalf("want 502 with the reason, got %d %s", w.Code, w.Body.String())
	}
}

func TestAmazonToolkit_InstallsDoNotOverlap(t *testing.T) {
	var state []amazonToolkitSkill
	e := newAmazonTestEnv(t, &state, nil)
	var active, peak int32
	e.h.amazonToolkitRun = func(_ context.Context, args ...string) ([]byte, error) {
		if args[0] == "install" {
			n := atomic.AddInt32(&active, 1)
			for {
				p := atomic.LoadInt32(&peak)
				if n <= p || atomic.CompareAndSwapInt32(&peak, p, n) {
					break
				}
			}
			time.Sleep(40 * time.Millisecond)
			atomic.AddInt32(&active, -1)
		}
		return []byte(`{"installed":[]}`), nil
	}
	var wg sync.WaitGroup
	for i := 0; i < 4; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			e.do(http.MethodPost, "/api/device/connectors/amazon/toolkit", map[string]string{"action": "install"})
		}()
	}
	wg.Wait()
	if peak != 1 {
		t.Fatalf("installs overlapped: peak concurrency %d", peak)
	}
}

func TestAmazonToolkit_ScriptCandidatesPreferHermesThenOpenclaw(t *testing.T) {
	got := amazonToolkitScriptCandidates("/root/.openclaw")
	if len(got) != 2 || !strings.HasPrefix(got[0], "/root/.hermes/skills/openclaw-imports/") ||
		got[1] != filepath.Join("/root/.openclaw", "workspace", "skills", "connectors", "scripts", "amazon_toolkit.py") {
		t.Fatalf("unexpected candidates: %v", got)
	}
}

func TestAmazonToolkit_RealRunnerReportsAMissingHelper(t *testing.T) {
	h := &DeviceMQTTHandler{config: &config.Config{OpenclawConfigDir: t.TempDir()}}
	if _, err := h.runAmazonToolkitScript(context.Background(), "status"); !errors.Is(err, errAmazonToolkitUnavailable) {
		t.Fatalf("want errAmazonToolkitUnavailable, got %v", err)
	}
}

// The page reads connection state from the generic route: it must show connected without ever returning the token.
func TestAmazonConnectionStatus_NeverReturnsTheToken(t *testing.T) {
	var state []amazonToolkitSkill
	e := newAmazonTestEnv(t, &state, nil)

	w := e.do(http.MethodGet, "/api/device/connectors/amazon", nil)
	if w.Code != http.StatusOK || !strings.Contains(w.Body.String(), `"connected":false`) {
		t.Fatalf("not linked: %d %s", w.Code, w.Body.String())
	}

	const secret = "Atza|amazon-access-token-value"
	const refresh = "Atzr|amazon-refresh-token-value"
	if err := e.h.connectorWriter.Write(context.Background(), ConnectorCreds{
		Connector: "amazon", AuthType: "oauth", AccessToken: secret, RefreshToken: refresh, ObtainedAt: 1760000000,
	}); err != nil {
		t.Fatalf("write: %v", err)
	}
	w = e.do(http.MethodGet, "/api/device/connectors/amazon", nil)
	body := w.Body.String()
	if !strings.Contains(body, `"connected":true`) {
		t.Fatalf("linked: %s", body)
	}
	if strings.Contains(body, secret) || strings.Contains(body, refresh) {
		t.Fatalf("a token leaked into the status response: %s", body)
	}
}
