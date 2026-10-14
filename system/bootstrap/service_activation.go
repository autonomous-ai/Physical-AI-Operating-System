package bootstrap

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"sort"
	"strings"
	"time"

	"go.autonomous.ai/os/system/bootstrap/state"
	"go.autonomous.ai/os/system/domain"
	"go.autonomous.ai/os/system/lib/core/system"
)

// activationPlan accepts only decoded, signature-verified service metadata. It
// validates the whole replacement graph before starting any service transition.
func (b *Bootstrap) activationPlan(meta domain.OTAMetadata) ([]string, error) {
	r, err := b.serviceRegistry()
	if err != nil {
		return nil, err
	}
	desired := map[string]domain.OTAComponent{}
	deviceType := resolveDeviceType()
	for key, c := range meta {
		if deviceType == "" || !c.ActivateByDevice[deviceType] || !r.Allows(key) {
			continue
		}
		if err := domain.ValidateOTAServiceComponent(key, c); err != nil {
			return nil, err
		}
		if _, installed := r.Components[key]; installed {
			continue
		}
		floor := strings.TrimSpace(c.MinVersionByDevice[deviceType])
		if floor == "" || compareVersions(floor, "0") <= 0 {
			continue
		}
		if compareVersions(floor, c.Version) > 0 {
			return nil, fmt.Errorf("activation floor exceeds version for %q", key)
		}
		desired[key] = c
	}
	keys := make([]string, 0, len(desired))
	for key := range desired {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	owners := map[string]string{}
	for _, key := range keys {
		for _, old := range desired[key].Replaces {
			if prior := owners[old]; prior != "" {
				return nil, fmt.Errorf("conflicting activation owners for %q: %s and %s", old, prior, key)
			}
			owners[old] = key
			if !r.Allows(old) {
				return nil, fmt.Errorf("activation %q replaces already replaced component %q", key, old)
			}
			if !domain.LegacyOTAService(old) {
				if _, installed := r.Components[old]; !installed {
					if _, planned := desired[old]; !planned {
						return nil, fmt.Errorf("activation %q requires installed or promoted predecessor %q", key, old)
					}
				}
			}
		}
	}
	visited := map[string]int{}
	ordered := []string{}
	var visit func(string) error
	visit = func(key string) error {
		if visited[key] == 1 {
			return fmt.Errorf("cyclic service activation at %q", key)
		}
		if visited[key] == 2 {
			return nil
		}
		visited[key] = 1
		for _, old := range desired[key].Replaces {
			if _, ok := desired[old]; ok {
				if err := visit(old); err != nil {
					return err
				}
			}
		}
		visited[key] = 2
		ordered = append(ordered, key)
		return nil
	}
	for _, key := range keys {
		if err := visit(key); err != nil {
			return nil, err
		}
	}
	return ordered, nil
}

func updaterSupportsAutomaticActivation() bool {
	path, err := exec.LookPath("software-update")
	if err != nil {
		return false
	}
	data, err := os.ReadFile(path)
	return err == nil && updaterSupportsActivationProtocol(data)
}

func updaterSupportsActivationProtocol(data []byte) bool {
	if !updaterSupportsServiceProtocol(data) {
		return false
	}
	count := 0
	for _, line := range strings.Split(string(data), "\n") {
		if strings.HasPrefix(line, "# Service OTA activation protocol:") {
			if line != "# Service OTA activation protocol: 1" {
				return false
			}
			count++
		}
	}
	return count == 1
}

func (b *Bootstrap) activatePromotedServices(ctx context.Context, meta domain.OTAMetadata) (bool, error) {
	keys, err := b.activationPlan(meta)
	if err != nil {
		return false, err
	}
	if len(keys) == 0 {
		return false, nil
	}
	if !updaterSupportsAutomaticActivation() {
		return false, fmt.Errorf("software-update does not support promoted service activation")
	}
	blocked, err := b.rollbackVersions()
	if err != nil {
		return false, err
	}
	changed := false
	for _, key := range keys {
		c := meta[key]
		if strings.TrimSpace(blocked[key]) == strings.TrimSpace(c.Version) {
			// Stop dependents too: installing a successor would bypass the failed version guard.
			return changed, fmt.Errorf("activation %q version %q blocked after rollback", key, c.Version)
		}
		b.announceUpdateStart()
		runCtx, cancel := context.WithTimeout(ctx, 10*time.Minute)
		// The updater revalidates the fresh signed promotion under its update lock.
		out, runErr := system.Run(runCtx, "software-update", key, "--activate-promoted")
		cancel()
		if runErr != nil {
			return changed, fmt.Errorf("activate %s: %w: %s", key, runErr, out)
		}
		r, err := b.serviceRegistry()
		if err != nil {
			return changed, err
		}
		record, ok := r.Components[key]
		if !ok || !r.Allows(key) {
			return changed, fmt.Errorf("activation %q did not establish ownership", key)
		}
		if err := r.ValidateTopology(key, c); err != nil {
			return changed, err
		}
		b.state.Components[key] = record.Version
		// Persist each successful transition even if a later independent activation fails.
		if err := state.Save(b.cfg.StateFile, b.state); err != nil {
			return changed, fmt.Errorf("save activation state: %w", err)
		}
		changed = true
	}
	return changed, nil
}
