package mqtthandler

import (
	"context"
	"encoding/json"
	"errors"
	"log/slog"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/gin-gonic/gin"

	"go.autonomous.ai/os/system/server/serializers"
)

// amazonToolkitTimeout bounds one run of the helper (a download of about 400 KB plus a few file writes).
const amazonToolkitTimeout = 90 * time.Second

// amazonToolkitErrMax caps the helper's message that is echoed back to the local page.
const amazonToolkitErrMax = 300

// errAmazonToolkitUnavailable means the connectors skill (which ships the helper) is not on this device.
var errAmazonToolkitUnavailable = errors.New("the connectors skill is not installed on this device")

// amazonToolkitMu serializes installs and removals: two clicks must not write the same folders at once.
var amazonToolkitMu sync.Mutex

// amazonToolkitRunner runs one helper command and returns its stdout. Tests replace it.
type amazonToolkitRunner func(ctx context.Context, args ...string) ([]byte, error)

type amazonToolkitSkill struct {
	Skill  string `json:"skill"`
	Commit string `json:"commit"`
}

// amazonToolkitResponse is what the local Amazon page shows about Amazon's optional skills.
type amazonToolkitResponse struct {
	// Available is false when the helper is missing, so the page can say so instead of failing.
	Available    bool                 `json:"available"`
	Installed    []amazonToolkitSkill `json:"installed"`
	PinnedCommit string               `json:"pinned_commit,omitempty"`
}

type amazonToolkitRequest struct {
	Action string `json:"action"`
}

// amazonToolkitScriptCandidates lists where the connectors skill keeps its helper: Hermes first (it keeps the
// built-in skills under skills/openclaw-imports), then the OpenClaw workspace.
func amazonToolkitScriptCandidates(openclawDir string) []string {
	rel := filepath.Join("connectors", "scripts", "amazon_toolkit.py")
	out := []string{filepath.Join("/root/.hermes/skills/openclaw-imports", rel)}
	if openclawDir != "" {
		out = append(out, filepath.Join(openclawDir, "workspace", "skills", rel))
	}
	return out
}

func (h *DeviceMQTTHandler) amazonToolkitScript() string {
	dir := ""
	if h.config != nil {
		dir = h.config.OpenclawConfigDir
	}
	for _, p := range amazonToolkitScriptCandidates(dir) {
		if st, err := os.Stat(p); err == nil && !st.IsDir() {
			return p
		}
	}
	return ""
}

// runAmazonToolkitScript is the real runner: python3 <helper> <args...>.
func (h *DeviceMQTTHandler) runAmazonToolkitScript(ctx context.Context, args ...string) ([]byte, error) {
	script := h.amazonToolkitScript()
	if script == "" {
		return nil, errAmazonToolkitUnavailable
	}
	cmd := exec.CommandContext(ctx, "python3", append([]string{script}, args...)...)
	out, err := cmd.Output()
	if err != nil {
		var ee *exec.ExitError
		if errors.As(err, &ee) {
			msg := strings.TrimSpace(strings.TrimPrefix(strings.TrimSpace(string(ee.Stderr)), "error:"))
			if msg == "" {
				msg = "the helper failed"
			}
			if len(msg) > amazonToolkitErrMax {
				msg = msg[:amazonToolkitErrMax]
			}
			return nil, errors.New(msg)
		}
		return nil, err
	}
	return out, nil
}

func (h *DeviceMQTTHandler) amazonToolkitRunner() amazonToolkitRunner {
	if h.amazonToolkitRun != nil {
		return h.amazonToolkitRun
	}
	return h.runAmazonToolkitScript
}

// amazonToolkitStatus asks the helper what is installed.
func (h *DeviceMQTTHandler) amazonToolkitStatus(ctx context.Context) (amazonToolkitResponse, error) {
	raw, err := h.amazonToolkitRunner()(ctx, "status")
	if err != nil {
		return amazonToolkitResponse{}, err
	}
	var parsed struct {
		Installed    []amazonToolkitSkill `json:"installed"`
		PinnedCommit string               `json:"pinned_commit"`
	}
	if err := json.Unmarshal(raw, &parsed); err != nil {
		return amazonToolkitResponse{}, errors.New("the helper returned an unreadable answer")
	}
	if parsed.Installed == nil {
		parsed.Installed = []amazonToolkitSkill{}
	}
	return amazonToolkitResponse{Available: true, Installed: parsed.Installed, PinnedCommit: parsed.PinnedCommit}, nil
}

// GetAmazonToolkit handles GET /api/device/connectors/amazon/toolkit.
func (h *DeviceMQTTHandler) GetAmazonToolkit(c *gin.Context) {
	ctx, cancel := context.WithTimeout(c.Request.Context(), amazonToolkitTimeout)
	defer cancel()
	res, err := h.amazonToolkitStatus(ctx)
	if errors.Is(err, errAmazonToolkitUnavailable) {
		c.JSON(http.StatusOK, serializers.ResponseSuccess(amazonToolkitResponse{Installed: []amazonToolkitSkill{}}))
		return
	}
	if err != nil {
		slog.Warn("amazon toolkit: status failed", "component", "mqtt", "error", err)
		c.JSON(http.StatusInternalServerError, serializers.ResponseError("could not read the installed skills"))
		return
	}
	c.JSON(http.StatusOK, serializers.ResponseSuccess(res))
}

// SetAmazonToolkit handles POST /api/device/connectors/amazon/toolkit with {"action":"install"|"remove"}.
// The click on the local page is the user's confirmation, so install passes --confirmed to the helper.
func (h *DeviceMQTTHandler) SetAmazonToolkit(c *gin.Context) {
	var req amazonToolkitRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, serializers.ResponseError("invalid body: "+err.Error()))
		return
	}
	var args []string
	switch strings.TrimSpace(req.Action) {
	case "install":
		args = []string{"install", "--confirmed"}
	case "remove":
		args = []string{"remove"}
	default:
		c.JSON(http.StatusBadRequest, serializers.ResponseError(`action must be "install" or "remove"`))
		return
	}

	amazonToolkitMu.Lock()
	defer amazonToolkitMu.Unlock()
	ctx, cancel := context.WithTimeout(c.Request.Context(), amazonToolkitTimeout)
	defer cancel()

	if _, err := h.amazonToolkitRunner()(ctx, args...); err != nil {
		if errors.Is(err, errAmazonToolkitUnavailable) {
			c.JSON(http.StatusServiceUnavailable, serializers.ResponseError(err.Error()))
			return
		}
		slog.Warn("amazon toolkit: action failed", "component", "mqtt", "action", args[0], "error", err)
		c.JSON(http.StatusBadGateway, serializers.ResponseError(err.Error()))
		return
	}
	res, err := h.amazonToolkitStatus(ctx)
	if err != nil {
		c.JSON(http.StatusInternalServerError, serializers.ResponseError("done, but the installed skills could not be read back"))
		return
	}
	slog.Info("amazon toolkit: action done", "component", "mqtt", "action", args[0], "skills", len(res.Installed))
	c.JSON(http.StatusOK, serializers.ResponseSuccess(res))
}
