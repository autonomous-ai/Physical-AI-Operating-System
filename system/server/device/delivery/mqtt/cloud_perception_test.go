package mqtthandler

import (
	"encoding/json"
	"path/filepath"
	"testing"

	"go.autonomous.ai/os/system/domain"
	"go.autonomous.ai/os/system/server/config"
)

func TestCloudPerceptionSetRejectsEnableBeforeStarting(t *testing.T) {
	h, messages := infoTestHandler(t, filepath.Join(t.TempDir(), "schedules.json"))
	if err := h.handleCloudPerceptionSet(domain.MQTTDataCommand{Data: json.RawMessage(`{"pose":true}`)}); err == nil {
		t.Fatal("enable over MQTT accepted")
	}
	msg := nextPublish(t, messages)
	if string(msg["status"]) != `"failure"` || string(msg["kind"]) != `"cloud_perception.set"` {
		t.Fatalf("ack = %v", msg)
	}
	if h.config.CloudPerceptionSettings().Pose {
		t.Fatal("rejected request mutated config")
	}
}

func TestInfoReportsCloudPerception(t *testing.T) {
	cfg := &config.Config{CloudPerception: &config.CloudPerception{Emotion: true}}
	b, err := json.Marshal(domain.NewMQTTInfoResponse(cfg, "info", "aa"))
	if err != nil {
		t.Fatal(err)
	}
	var m map[string]json.RawMessage
	_ = json.Unmarshal(b, &m)
	want := `{"motion":false,"emotion":true,"pose":false,"fire_hazard":false,"speaker_id":false,"speech_emotion":false}`
	if string(m["cloud_perception"]) != want {
		t.Fatalf("cloud_perception = %s, want %s", m["cloud_perception"], want)
	}
}
