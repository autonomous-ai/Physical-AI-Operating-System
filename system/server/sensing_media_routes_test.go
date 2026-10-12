package server

import (
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/gin-gonic/gin"
	"go.autonomous.ai/os/system/server/config"
	sensinghttp "go.autonomous.ai/os/system/server/sensing/delivery/http"
)

func TestSensingMediaRoutesNeedTheOwner(t *testing.T) {
	gin.SetMode(gin.TestMode)
	s := &Server{
		config:         &config.Config{LLMAPIKey: "test-owner-token"},
		sensingHandler: &sensinghttp.SensingHandler{},
	}
	router := gin.New()
	s.registerSensingMediaRoutes(router.Group("/api/sensing"))

	for _, path := range []string{
		"/api/sensing/snapshot/sensing_bearing/20261012-195058.jpg",
		"/api/sensing/agent-snapshot/hermes/media-hal-snapshots/snap_1791814884873.jpg",
		"/api/sensing/audio/incoming_1791814381477.wav",
	} {
		for _, tc := range []struct {
			name, auth string
			allowed    bool
		}{
			{"no token", "", false},
			{"wrong token", "Bearer nope", false},
			{"owner token", "Bearer test-owner-token", true},
		} {
			req := httptest.NewRequest(http.MethodGet, path, nil)
			req.RemoteAddr = "192.168.1.20:1234"
			if tc.auth != "" {
				req.Header.Set("Authorization", tc.auth)
			}
			out := httptest.NewRecorder()
			router.ServeHTTP(out, req)
			if denied := out.Code == http.StatusUnauthorized; denied == tc.allowed {
				t.Errorf("%s %s: status %d", tc.name, path, out.Code)
			}
		}
	}
}
