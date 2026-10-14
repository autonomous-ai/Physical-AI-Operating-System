"""Streaming noise evidence is exact, bounded, per-capture, and fail-closed."""

import threading

import pytest

from hal.drivers.voice._internal.noise_evidence import NoiseEvidence


BLOCK = 1024


def block(value):
    return value.to_bytes(2, 'little', signed=True) * 512


def expected(scores, threshold=.15):
    voiced = [score >= threshold for score in scores]
    ratio = sum(voiced) / len(scores)
    if any(voiced):
        first = voiced.index(True)
        last = len(voiced) - 1 - voiced[::-1].index(True)
        span = voiced[first:last + 1]
        return max(scores), sum(scores) / len(scores), ratio, sum(span) / len(span), len(span) * 512 / 16000
    return max(scores), sum(scores) / len(scores), ratio, ratio, 0.0


@pytest.fixture
def workers():
    active = []
    releases = []
    def create(factory, **kwargs):
        worker = NoiseEvidence(factory, max_samples=kwargs.pop('max_samples', 512 * 20), **kwargs)
        active.append(worker)
        return worker
    yield create, releases
    for release in releases:
        release.set()
    for worker in active:
        worker.close()
        if worker._thread is not None:
            assert worker.done.wait(3)
            worker._thread.join(1)
            assert not worker._thread.is_alive()


def test_arbitrary_chunk_boundaries_match_whole_input_for_every_aligned_prefix(workers):
    create, _ = workers
    scores = [0, .15, .9, .02, .3, .1, .8, 0, 0, .2]
    pcm = b''.join(block(index) for index in range(len(scores)))
    seen = []
    def feed(data):
        seen.append(data)
        return scores[len(seen) - 1]
    worker = create(lambda: feed).start()
    # Split samples at different offsets; no intermediate tail may be padded.
    cuts = [2, 1028, 1534, 4098, 5122, len(pcm)]
    offset = 0
    for end in cuts:
        assert worker.append(pcm[offset:end])
        offset = end
    worker.finish()
    assert worker.done.wait(1)
    assert seen == [block(index) for index in range(10)]
    for count in range(1, 11):
        assert worker.metrics(pcm[:count * BLOCK]) == expected(scores[:count])
    assert len(worker._prefixes) == 10
    assert worker.metrics(pcm[:-2]) is None
    assert worker.metrics(block(99) + pcm[BLOCK:]) is None
    assert worker.metrics(pcm[BLOCK:]) is None  # Equal bytes from a suffix aren't a prefix.


def test_partial_final_tail_never_generates_padded_score(workers):
    create, _ = workers
    seen = []
    worker = create(lambda: lambda pcm: (seen.append(pcm), .2)[1]).start()
    pcm = block(1) + b'\x02\x00' * 33
    assert worker.append(pcm)
    worker.finish()
    assert worker.done.wait(1)
    assert seen == [block(1)]
    assert worker.metrics(pcm) is None
    assert worker.metrics(pcm[:BLOCK]) == expected([.2])


def test_factory_runs_off_capture_thread_and_append_does_not_wait_for_loading(workers):
    create, releases = workers
    entered, release = threading.Event(), threading.Event()
    releases.append(release)
    thread_ids = []
    def factory():
        thread_ids.append(threading.get_ident())
        entered.set()
        assert release.wait(2)
        return lambda _: .7
    worker = create(factory).start()
    assert entered.wait(1)
    assert worker.append(block(1))
    assert worker.metrics(block(1)) is None
    worker.finish()
    assert not worker.done.is_set()
    release.set()
    assert worker.done.wait(1)
    assert thread_ids != [threading.get_ident()]
    assert worker.metrics(block(1)) == expected([.7])


def test_inflight_pcm_counts_against_queue_bound_and_overflow_invalidates_all(workers):
    create, releases = workers
    entered, release = threading.Event(), threading.Event()
    releases.append(release)
    def feed(_):
        entered.set()
        assert release.wait(2)
        return .9
    worker = create(lambda: feed, max_queue_bytes=2 * BLOCK).start()
    assert worker.append(block(1))
    assert entered.wait(1)
    assert worker.append(block(2))
    assert not worker.append(block(3))
    assert worker.metrics(block(1)) is None
    release.set()
    assert worker.done.wait(1)
    assert not worker._prefixes


def test_total_limit_is_independent_of_released_queue_capacity(workers):
    create, _ = workers
    worker = create(lambda: lambda _: .7, max_samples=512).start()
    assert worker.append(block(1))
    with worker._condition:
        assert worker._condition.wait_for(lambda: worker._pending_bytes == 0, timeout=1)
    assert not worker.append(block(2))
    assert worker.done.wait(1)
    assert worker.metrics(block(1)) is None


@pytest.mark.parametrize('score', [float('nan'), float('inf'), -.1, 1.01])
def test_invalid_score_never_becomes_fallback_evidence(workers, score):
    create, _ = workers
    worker = create(lambda: lambda _: score).start()
    worker.append(block(1))
    worker.finish()
    assert worker.done.wait(1)
    assert worker.metrics(block(1)) is None
    assert not worker._prefixes


@pytest.mark.parametrize('where', ['factory', 'feed'])
def test_classifier_errors_retire_all_evidence(workers, where):
    create, _ = workers
    def fail(*_):
        raise RuntimeError('model unavailable')
    worker = create(fail if where == 'factory' else lambda: fail).start()
    worker.append(block(1))
    worker.finish()
    assert worker.done.wait(1)
    assert worker.metrics(block(1)) is None


def test_cancel_during_inference_is_immediate_and_late_result_is_not_published(workers):
    create, releases = workers
    entered, release = threading.Event(), threading.Event()
    releases.append(release)
    def feed(_):
        entered.set()
        assert release.wait(2)
        return .8
    worker = create(lambda: feed).start()
    worker.append(block(1))
    assert entered.wait(1)
    worker.close()
    assert not worker.done.is_set()
    assert not worker.append(block(2))
    assert worker.metrics(block(1)) is None
    release.set()
    assert worker.done.wait(1)
    assert not worker._prefixes


def test_new_capture_cannot_borrow_previous_capture_evidence(workers):
    create, _ = workers
    first = create(lambda: lambda _: .8).start()
    first.append(block(1))
    first.finish()
    assert first.done.wait(1)
    second = create(lambda: lambda _: .1).start()
    assert second.metrics(block(1)) is None
    second.append(block(1))
    second.finish()
    assert second.done.wait(1)
    assert first.metrics(block(1)) != second.metrics(block(1))
    first.close()
    assert first.metrics(block(1)) is None
    assert second.metrics(block(1)) == expected([.1])


def test_measured_silence_is_real_zero_evidence_not_failure_sentinel(workers):
    create, _ = workers
    worker = create(lambda: lambda _: 0.0).start()
    worker.append(block(0))
    worker.finish()
    assert worker.done.wait(1)
    assert worker.metrics(block(0)) == (0, 0, 0, 0, 0)
    assert worker.metrics(b'') is None
    assert not worker.append(block(0))


def test_processed_prefix_available_while_later_inference_runs(workers):
    create, releases = workers
    entered, release = threading.Event(), threading.Event()
    releases.append(release)
    calls = [0]
    def feed(_):
        calls[0] += 1
        if calls[0] == 2:
            entered.set()
            assert release.wait(2)
        return .7
    worker = create(lambda: feed).start()
    worker.append(block(1) + block(2))
    assert entered.wait(1)
    assert not worker.done.is_set()
    assert worker.metrics(block(1)) == expected([.7])
    assert worker.metrics(block(1) + block(2)) is None
    worker.finish()
    release.set()
    assert worker.done.wait(1)


def test_failure_after_valid_prefix_retires_earlier_evidence(workers):
    create, _ = workers
    def feed(data):
        if data == block(2):
            raise RuntimeError('later inference failed')
        return .7
    worker = create(lambda: feed).start()
    worker.append(block(1) + block(2))
    worker.finish()
    assert worker.done.wait(1)
    assert worker.metrics(block(1)) is None
    assert not worker._prefixes


def test_all_float32_probability_prefixes_match_reference_sum_exactly(workers):
    import random
    import struct
    create, _ = workers
    rng = random.Random(37)
    scores = [struct.unpack('f', struct.pack('f', rng.random() ** 10))[0] for _ in range(200)]
    pcm = b''.join(block(index) for index in range(len(scores)))
    worker = create(lambda: lambda data: scores[int.from_bytes(data[:2], 'little')], max_samples=512 * len(scores)).start()
    worker.append(pcm)
    worker.finish()
    assert worker.done.wait(2)
    for count in range(1, len(scores) + 1):
        assert worker.metrics(pcm[:count * BLOCK]) == expected(scores[:count])
    assert len(worker._prefixes) == 200


def test_odd_pcm_is_explicit_failure_not_a_dropped_half_sample(workers):
    create, _ = workers
    worker = create(lambda: lambda _: .7).start()
    assert not worker.append(b'x')
    assert worker.done.wait(1)
    assert worker.metrics(block(1)) is None
