package mqtthandler

import (
	"encoding/json"
	"errors"
	"log/slog"

	"go.autonomous.ai/os/system/device"
	"go.autonomous.ai/os/system/domain"
	"go.autonomous.ai/os/system/server/config"
)

// MQTT carries no sender authentication, so it may only withdraw consent. Turning an
// upload on stays a local choice in the device's web Settings > Privacy.
var (
	errCloudPerceptionEmpty      = errors.New("at least one of motion, emotion, pose, fire_hazard is required")
	errCloudPerceptionEnableMQTT = errors.New("cloud perceptions can only be turned on from the device's local Settings > Privacy")
)

// optOutCloudPerception applies req on top of current, rejecting any request to turn one on.
func optOutCloudPerception(current config.CloudPerception, req domain.CloudPerceptionSetData) (config.CloudPerception, error) {
	next := current
	changed := false
	for _, f := range []struct {
		in  *bool
		out *bool
	}{
		{req.Motion, &next.Motion},
		{req.Emotion, &next.Emotion},
		{req.Pose, &next.Pose},
		{req.FireHazard, &next.FireHazard},
	} {
		if f.in == nil {
			continue
		}
		if *f.in {
			return current, errCloudPerceptionEnableMQTT
		}
		*f.out = false
		changed = true
	}
	if !changed {
		return current, errCloudPerceptionEmpty
	}
	return next, nil
}

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

func (h *DeviceMQTTHandler) handleCloudPerceptionSet(env domain.MQTTDataCommand) error {
	var req domain.CloudPerceptionSetData
	if err := json.Unmarshal(env.Data, &req); err != nil {
		slog.Error("cloud_perception.set: invalid payload", "component", "mqtt", "error", err)
		h.publishCloudPerceptionAck("failure", "invalid payload", nil)
		return err
	}
	next, err := optOutCloudPerception(h.config.CloudPerceptionSettings(), req)
	if err != nil {
		slog.Warn("cloud_perception.set: rejected", "component", "mqtt", "error", err)
		h.publishCloudPerceptionAck("failure", err.Error(), nil)
		return err
	}

	slog.Info("cloud_perception.set: received", "component", "mqtt",
		"motion", next.Motion, "emotion", next.Emotion, "pose", next.Pose, "fire_hazard", next.FireHazard)
	h.publishCloudPerceptionAck("starting", "", nil)

	go func() {
		if err := h.deviceService.UpdateConfig(domain.UpdateConfigRequest{CloudPerception: &next}); err != nil {
			slog.Error("cloud_perception.set: UpdateConfig failed", "component", "mqtt", "error", err)
			h.publishCloudPerceptionAck("failure", err.Error(), &next)
			return
		}
		h.publishCloudPerceptionAck("success", "", &next)
	}()
	return nil
}
