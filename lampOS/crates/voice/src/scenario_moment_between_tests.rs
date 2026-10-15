use super::*;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};

const HOST_GUARD: Duration = Duration::from_secs(30);

fn root_scope() -> Scope {
    let root = Scope::enter(HOST_GUARD).unwrap();
    root.set_pump(|_| Ok(())).unwrap();
    root
}

fn receive<T>(receiver: &Receiver<T>) -> T {
    receiver
        .recv_timeout(HOST_GUARD)
        .expect("fixture thread must answer within the host guard")
}

fn assert_host_clock() {
    assert!(!is_active());
    let before = lamp_ipc::monotonic_us();
    let read = monotonic_us();
    assert!(before <= read && read <= lamp_ipc::monotonic_us());
    let before = Instant::now();
    let read = instant();
    assert!(before <= read && read <= Instant::now());
}

#[test]
fn inclusive_projection_uses_absolute_origin_offsets_without_running_peers() {
    let root = Scope::enter(HOST_GUARD).unwrap();
    let calls = Rc::new(Cell::new(0));
    let counted = calls.clone();
    root.set_pump(move |_| {
        counted.set(counted.get() + 1);
        Ok(())
    })
    .unwrap();
    let initial = root.moment().unwrap();
    step(Phase::Run).unwrap();
    let middle = root.moment().unwrap();
    step(Phase::Run).unwrap();
    let last = root.moment().unwrap();
    for (lower, upper, offsets) in [(&initial, &middle, [0, 1, 2]), (&middle, &last, [2, 3, 4])] {
        for offset in offsets {
            let elapsed = Duration::from_millis(offset);
            for _ in 0..16 {
                let projected = lower.between(upper, elapsed).unwrap();
                assert_eq!(projected.elapsed(), elapsed);
                assert_eq!(
                    projected.monotonic_us(),
                    initial.monotonic_us() + offset * 1_000
                );
                assert_eq!(projected.instant(), initial.instant() + elapsed);
            }
        }
    }
    assert_eq!(calls.get(), 2);
    assert_eq!(root.moment().unwrap().elapsed(), last.elapsed());
    assert_eq!(
        (monotonic_us(), instant()),
        (last.monotonic_us(), last.instant())
    );
}

#[test]
fn equal_endpoints_admit_only_their_exact_offset() {
    let root = root_scope();
    step(Phase::Run).unwrap();
    let moment = root.moment().unwrap();
    let duplicate = moment.clone();
    let projected = moment.between(&duplicate, moment.elapsed()).unwrap();
    assert_eq!(projected.elapsed(), moment.elapsed());
    assert_eq!(projected.monotonic_us(), moment.monotonic_us());
    assert_eq!(projected.instant(), moment.instant());
    assert!(
        moment
            .between(&duplicate, moment.elapsed() - Duration::from_nanos(1))
            .is_err()
    );
    assert!(
        moment
            .between(&duplicate, moment.elapsed() + Duration::from_nanos(1))
            .is_err()
    );
    assert_eq!(root.moment().unwrap().elapsed(), moment.elapsed());
}

#[test]
fn reversed_and_outside_intervals_fail_without_changing_the_root() {
    let root = root_scope();
    step(Phase::Run).unwrap();
    let lower = root.moment().unwrap();
    step(Phase::Run).unwrap();
    let upper = root.moment().unwrap();
    assert!(
        upper
            .between(&lower, Duration::from_millis(3))
            .unwrap_err()
            .to_string()
            .contains("reversed")
    );
    for outside in [
        Duration::ZERO,
        lower.elapsed() - Duration::from_nanos(1),
        upper.elapsed() + Duration::from_nanos(1),
        Duration::MAX,
    ] {
        assert!(
            lower
                .between(&upper, outside)
                .unwrap_err()
                .to_string()
                .contains("outside")
        );
    }
    assert_eq!(
        lower
            .between(&upper, Duration::from_millis(3))
            .unwrap()
            .elapsed(),
        Duration::from_millis(3)
    );
    assert_eq!(root.moment().unwrap().elapsed(), upper.elapsed());
}

#[test]
fn nanosecond_projection_keeps_order_inside_one_integer_microsecond() {
    let root = Rc::new(Scope::enter(HOST_GUARD).unwrap());
    let weak_root = Rc::downgrade(&root);
    let snapshots = Rc::new(RefCell::new(None));
    let captured = snapshots.clone();
    root.set_pump(move |_| {
        let root = weak_root.upgrade().unwrap();
        elapse_for_fault(Duration::from_nanos(900))?;
        let lower = root.moment()?;
        elapse_for_fault(Duration::from_nanos(50))?;
        *captured.borrow_mut() = Some((lower, root.moment()?));
        Ok(())
    })
    .unwrap();
    step(Phase::Run).unwrap();
    let (lower, upper) = snapshots.borrow_mut().take().unwrap();
    let projected = lower.between(&upper, Duration::from_nanos(925)).unwrap();
    assert_eq!(projected.elapsed(), Duration::from_nanos(925));
    assert_eq!(projected.monotonic_us(), lower.monotonic_us());
    assert_eq!(projected.monotonic_us(), upper.monotonic_us());
    assert_eq!(
        projected.instant() - lower.instant(),
        Duration::from_nanos(25)
    );
    assert_eq!(
        upper.instant() - projected.instant(),
        Duration::from_nanos(25)
    );
    std::thread::spawn(move || {
        let mut observer = ObserverScope::enter(lower.clone()).unwrap();
        observer.observe(projected.clone()).unwrap();
        assert!(observer.observe(lower).is_err());
        assert_eq!(instant(), projected.instant());
        observer.observe(upper.clone()).unwrap();
        assert!(observer.observe(projected).is_err());
        assert_eq!(instant(), upper.instant());
    })
    .join()
    .unwrap();
    assert_eq!(
        root.moment().unwrap().elapsed(),
        Duration::from_nanos(2_000_950)
    );
}

#[test]
fn same_valued_distinct_roots_cannot_be_mixed_or_admitted_by_projection() {
    let root = root_scope();
    let original = root.moment().unwrap();
    std::thread::scope(|threads| {
        let (snapshot_tx, snapshot_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::channel();
        let same_values = original.clone();
        let other = threads.spawn(move || {
            let other = root_scope();
            // Identity instrumentation: force equal epochs before either value
            // from this independent TLS root is published. This is not a clock.
            ACTIVE.with(|active| {
                let mut active = active.borrow_mut();
                let state = active.as_mut().unwrap();
                state.epoch_us = same_values.origin.epoch_us;
                state.epoch = same_values.origin.epoch;
                let origin = Arc::get_mut(&mut state.origin).unwrap();
                origin.epoch_us = state.epoch_us;
                origin.epoch = state.epoch;
            });
            snapshot_tx.send(other.moment().unwrap()).unwrap();
            match release_rx.recv_timeout(HOST_GUARD) {
                Ok(()) | Err(RecvTimeoutError::Disconnected) => {}
                Err(RecvTimeoutError::Timeout) => {
                    panic!("projection check never released its root")
                }
            }
        });
        let foreign = receive(&snapshot_rx);
        assert_eq!(original.elapsed(), foreign.elapsed());
        assert_eq!(original.monotonic_us(), foreign.monotonic_us());
        assert_eq!(original.instant(), foreign.instant());
        assert!(original.between(&foreign, Duration::ZERO).is_err());
        assert!(foreign.between(&original, Duration::ZERO).is_err());
        let original = original.clone();
        threads
            .spawn(move || {
                let own = original.between(&original, Duration::ZERO).unwrap();
                let foreign = foreign.between(&foreign, Duration::ZERO).unwrap();
                let mut observer = ObserverScope::enter(own.clone()).unwrap();
                assert!(observer.observe(foreign).is_err());
                assert_eq!(
                    (monotonic_us(), instant()),
                    (own.monotonic_us(), own.instant())
                );
            })
            .join()
            .unwrap();
        release_tx.send(()).unwrap();
        other.join().unwrap();
    });
    assert_eq!(root.moment().unwrap().elapsed(), Duration::ZERO);
}

#[test]
fn inconsistent_lower_and_upper_endpoints_are_validated_before_projection() {
    let root = root_scope();
    let lower = root.moment().unwrap();
    step(Phase::Run).unwrap();
    let upper = root.moment().unwrap();
    for endpoint in [&lower, &upper] {
        // Invalid-value instrumentation exercises validation; these values
        // cannot be produced by the public snapshot/projection operations.
        let mut wrong_us = endpoint.clone();
        wrong_us.monotonic_us += 1;
        let mut wrong_instant = endpoint.clone();
        wrong_instant.instant += Duration::from_nanos(1);
        let mut wrong_elapsed = endpoint.clone();
        wrong_elapsed.elapsed += Duration::from_nanos(1);
        for invalid in [wrong_us, wrong_instant, wrong_elapsed] {
            let error = if endpoint.elapsed().is_zero() {
                invalid
                    .between(&upper, Duration::from_millis(1))
                    .unwrap_err()
            } else {
                lower
                    .between(&invalid, Duration::from_millis(1))
                    .unwrap_err()
            };
            assert!(error.to_string().contains("does not match"));
        }
    }
    assert_eq!(
        lower
            .between(&upper, Duration::from_millis(1))
            .unwrap()
            .elapsed(),
        Duration::from_millis(1)
    );
    assert_eq!(root.moment().unwrap().elapsed(), upper.elapsed());
}

#[test]
fn projection_preserves_the_last_microsecond_and_rejects_overflowing_endpoints() {
    // Boundary instrumentation supplies representable and malformed endpoints;
    // an interior point between two valid ordered endpoints cannot overflow.
    let epoch = Instant::now();
    let origin = Arc::new(Origin::new(u64::MAX - 1, epoch));
    let lower = Moment::checked(origin.clone(), Duration::ZERO).unwrap();
    let upper = Moment::checked(origin, Duration::from_nanos(1_999)).unwrap();
    let projected = lower.between(&upper, Duration::from_nanos(1_500)).unwrap();
    assert_eq!(projected.monotonic_us(), u64::MAX);
    assert_eq!(projected.instant() - epoch, Duration::from_nanos(1_500));
    let mut overflowing = upper.clone();
    overflowing.elapsed = Duration::from_nanos(2_000);
    assert!(
        lower
            .between(&overflowing, Duration::ZERO)
            .unwrap_err()
            .to_string()
            .contains("monotonic clock overflow")
    );
    overflowing.elapsed = Duration::MAX;
    assert!(
        lower
            .between(&overflowing, Duration::ZERO)
            .unwrap_err()
            .to_string()
            .contains("microseconds overflow")
    );
    assert_eq!(
        lower.between(&upper, upper.elapsed()).unwrap().instant(),
        upper.instant()
    );
}

#[test]
fn projection_rejects_an_endpoint_with_instant_overflow() {
    // Find the platform's representable Instant boundary without advancing time.
    let epoch = Instant::now();
    let (mut fits, mut too_far) = (0, u64::MAX);
    assert!(epoch.checked_add(Duration::from_secs(too_far)).is_none());
    while too_far - fits > 1 {
        let middle = fits + (too_far - fits) / 2;
        if epoch.checked_add(Duration::from_secs(middle)).is_some() {
            fits = middle;
        } else {
            too_far = middle;
        }
    }
    let edge = epoch.checked_add(Duration::from_secs(fits)).unwrap();
    let lower = Moment::checked(Arc::new(Origin::new(0, edge)), Duration::ZERO).unwrap();
    let mut overflowing = lower.clone();
    overflowing.elapsed = Duration::from_secs(1);
    assert!(
        lower
            .between(&overflowing, Duration::ZERO)
            .unwrap_err()
            .to_string()
            .contains("Instant overflow")
    );
    assert_eq!(
        lower.between(&lower, Duration::ZERO).unwrap().instant(),
        edge
    );
}

#[test]
fn an_external_thread_can_project_without_admitting_or_advancing_its_clock() {
    let root = root_scope();
    let lower = root.moment().unwrap();
    step(Phase::Run).unwrap();
    let upper = root.moment().unwrap();
    std::thread::spawn(move || {
        assert_host_clock();
        let projected = lower.between(&upper, Duration::from_millis(1)).unwrap();
        assert_host_clock();
        let mut observer = ObserverScope::enter(lower.clone()).unwrap();
        for _ in 0..16 {
            let repeated = lower.between(&upper, projected.elapsed()).unwrap();
            assert_eq!(repeated.instant(), projected.instant());
            assert_eq!(
                (monotonic_us(), instant()),
                (lower.monotonic_us(), lower.instant())
            );
        }
        observer.observe(projected.clone()).unwrap();
        observer.observe(projected.clone()).unwrap();
        assert_eq!(
            (monotonic_us(), instant()),
            (projected.monotonic_us(), projected.instant())
        );
        observer.observe(upper.clone()).unwrap();
        assert!(observer.observe(projected).is_err());
        assert_eq!(
            (monotonic_us(), instant()),
            (upper.monotonic_us(), upper.instant())
        );
        drop(observer);
        assert_host_clock();
    })
    .join()
    .unwrap();
    assert_eq!(root.moment().unwrap().elapsed(), Duration::from_millis(2));
}

fn assert_invalidation_freezes_observer(lower: Moment, upper: Moment, invalidate: impl FnOnce()) {
    std::thread::scope(|threads| {
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (check_tx, check_rx) = mpsc::channel();
        let observer = threads.spawn(move || {
            let projected = lower.between(&upper, Duration::from_millis(1)).unwrap();
            let mut observer = ObserverScope::enter(projected.clone()).unwrap();
            entered_tx.send(()).unwrap();
            receive(&check_rx);
            assert!(lower.between(&upper, projected.elapsed()).is_err());
            assert!(projected.between(&projected, projected.elapsed()).is_err());
            assert_eq!(
                (monotonic_us(), instant()),
                (projected.monotonic_us(), projected.instant())
            );
            assert_eq!(projected.elapsed(), Duration::from_millis(1));
            assert!(observer.observe(projected.clone()).is_err());
            assert_eq!(
                (monotonic_us(), instant()),
                (projected.monotonic_us(), projected.instant())
            );
            drop(observer);
            assert_host_clock();
            assert!(ObserverScope::enter(projected).is_err());
        });
        receive(&entered_rx);
        invalidate();
        check_tx.send(()).unwrap();
        observer.join().unwrap();
    });
}

#[test]
fn normal_root_drop_invalidates_projection_but_keeps_admitted_reads_frozen() {
    let root = root_scope();
    let lower = root.moment().unwrap();
    step(Phase::Run).unwrap();
    let upper = root.moment().unwrap();
    assert_invalidation_freezes_observer(lower, upper, || drop(root));
    assert_host_clock();
}

#[test]
fn callback_panic_invalidates_projection_but_keeps_admitted_reads_frozen() {
    let root = Scope::enter(HOST_GUARD).unwrap();
    let calls = Rc::new(Cell::new(0));
    let counted = calls.clone();
    root.set_pump(move |_| {
        counted.set(counted.get() + 1);
        if counted.get() == 2 {
            panic!("injected projection origin failure");
        }
        Ok(())
    })
    .unwrap();
    let lower = root.moment().unwrap();
    step(Phase::Run).unwrap();
    let upper = root.moment().unwrap();
    assert_invalidation_freezes_observer(lower, upper, || {
        let failure = catch_unwind(|| step(Phase::Run)).unwrap_err();
        assert_eq!(
            failure.downcast_ref::<&str>(),
            Some(&"injected projection origin failure")
        );
        assert!(root.moment().is_err());
        assert_host_clock();
    });
    assert_eq!(calls.get(), 2);
}

#[test]
fn root_thread_teardown_invalidates_both_projection_endpoints() {
    let (lower, upper) = std::thread::spawn(|| {
        let root = root_scope();
        let lower = root.moment().unwrap();
        step(Phase::Run).unwrap();
        let upper = root.moment().unwrap();
        // TLS teardown owns invalidation even if the thread forgets its guard.
        std::mem::forget(root);
        (lower, upper)
    })
    .join()
    .unwrap();
    assert!(lower.between(&upper, Duration::from_millis(1)).is_err());
    assert_eq!(lower.elapsed(), Duration::ZERO);
    assert_eq!(upper.elapsed(), Duration::from_millis(2));
    assert_host_clock();
}
