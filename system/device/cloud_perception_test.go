package device

import (
	"context"
	"errors"
	"sync"
	"testing"
	"time"

	"go.autonomous.ai/os/system/domain"
	"go.autonomous.ai/os/system/server/config"
)

func ptr(b bool) *bool { return &b }

func cloudTestService(t *testing.T, apply func(context.Context, config.CloudPerception) error, restart func(context.Context) error) *Service {
	t.Helper()
	t.Chdir(t.TempDir())
	if restart == nil {
		restart = func(context.Context) error { return errors.New("unexpected HAL restart") }
	}
	return &Service{config: baseConfig(), halCloudPerceptionApply: apply, halRestartCommand: restart}
}

func TestCloudPerceptionReturnsOnlyAfterHALApplied(t *testing.T) {
	applied := make(chan config.CloudPerception, 1)
	s := cloudTestService(t, func(ctx context.Context, cp config.CloudPerception) error {
		time.Sleep(150 * time.Millisecond) // slow HAL
		applied <- cp
		return nil
	}, nil)

	got, err := s.UpdateCloudPerception(domain.CloudPerceptionSetData{Pose: ptr(true)}, false)
	if err != nil {
		t.Fatal(err)
	}
	select {
	case cp := <-applied:
		if cp != got || !cp.Pose {
			t.Fatalf("applied %+v, returned %+v", cp, got)
		}
	default:
		t.Fatal("returned before HAL applied")
	}
	// Unchanged values do not call HAL again.
	if _, err := s.UpdateCloudPerception(domain.CloudPerceptionSetData{Pose: ptr(true)}, false); err != nil {
		t.Fatal(err)
	}
	if len(applied) != 0 {
		t.Fatal("unchanged request re-applied")
	}
}

func TestCloudPerceptionRestartWaitsForSlowHALToConfirm(t *testing.T) {
	restarted := false
	confirmsAfterRestart := 0
	s := cloudTestService(t,
		func(ctx context.Context, cp config.CloudPerception) error {
			if !restarted {
				return errors.New("HTTP 404") // old HAL without the endpoint
			}
			confirmsAfterRestart++
			if confirmsAfterRestart < 4 {
				return errors.New("connection refused") // new HAL still booting
			}
			if !cp.Pose {
				t.Errorf("confirmed %+v, want pose on", cp)
			}
			return nil
		},
		func(context.Context) error { restarted = true; return nil })
	s.cloudApply.retryEvery = 5 * time.Millisecond
	s.cloudApply.readyTimeout = time.Second

	if _, err := s.UpdateCloudPerception(domain.CloudPerceptionSetData{Pose: ptr(true)}, false); err != nil {
		t.Fatalf("restart fallback should succeed once HAL confirms: %v", err)
	}
	if confirmsAfterRestart != 4 {
		t.Fatalf("returned after %d confirm attempts, want the 4th (first success)", confirmsAfterRestart)
	}
}

func TestCloudPerceptionRestartWithoutReadyHALIsAnError(t *testing.T) {
	s := cloudTestService(t,
		func(context.Context, config.CloudPerception) error { return errors.New("connection refused") },
		func(context.Context) error { return nil }) // systemctl succeeds, HAL never comes up
	s.cloudApply.retryEvery = 5 * time.Millisecond
	s.cloudApply.readyTimeout = 50 * time.Millisecond

	start := time.Now()
	if _, err := s.UpdateCloudPerception(domain.CloudPerceptionSetData{Pose: ptr(true)}, false); err == nil {
		t.Fatal("reported success although HAL never confirmed after the restart")
	}
	if time.Since(start) > time.Second {
		t.Fatal("did not give up at the ready timeout")
	}
}

func TestCloudPerceptionApplyAndRestartFailureIsReportedAndRetryable(t *testing.T) {
	fail := true
	applies := 0
	s := cloudTestService(t,
		func(context.Context, config.CloudPerception) error {
			applies++
			if fail {
				return errors.New("HAL down")
			}
			return nil
		},
		func(context.Context) error {
			if fail {
				return errors.New("systemctl failed")
			}
			return nil
		})

	if _, err := s.UpdateCloudPerception(domain.CloudPerceptionSetData{Pose: ptr(true)}, false); err == nil {
		t.Fatal("apply + restart failure reported as success")
	}
	// The saved value is unchanged by a retry, but the pending apply is retried.
	fail = false
	if _, err := s.UpdateCloudPerception(domain.CloudPerceptionSetData{Pose: ptr(true)}, false); err != nil {
		t.Fatalf("retry: %v", err)
	}
	if applies != 2 {
		t.Fatalf("applies = %d, want the failed apply retried once", applies)
	}
}

func TestCloudPerceptionHALSeesChangesInSaveOrder(t *testing.T) {
	var mu sync.Mutex
	var seen []bool
	s := cloudTestService(t, func(ctx context.Context, cp config.CloudPerception) error {
		time.Sleep(20 * time.Millisecond)
		mu.Lock()
		seen = append(seen, cp.Pose)
		mu.Unlock()
		return nil
	}, nil)

	var wg sync.WaitGroup
	for i := 0; i < 10; i++ {
		on := i%2 == 0
		wg.Add(1)
		go func() {
			defer wg.Done()
			_, _ = s.UpdateCloudPerception(domain.CloudPerceptionSetData{Pose: ptr(on)}, false)
		}()
	}
	wg.Wait()
	mu.Lock()
	defer mu.Unlock()
	if len(seen) == 0 || seen[len(seen)-1] != s.config.CloudPerceptionSettings().Pose {
		t.Fatalf("last HAL value %v does not match saved %v (sequence %v)", seen, s.config.CloudPerceptionSettings().Pose, seen)
	}
}

func TestConcurrentOptOutsDoNotUndoEachOther(t *testing.T) {
	s := cloudTestService(t, func(context.Context, config.CloudPerception) error {
		time.Sleep(30 * time.Millisecond)
		return nil
	}, nil)
	if _, err := s.UpdateCloudPerception(domain.CloudPerceptionSetData{Pose: ptr(true), Motion: ptr(true), Emotion: ptr(true)}, false); err != nil {
		t.Fatal(err)
	}

	var wg sync.WaitGroup
	for _, patch := range []domain.CloudPerceptionSetData{{Pose: ptr(false)}, {Motion: ptr(false)}} {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if _, err := s.UpdateCloudPerception(patch, true); err != nil {
				t.Error(err)
			}
		}()
	}
	wg.Wait()
	if got := s.config.CloudPerceptionSettings(); got != (config.CloudPerception{Emotion: true}) {
		t.Fatalf("after concurrent opt-outs = %+v, want only emotion left on", got)
	}
}

func TestCloudPerceptionOptOutOnlyRejectsEnable(t *testing.T) {
	s := cloudTestService(t, func(context.Context, config.CloudPerception) error {
		t.Fatal("HAL called for a rejected request")
		return nil
	}, nil)
	if _, err := s.UpdateCloudPerception(domain.CloudPerceptionSetData{Pose: ptr(false), Motion: ptr(true)}, true); !errors.Is(err, ErrCloudPerceptionEnable) {
		t.Fatalf("err = %v", err)
	}
	if _, err := s.UpdateCloudPerception(domain.CloudPerceptionSetData{}, true); !errors.Is(err, ErrCloudPerceptionEmpty) {
		t.Fatalf("empty err = %v", err)
	}
	if s.config.CloudPerception != nil {
		t.Fatal("rejected request mutated config")
	}
}

func TestUpdateConfigCloudPerceptionUsesConfirmedPath(t *testing.T) {
	applied := 0
	s := cloudTestService(t, func(context.Context, config.CloudPerception) error { applied++; return nil }, nil)
	if err := s.UpdateConfig(domain.UpdateConfigRequest{CloudPerception: &domain.CloudPerceptionSetData{FireHazard: ptr(true)}}); err != nil {
		t.Fatal(err)
	}
	if applied != 1 || !s.config.CloudPerceptionSettings().FireHazard {
		t.Fatalf("applied = %d, saved = %+v", applied, s.config.CloudPerceptionSettings())
	}
}
