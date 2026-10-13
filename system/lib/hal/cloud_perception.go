package hal

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"time"

	"go.autonomous.ai/os/system/server/config"
)

var cloudPerceptionClient = &http.Client{Timeout: 30 * time.Second}

// SetCloudPerception applies the camera and voice cloud opt-ins to the running HAL without a restart.
func SetCloudPerception(ctx context.Context, cp config.CloudPerception) error {
	body, err := json.Marshal(cp)
	if err != nil {
		return fmt.Errorf("encode cloud perception: %w", err)
	}
	req, err := newRequest(http.MethodPost, "/sensing/cloud-perception", bytes.NewReader(body))
	if err != nil {
		return fmt.Errorf("create cloud perception request: %w", err)
	}
	resp, err := cloudPerceptionClient.Do(req.WithContext(ctx))
	if err != nil {
		return fmt.Errorf("post cloud perception: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return fmt.Errorf("HAL cloud perception returned HTTP %d", resp.StatusCode)
	}
	return nil
}
