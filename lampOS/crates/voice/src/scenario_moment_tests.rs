use super::*;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};

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
fn snapshot_reads_and_observer_adoption_do_not_run_peers() {
    let root = Scope::enter(HOST_GUARD).unwrap();
    let calls = Rc::new(Cell::new(0));
    let counted = calls.clone();
    root.set_pump(move |_| {
        counted.set(counted.get() + 1);
        Ok(())
    })
    .unwrap();
    let initial = root.moment().unwrap();
    for _ in 0..16 {
        let snapshot = root.moment().unwrap();
        assert_eq!(snapshot.elapsed(), Duration::ZERO);
        assert_eq!(snapshot.monotonic_us(), initial.monotonic_us());
        assert_eq!(snapshot.instant(), initial.instant());
        assert_eq!(
            (monotonic_us(), instant()),
            (snapshot.monotonic_us(), snapshot.instant())
        );
    }
    assert_eq!(calls.get(), 0);
    step(Phase::Run).unwrap();
    let later = root.moment().unwrap();
    assert_eq!(later.elapsed(), Duration::from_millis(2));
    assert_eq!(later.monotonic_us() - initial.monotonic_us(), 2_000);
    assert_eq!(
        later.instant() - initial.instant(),
        Duration::from_millis(2)
    );
    std::thread::spawn(move || {
        assert_host_clock();
        let mut observer = ObserverScope::enter(initial.clone()).unwrap();
        // The root has already advanced. The observer stays on its admitted
        // snapshot, and repeated admission of the same live moment is harmless.
        for _ in 0..16 {
            assert!(is_active());
            assert_eq!(monotonic_us(), initial.monotonic_us());
            assert_eq!(instant(), initial.instant());
        }
        observer.observe(initial).unwrap();
        observer.observe(later.clone()).unwrap();
        assert_eq!(monotonic_us(), later.monotonic_us());
        assert_eq!(instant(), later.instant());
        drop(observer);
        assert_host_clock();
    })
    .join()
    .unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!(root.moment().unwrap().elapsed(), Duration::from_millis(2));
}

#[test]
fn observer_rejects_nesting_root_pumps_and_fault_injection() {
    let root = root_scope();
    let initial = root.moment().unwrap();
    assert!(ObserverScope::enter(initial.clone()).is_err());
    std::thread::spawn(move || {
        let observer = ObserverScope::enter(initial.clone()).unwrap();
        assert!(Scope::enter(HOST_GUARD).is_err());
        assert!(ObserverScope::enter(initial.clone()).is_err());
        assert!(step(Phase::Run).is_err());
        assert!(
            step(Phase::Shutdown {
                deadline: Instant::now() + HOST_GUARD,
            })
            .is_err()
        );
        assert!(elapse_for_fault(Duration::from_nanos(1)).is_err());
        assert_eq!(monotonic_us(), initial.monotonic_us());
        assert_eq!(instant(), initial.instant());
        drop(observer);
        assert_host_clock();
        assert!(!step(Phase::Run).unwrap());
    })
    .join()
    .unwrap();
    assert_eq!(root.moment().unwrap().elapsed(), Duration::ZERO);
}

#[test]
fn submicrosecond_snapshots_preserve_order_even_when_integer_time_is_equal() {
    let root = Rc::new(Scope::enter(HOST_GUARD).unwrap());
    let weak_root = Rc::downgrade(&root);
    let snapshots = Rc::new(RefCell::new(None));
    let captured = snapshots.clone();
    root.set_pump(move |_| {
        let root = weak_root.upgrade().unwrap();
        elapse_for_fault(Duration::from_nanos(900))?;
        let first = root.moment()?;
        elapse_for_fault(Duration::from_nanos(50))?;
        *captured.borrow_mut() = Some((first, root.moment()?));
        Ok(())
    })
    .unwrap();
    step(Phase::Run).unwrap();
    let (first, second) = snapshots.borrow_mut().take().unwrap();
    assert_eq!(first.elapsed(), Duration::from_nanos(900));
    assert_eq!(second.elapsed(), Duration::from_nanos(950));
    assert_eq!(first.monotonic_us(), second.monotonic_us());
    assert_eq!(second.instant() - first.instant(), Duration::from_nanos(50));
    assert_eq!(
        root.moment().unwrap().elapsed(),
        Duration::from_nanos(2_000_950)
    );
    std::thread::spawn(move || {
        let mut observer = ObserverScope::enter(first.clone()).unwrap();
        observer.observe(second.clone()).unwrap();
        assert!(observer.observe(first).is_err());
        assert_eq!(monotonic_us(), second.monotonic_us());
        assert_eq!(instant(), second.instant());
        observer.observe(second).unwrap();
    })
    .join()
    .unwrap();
}

#[test]
fn unrelated_roots_with_identical_epoch_values_are_still_distinct() {
    let root = root_scope();
    let original = root.moment().unwrap();
    std::thread::scope(|threads| {
        let (snapshot_tx, snapshot_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::channel();
        let same_values = original.clone();
        let other = threads.spawn(move || {
            let other = root_scope();
            // Force the collision the identity must survive, before this root
            // has published any moment. Both are actual independent TLS roots.
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
                Err(RecvTimeoutError::Timeout) => panic!("collision check never released its root"),
            }
        });
        let foreign = receive(&snapshot_rx);
        let original = original.clone();
        threads
            .spawn(move || {
                assert_eq!(original.elapsed(), foreign.elapsed());
                assert_eq!(original.monotonic_us(), foreign.monotonic_us());
                assert_eq!(original.instant(), foreign.instant());
                let mut observer = ObserverScope::enter(original.clone()).unwrap();
                assert!(observer.observe(foreign).is_err());
                assert_eq!(monotonic_us(), original.monotonic_us());
                assert_eq!(instant(), original.instant());
            })
            .join()
            .unwrap();
        release_tx.send(()).unwrap();
        other.join().unwrap();
    });
}

fn observing_until_invalidated(moment: Moment) -> (Sender<()>, std::thread::JoinHandle<()>) {
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (check_tx, check_rx) = mpsc::channel();
    let thread = std::thread::spawn(move || {
        let mut observer = ObserverScope::enter(moment.clone()).unwrap();
        entered_tx.send(()).unwrap();
        receive(&check_rx);
        // Invalidation is not a clock reading and must not make time jump.
        assert!(is_active());
        assert_eq!(monotonic_us(), moment.monotonic_us());
        assert_eq!(instant(), moment.instant());
        assert!(observer.observe(moment.clone()).is_err());
        assert_eq!(monotonic_us(), moment.monotonic_us());
        assert_eq!(instant(), moment.instant());
        drop(observer);
        assert_host_clock();
        assert!(ObserverScope::enter(moment).is_err());
        assert_host_clock();
    });
    receive(&entered_rx);
    (check_tx, thread)
}

#[test]
fn root_drop_invalidates_adoption_but_preserves_an_observers_last_reading() {
    let root = root_scope();
    let moment = root.moment().unwrap();
    let (check, observer) = observing_until_invalidated(moment.clone());
    drop(root);
    assert_host_clock();
    // A snapshot remains a truthful immutable value after its root disappears.
    assert_eq!(moment.elapsed(), Duration::ZERO);
    check.send(()).unwrap();
    observer.join().unwrap();
}

#[test]
fn callback_panic_invalidates_adoption_and_stale_scope_cannot_snapshot_successor() {
    let old = Scope::enter(HOST_GUARD).unwrap();
    let moment = old.moment().unwrap();
    old.set_pump(|_| panic!("injected root failure")).unwrap();
    let (check, observer) = observing_until_invalidated(moment);
    assert!(catch_unwind(|| step(Phase::Run)).is_err());
    assert_host_clock();
    assert!(old.moment().is_err());
    let successor = root_scope();
    assert!(old.moment().is_err());
    drop(old);
    assert_eq!(successor.moment().unwrap().elapsed(), Duration::ZERO);
    check.send(()).unwrap();
    observer.join().unwrap();
    step(Phase::Run).unwrap();
    assert_eq!(
        successor.moment().unwrap().elapsed(),
        Duration::from_millis(2)
    );
}

#[test]
fn rejected_inconsistent_moment_leaves_the_admitted_clock_unchanged() {
    let root = root_scope();
    let valid = root.moment().unwrap();
    std::thread::spawn(move || {
        let mut wrong_us = valid.clone();
        wrong_us.monotonic_us += 1;
        let mut wrong_instant = valid.clone();
        wrong_instant.instant += Duration::from_nanos(1);
        let mut wrong_elapsed = valid.clone();
        wrong_elapsed.elapsed += Duration::from_nanos(1);
        for invalid in [&wrong_us, &wrong_instant, &wrong_elapsed] {
            assert!(ObserverScope::enter(invalid.clone()).is_err());
            assert_host_clock();
        }
        let mut observer = ObserverScope::enter(valid.clone()).unwrap();
        for invalid in [wrong_us, wrong_instant, wrong_elapsed] {
            assert!(observer.observe(invalid).is_err());
            assert_eq!(monotonic_us(), valid.monotonic_us());
            assert_eq!(instant(), valid.instant());
        }
        observer.observe(valid).unwrap();
    })
    .join()
    .unwrap();
}

#[test]
fn root_snapshot_rejects_changed_origin_values() {
    let root = root_scope();
    ACTIVE.with(|active| active.borrow_mut().as_mut().unwrap().epoch_us += 1);
    assert!(root.moment().is_err());
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        let state = active.as_mut().unwrap();
        state.epoch_us = state.origin.epoch_us;
        state.epoch += Duration::from_nanos(1);
    });
    assert!(root.moment().is_err());
}

#[test]
fn checked_moments_keep_submicroseconds_at_the_last_representable_microsecond() {
    let epoch = Instant::now();
    let origin = Arc::new(Origin::new(u64::MAX - 1, epoch));
    let last = Moment::checked(origin.clone(), Duration::from_nanos(1_999)).unwrap();
    assert_eq!(last.monotonic_us(), u64::MAX);
    assert_eq!(last.instant() - epoch, Duration::from_nanos(1_999));
    assert_eq!(last.elapsed(), Duration::from_nanos(1_999));
    assert!(Moment::checked(origin, Duration::from_nanos(2_000)).is_err());
    assert!(Moment::checked(Arc::new(Origin::new(0, epoch)), Duration::MAX).is_err());
}

#[test]
fn checked_moments_reject_instant_overflow_even_when_microseconds_fit() {
    // Find this platform's actual Instant boundary without advancing a clock or
    // assuming a particular OS representation of Instant.
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
    let origin = Arc::new(Origin::new(0, edge));
    assert!(Moment::checked(origin.clone(), Duration::ZERO).is_ok());
    assert!(Moment::checked(origin, Duration::from_secs(1)).is_err());
}

#[test]
fn observer_is_thread_local_and_does_not_affect_an_independent_root_scope() {
    let first = root_scope();
    step(Phase::Run).unwrap();
    let snapshot = first.moment().unwrap();
    std::thread::spawn(move || {
        let observer = ObserverScope::enter(snapshot.clone()).unwrap();
        std::thread::spawn(|| {
            assert_host_clock();
            let independent = root_scope();
            for _ in 0..3 {
                step(Phase::Run).unwrap();
            }
            assert_eq!(
                independent.moment().unwrap().elapsed(),
                Duration::from_millis(6)
            );
            drop(independent);
            assert_host_clock();
        })
        .join()
        .unwrap();
        assert_eq!(monotonic_us(), snapshot.monotonic_us());
        assert_eq!(instant(), snapshot.instant());
        drop(observer);
        assert_host_clock();
    })
    .join()
    .unwrap();
    assert_eq!(first.moment().unwrap().elapsed(), Duration::from_millis(2));
}

#[test]
fn root_thread_teardown_invalidates_a_snapshot_when_its_guard_was_forgotten() {
    let snapshot = std::thread::spawn(|| {
        let root = root_scope();
        let snapshot = root.moment().unwrap();
        // TLS teardown still owns the root state if a caller leaks its guard.
        std::mem::forget(root);
        snapshot
    })
    .join()
    .unwrap();
    assert_eq!(snapshot.elapsed(), Duration::ZERO);
    assert!(ObserverScope::enter(snapshot).is_err());
    assert_host_clock();
}

#[test]
fn observer_unwind_restores_host_clock_without_invalidating_its_root() {
    let root = root_scope();
    let snapshot = root.moment().unwrap();
    std::thread::spawn(move || {
        let failing = snapshot.clone();
        assert!(
            catch_unwind(move || {
                let _observer = ObserverScope::enter(failing).unwrap();
                panic!("injected observer failure");
            })
            .is_err()
        );
        assert_host_clock();
        let observer = ObserverScope::enter(snapshot.clone()).unwrap();
        assert_eq!(instant(), snapshot.instant());
        drop(observer);
        assert_host_clock();
    })
    .join()
    .unwrap();
    step(Phase::Run).unwrap();
    assert_eq!(root.moment().unwrap().elapsed(), Duration::from_millis(2));
}
