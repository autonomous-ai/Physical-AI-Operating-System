package ota

import (
	"context"
	"errors"
	"net/http"
	"os"
	"path/filepath"
	"testing"
)

func TestTriggerServiceUpdateRequiresRegisteredUnblockedTarget(t *testing.T) {
	var paths []string
	fakeBootstrap(t, func(w http.ResponseWriter, r *http.Request) {
		paths = append(paths, r.URL.Path)
		w.WriteHeader(http.StatusOK)
	})
	old := serviceRegistryPath
	serviceRegistryPath = filepath.Join(t.TempDir(), "ownership.json")
	t.Cleanup(func() { serviceRegistryPath = old })
	payload := `{"format":1,"components":{"lamp-os":{"service":"lamp-os","replaces":["hal"],"version":"1.0.0","release":"/opt/autonomous-services/lamp-os/releases/0123456789abcdef0123456789abcdef","data":"/var/lib/autonomous-services/lamp-os/releases/0123456789abcdef0123456789abcdef"}},"blocked":{"hal":"lamp-os"}}`
	if err := os.WriteFile(serviceRegistryPath, []byte(payload), 0600); err != nil {
		t.Fatal(err)
	}

	for _, key := range []string{"hal", "device", "unknown-service", "../bad", ""} {
		if _, err := TriggerUpdate(context.Background(), hermesCfg(), key); !errors.Is(err, ErrUnknownTarget) {
			t.Errorf("target %q got %v", key, err)
		}
	}
	if _, err := TriggerUpdate(context.Background(), hermesCfg(), "lamp-os"); err != nil {
		t.Fatal(err)
	}
	if len(paths) != 1 || paths[0] != "/force-update/lamp-os" {
		t.Fatalf("paths %v", paths)
	}
}
