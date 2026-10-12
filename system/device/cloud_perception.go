package device

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"sync"
	"time"

	"go.autonomous.ai/os/system/domain"
	"go.autonomous.ai/os/system/lib/hal"
	"go.autonomous.ai/os/system/server/config"
)

const (
	cloudPerceptionApplyTimeout = 30 * time.Second
	// After a fallback restart, HAL needs time to boot before it can confirm.
	cloudPerceptionReadyTimeout = 90 * time.Second
	cloudPerceptionRetryEvery   = time.Second
)

var (
	// ErrCloudPerceptionEmpty rejects a request that names no perception.
	ErrCloudPerceptionEmpty = errors.New("at least one of motion, emotion, pose, fire_hazard is required")
	// ErrCloudPerceptionEnable rejects turning an upload on from a channel that cannot carry consent.
	ErrCloudPerceptionEnable = errors.New("cloud perceptions can only be turned on from the device's local Settings > Privacy")
)

// cloudPerceptionApply serializes save + HAL confirmation so HAL receives changes in
// save order; pending keeps a failed apply retryable with an unchanged request.
type cloudPerceptionApply struct {
	mu      sync.Mutex
	pending bool
	// Test overrides; zero means the defaults above.
	readyTimeout time.Duration
	retryEvery   time.Duration
}

// UpdateCloudPerception merges patch into the latest saved opt-ins and returns only
// after HAL has applied them (live, or by a restart when the live call fails).
// optOutOnly rejects any request to turn a perception on.
func (s *Service) UpdateCloudPerception(patch domain.CloudPerceptionSetData, optOutOnly bool) (config.CloudPerception, error) {
	fields := []*bool{patch.Motion, patch.Emotion, patch.Pose, patch.FireHazard}
	named := false
	for _, v := range fields {
		if v == nil {
			continue
		}
		named = true
		if optOutOnly && *v {
			return s.config.CloudPerceptionSettings(), ErrCloudPerceptionEnable
		}
	}
	if !named {
		return s.config.CloudPerceptionSettings(), ErrCloudPerceptionEmpty
	}

	s.cloudApply.mu.Lock()
	defer s.cloudApply.mu.Unlock()

	var next config.CloudPerception
	if err := s.config.WithLockSave(func(c *config.Config) {
		// Merge onto the latest saved value under the config lock, so concurrent
		// requests that each name one key never overwrite each other.
		prev := c.CloudPerceptionSettings()
		next = prev
		for _, f := range []struct {
			in  *bool
			out *bool
		}{
			{patch.Motion, &next.Motion},
			{patch.Emotion, &next.Emotion},
			{patch.Pose, &next.Pose},
			{patch.FireHazard, &next.FireHazard},
		} {
			if f.in != nil {
				*f.out = *f.in
			}
		}
		c.CloudPerception = &next
		s.cloudApply.pending = s.cloudApply.pending || next != prev
	}); err != nil {
		return s.config.CloudPerceptionSettings(), fmt.Errorf("save config: %w", err)
	}
	if !s.cloudApply.pending {
		return next, nil
	}
	if err := s.applyCloudPerceptionAndWait(next); err != nil {
		return next, err
	}
	s.cloudApply.pending = false
	return next, nil
}

// applyCloudPerceptionAndWait requires cloudApply.mu. HAL replies only after it has
// created or retired the processors; a failed live call falls back to a restart,
// after which HAL reads the saved value at boot.
func (s *Service) applyCloudPerceptionAndWait(cp config.CloudPerception) error {
	ctx, cancel := context.WithTimeout(context.Background(), cloudPerceptionApplyTimeout)
	defer cancel()
	apply := s.halCloudPerceptionApply
	if apply == nil {
		apply = func(ctx context.Context, cp config.CloudPerception) error {
			return hal.SetCloudPerception(ctx, cp.Motion, cp.Emotion, cp.Pose, cp.FireHazard)
		}
	}
	err := apply(ctx, cp)
	if err == nil {
		slog.Info("hal cloud perception applied live", "component", "device",
			"motion", cp.Motion, "emotion", cp.Emotion, "pose", cp.Pose, "fire_hazard", cp.FireHazard)
		if err := config.SnapshotHALConfig(); err != nil {
			slog.Warn("hal config snapshot failed", "component", "device", "error", err)
		}
		return nil
	}
	slog.Warn("hal cloud perception apply failed, restarting instead", "component", "device", "error", err)
	restartCtx, cancelRestart := context.WithTimeout(context.Background(), cloudPerceptionApplyTimeout)
	defer cancelRestart()
	if rerr := s.restartHALAndWait(restartCtx, "cloud perception change"); rerr != nil {
		return fmt.Errorf("apply cloud perception: %w; HAL restart: %w", err, rerr)
	}
	// `systemctl restart` returning only means the process was started. Success is
	// reported only once the new HAL answers the apply call with the wanted state.
	return s.confirmCloudPerceptionAfterRestart(apply, cp)
}

func (s *Service) confirmCloudPerceptionAfterRestart(apply func(context.Context, config.CloudPerception) error, cp config.CloudPerception) error {
	readyTimeout, retryEvery := s.cloudApply.readyTimeout, s.cloudApply.retryEvery
	if readyTimeout == 0 {
		readyTimeout = cloudPerceptionReadyTimeout
	}
	if retryEvery == 0 {
		retryEvery = cloudPerceptionRetryEvery
	}
	deadline := time.Now().Add(readyTimeout)
	for {
		ctx, cancel := context.WithTimeout(context.Background(), cloudPerceptionApplyTimeout)
		err := apply(ctx, cp)
		cancel()
		if err == nil {
			slog.Info("hal cloud perception confirmed after restart", "component", "device")
			return nil
		}
		if !time.Now().Add(retryEvery).Before(deadline) {
			return fmt.Errorf("HAL restarted but did not confirm cloud perception within %s: %w", readyTimeout, err)
		}
		time.Sleep(retryEvery)
	}
}
