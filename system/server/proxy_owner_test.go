package server

import (
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/gin-gonic/gin"

	"go.autonomous.ai/os/system/server/config"
)

func TestOwnerActionHeaderOnlyForLoggedInOwner(t *testing.T) {
	gin.SetMode(gin.TestMode)
	cfg := &config.Config{LLMAPIKey: "llm-key", SessionSecret: "00112233445566778899aabbccddeeff"}

	run := func(req *http.Request) string {
		var got string
		r := gin.New()
		r.POST("/x", ownerActionHeader(cfg), func(c *gin.Context) { got = c.Request.Header.Get(halOwnerActionHeader) })
		r.ServeHTTP(httptest.NewRecorder(), req)
		return got
	}

	forged := httptest.NewRequest(http.MethodPost, "/x", nil)
	forged.Header.Set(halOwnerActionHeader, "1")
	forged.Header.Set("Authorization", "Bearer llm-key") // scripts and the agent
	if got := run(forged); got != "" {
		t.Fatalf("LLM-key caller kept a forged owner header: %q", got)
	}
}
