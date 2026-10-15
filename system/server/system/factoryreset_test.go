package system

import (
	"slices"
	"testing"

	"go.autonomous.ai/os/system/lib/syspath"
)

// Unshipped logs can hold the previous owner's speech; left behind, they would
// ship with the next owner's key after the device is set up again.
func TestFactoryResetWipesTheLogSpool(t *testing.T) {
	if !slices.Contains(deviceWipePaths, syspath.GELFSpoolDir()) {
		t.Fatalf("deviceWipePaths %v does not include the GELF spool %s", deviceWipePaths, syspath.GELFSpoolDir())
	}
}

// Connector credentials sit under the OpenClaw workspace, which only the
// OpenClaw reset wipes; on any other runtime the next owner would inherit them.
func TestFactoryResetWipesConnectorCredentials(t *testing.T) {
	const configs = "/root/.openclaw/workspace/configs"
	if !slices.Contains(deviceWipePaths, configs) {
		t.Fatalf("deviceWipePaths %v does not include the connector credentials %s", deviceWipePaths, configs)
	}
}
