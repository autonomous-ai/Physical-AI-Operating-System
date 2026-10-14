package bootstrap

import (
	"context"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"go.autonomous.ai/os/system/domain"
)

func promotedService(key string, replaces ...string) domain.OTAComponent {
	return domain.OTAComponent{Service: key, Replaces: replaces, Version: "2.0.0", MinVersionByDevice: map[string]string{"ota-test": "2.0.0"}, ActivateByDevice: map[string]bool{"ota-test": true}}
}
func TestActivationPlanPromotionAndOwnership(t *testing.T) {
	b, _ := serviceBootstrap(t)
	cases := []struct {
		name   string
		mutate func(*domain.OTAComponent)
		want   bool
	}{
		{"promoted", func(*domain.OTAComponent) {}, true},
		{"wrong device", func(c *domain.OTAComponent) { c.ActivateByDevice = map[string]bool{"lamp": true} }, false},
		{"disabled", func(c *domain.OTAComponent) { c.ActivateByDevice["ota-test"] = false }, false},
		{"global floor insufficient", func(c *domain.OTAComponent) { c.MinVersionByDevice = nil; c.MinVersion = "2.0.0" }, false},
		{"zero floor holds", func(c *domain.OTAComponent) { c.MinVersionByDevice["ota-test"] = "0.0.0" }, false},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			c := promotedService("lamp-os", "hal", "os-server")
			tc.mutate(&c)
			got, err := b.activationPlan(domain.OTAMetadata{"lamp-os": c})
			if err != nil || (len(got) > 0) != tc.want {
				t.Fatalf("plan %v %v", got, err)
			}
		})
	}
	putServiceRegistry(t, b, sampleServiceRegistry())
	plan, err := b.activationPlan(domain.OTAMetadata{"lamp-os": promotedService("lamp-os", "hal", "os-server")})
	if err != nil || len(plan) != 0 {
		t.Fatalf("installed service activated: %v %v", plan, err)
	}
}
func TestActivationPlanChainsConflictsAndCycles(t *testing.T) {
	b, _ := serviceBootstrap(t)
	first := promotedService("z-old", "hal", "os-server")
	last := promotedService("a-new", "z-old")
	got, err := b.activationPlan(domain.OTAMetadata{"z-old": first, "a-new": last})
	if err != nil || !reflect.DeepEqual(got, []string{"z-old", "a-new"}) {
		t.Fatalf("chain %v %v", got, err)
	}
	for name, meta := range map[string]domain.OTAMetadata{
		"conflict":            {"z-old": first, "other": promotedService("other", "hal")},
		"cycle":               {"a": promotedService("a", "b"), "b": promotedService("b", "a")},
		"missing predecessor": {"a-new": last},
		"invalid nonservice":  {"hal": {Version: "2.0.0", ActivateByDevice: map[string]bool{"ota-test": true}}},
	} {
		t.Run(name, func(t *testing.T) {
			if _, err := b.activationPlan(meta); err == nil {
				t.Fatal("invalid activation accepted")
			}
		})
	}
	putServiceRegistry(t, b, sampleServiceRegistry())
	if _, err := b.activationPlan(domain.OTAMetadata{"other": promotedService("other", "hal")}); err == nil {
		t.Fatal("replaced ancestor resurrected")
	}
}
func TestActivationMetadataNeverTrustsUnsignedIntent(t *testing.T) {
	payload := []byte(`{"hal":{"version":"2.0.0","activate_by_device":{"ota-test":true}}}`)
	meta, err := decodeOTAMetadataPayload(payload, false)
	if err != nil || len(meta) != 0 {
		t.Fatalf("unsigned activation survived %v %v", meta, err)
	}
	if _, err := decodeOTAMetadataPayload(payload, true); err == nil {
		t.Fatal("signed nonservice activation accepted")
	}
}
func TestActivationUpdaterFlagAndRollbackGuard(t *testing.T) {
	b, calls := serviceBootstrap(t)
	path := filepath.Join(filepath.Dir(calls), "bin", "software-update")
	body := "#!/bin/sh\n# Service OTA protocol: 1\n# Service OTA activation protocol: 1\nprintf '%s\\n' \"$*\" >> \"$UPDATE_TEST_CALLS\"\nexit 1\n"
	if err := os.WriteFile(path, []byte(body), 0755); err != nil {
		t.Fatal(err)
	}
	meta := domain.OTAMetadata{"lamp-os": promotedService("lamp-os", "hal", "os-server")}
	if _, err := b.activatePromotedServices(context.Background(), meta); err == nil {
		t.Fatal("updater failure swallowed")
	}
	data, err := os.ReadFile(calls)
	if err != nil || string(data) != "lamp-os --activate-promoted\n" {
		t.Fatalf("calls %q %v", data, err)
	}
	if err := os.Remove(calls); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(b.rollbackConfigPath, []byte(`{"rollback_versions":{"lamp-os":"2.0.0"}}`), 0600); err != nil {
		t.Fatal(err)
	}
	if _, err := b.activatePromotedServices(context.Background(), meta); err == nil {
		t.Fatal("rollback guard bypassed")
	}
	if _, err := os.Stat(calls); !os.IsNotExist(err) {
		t.Fatal("blocked activation invoked updater")
	}
	if err := os.WriteFile(path, []byte(strings.ReplaceAll(body, "# Service OTA activation protocol: 1\n", "")), 0755); err != nil {
		t.Fatal(err)
	}
	if updaterSupportsAutomaticActivation() {
		t.Fatal("legacy updater advertised automatic activation")
	}
}

func TestActivationSuccessIsPersistedAndNotRepeated(t *testing.T) {
	b, calls := serviceBootstrap(t)
	fixture := filepath.Join(filepath.Dir(calls), "next-ownership.json")
	r := sampleServiceRegistry()
	putServiceRegistry(t, b, r)
	if err := os.Rename(filepath.Join(b.serviceDirectory(), "ownership.json"), fixture); err != nil {
		t.Fatal(err)
	}
	t.Setenv("ACTIVATION_REGISTRY_FIXTURE", fixture)
	t.Setenv("ACTIVATION_REGISTRY_TARGET", filepath.Join(b.serviceDirectory(), "ownership.json"))
	body := "#!/bin/sh\n# Service OTA protocol: 1\n# Service OTA activation protocol: 1\nprintf '%s\\n' \"$*\" >> \"$UPDATE_TEST_CALLS\"\ncp \"$ACTIVATION_REGISTRY_FIXTURE\" \"$ACTIVATION_REGISTRY_TARGET\"\n"
	if err := os.WriteFile(filepath.Join(filepath.Dir(calls), "bin", "software-update"), []byte(body), 0755); err != nil {
		t.Fatal(err)
	}
	meta := domain.OTAMetadata{"lamp-os": promotedService("lamp-os", "hal", "os-server")}
	changed, err := b.activatePromotedServices(context.Background(), meta)
	if err != nil || !changed {
		t.Fatalf("activation %v %v", changed, err)
	}
	if b.state.Components["lamp-os"] != "1.0.0" {
		t.Fatal("did not use actual installed version")
	}
	if _, err := os.Stat(b.cfg.StateFile); err != nil {
		t.Fatal("successful activation not persisted")
	}
	changed, err = b.activatePromotedServices(context.Background(), meta)
	if err != nil || changed {
		t.Fatalf("repeated activation %v %v", changed, err)
	}
	data, err := os.ReadFile(calls)
	if err != nil || string(data) != "lamp-os --activate-promoted\n" {
		t.Fatalf("calls %q %v", data, err)
	}
}
