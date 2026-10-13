package mqtthandler

import (
	"encoding/json"
	"errors"
	"log/slog"

	"go.autonomous.ai/os/system/device"
	"go.autonomous.ai/os/system/domain"
	"go.autonomous.ai/os/system/server/config"
)

func (h *DeviceMQTTHandler) publishCloudPerceptionAck(status, errMsg string, data *config.CloudPerception) {
	ack := domain.MQTTCloudPerceptionAck{
		MQTTInfoResponse: domain.NewMQTTInfoResponse(h.config, "data", device.GetDeviceMac()),
		Kind:             domain.KindCloudPerceptionSet,
		Status:           status,
		Error:            errMsg,
		Data:             data,
	}
	if err := h.publish(ack); err != nil {
		slog.Warn("cloud_perception.set: publish ack failed", "component", "mqtt", "status", status, "error", err)
	}
}

// handleCloudPerceptionSet only turns perceptions off: MQTT carries no sender
// authentication, so turning an upload on stays a local choice in Settings > Privacy.
func (h *DeviceMQTTHandler) handleCloudPerceptionSet(env domain.MQTTDataCommand) error {
	var req domain.CloudPerceptionSetData
	if err := json.Unmarshal(env.Data, &req); err != nil {
		slog.Error("cloud_perception.set: invalid payload", "component", "mqtt", "error", err)
		h.publishCloudPerceptionAck("failure", "invalid payload", nil)
		return err
	}
	named := false
	for _, f := range req.Patch(&config.CloudPerception{}) {
		if f.In == nil {
			continue
		}
		named = true
		if *f.In {
			h.publishCloudPerceptionAck("failure", device.ErrCloudPerceptionEnable.Error(), nil)
			return device.ErrCloudPerceptionEnable
		}
	}
	if !named {
		h.publishCloudPerceptionAck("failure", device.ErrCloudPerceptionEmpty.Error(), nil)
		return device.ErrCloudPerceptionEmpty
	}

	slog.Info("cloud_perception.set: received", "component", "mqtt")
	h.publishCloudPerceptionAck("starting", "", nil)

	go func() {
		// UpdateCloudPerception merges into the latest config under its own lock and
		// acks only after HAL applied, so concurrent opt-outs cannot undo each other.
		next, err := h.deviceService.UpdateCloudPerception(req, true)
		if err != nil {
			if errors.Is(err, device.ErrCloudPerceptionEnable) || errors.Is(err, device.ErrCloudPerceptionEmpty) {
				h.publishCloudPerceptionAck("failure", err.Error(), nil)
				return
			}
			slog.Error("cloud_perception.set: apply failed", "component", "mqtt", "error", err)
			h.publishCloudPerceptionAck("failure", err.Error(), &next)
			return
		}
		h.publishCloudPerceptionAck("success", "", &next)
	}()
	return nil
}
