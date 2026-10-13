package mqtthandler

import (
	"encoding/json"
	"log/slog"
	"time"

	"go.autonomous.ai/os/system/domain"
	"go.autonomous.ai/os/system/lib/hal"
	"go.autonomous.ai/os/system/server/config"
)

// The wakeword.settings.* kinds are the MQTT twin of the web Settings wake
// controls: same validation and save path as PUT /api/device/config.

// wakeSettingsReplyWait bounds how long a set waits before replying. Phrases,
// chime, light and the listening window apply live; only a gate change restarts
// HAL (up to 30 s), which would outlast the backend's reply wait.
var wakeSettingsReplyWait = 5 * time.Second

// handleWakeSettingsGet handles kind="wakeword.settings.get". Synchronous.
func (h *DeviceMQTTHandler) handleWakeSettingsGet(env domain.MQTTDataCommand) error {
	return h.publishDataResult(domain.KindWakeSettingsGet, "success", "", h.deviceService.WakeSettings())
}

// handleWakeSettingsSet handles kind="wakeword.settings.set" and replies once
// with the saved settings, or the validation/save error.
func (h *DeviceMQTTHandler) handleWakeSettingsSet(env domain.MQTTDataCommand) error {
	var d domain.WakeSettingsSetData
	if err := json.Unmarshal(env.Data, &d); err != nil {
		return h.publishDataResult(domain.KindWakeSettingsSet, "failure", "invalid wakeword.settings.set data: "+err.Error(), nil)
	}
	if d.Empty() {
		return h.publishDataResult(domain.KindWakeSettingsSet, "failure", "nothing to change", nil)
	}

	done := make(chan error, 1)
	go func() { done <- h.deviceService.UpdateConfig(d.UpdateRequest()) }()
	select {
	case err := <-done:
		if err != nil {
			slog.Error("wakeword.settings.set: failed", "component", "mqtt", "error", err)
			return h.publishDataResult(domain.KindWakeSettingsSet, "failure", err.Error(), nil)
		}
		slog.Info("wakeword.settings.set: saved", "component", "mqtt")
		return h.publishDataResult(domain.KindWakeSettingsSet, "success", "", h.deviceService.WakeSettings())
	case <-time.After(wakeSettingsReplyWait):
	}

	// Validation runs before the save, so a slow update has already saved and
	// is restarting HAL for a gate change. A later failure is reported on its own.
	settings := h.deviceService.WakeSettings()
	settings.Applying = true
	slog.Info("wakeword.settings.set: saved; HAL still applying", "component", "mqtt")
	go func() {
		if err := <-done; err != nil {
			slog.Error("wakeword.settings.set: apply failed", "component", "mqtt", "error", err)
			_ = h.publishDataResult(domain.KindWakeSettingsSet, "failure", err.Error(), nil)
		}
	}()
	return h.publishDataResult(domain.KindWakeSettingsSet, "success", "", settings)
}

// handleWakeLightPreview handles kind="wakeword.light.preview": HAL plays the
// look for a few seconds, then restores the strip. Nothing is saved.
func (h *DeviceMQTTHandler) handleWakeLightPreview(env domain.MQTTDataCommand) error {
	var d domain.WakeLightPreviewData
	if err := json.Unmarshal(env.Data, &d); err != nil {
		return h.publishDataResult(domain.KindWakeLightPreview, "failure", "invalid wakeword.light.preview data: "+err.Error(), nil)
	}
	if !h.hasLight() {
		return h.publishDataResult(domain.KindWakeLightPreview, "failure", errNoLight, nil)
	}
	if d.Color == nil {
		return h.publishDataResult(domain.KindWakeLightPreview, "failure", "color is required", nil)
	}
	look := config.WakeLight{Color: d.Color, Effect: d.Effect, Speed: d.Speed}
	if err := look.Validate(); err != nil {
		return h.publishDataResult(domain.KindWakeLightPreview, "failure", err.Error(), nil)
	}
	if look.Effect == "" {
		look.Effect = config.WakeLightDefault.Effect
	}
	if look.Speed == 0 {
		look.Speed = config.WakeLightDefault.Speed
	}
	res, err := hal.PreviewWakeLight(hal.WakeLightLook{Color: look.Color, Effect: look.Effect, Speed: look.Speed})
	if err != nil {
		slog.Warn("wakeword.light.preview: failed", "component", "mqtt", "error", err)
		return h.publishDataResult(domain.KindWakeLightPreview, "failure", err.Error(), nil)
	}
	return h.publishDataResult(domain.KindWakeLightPreview, "success", "", res)
}
