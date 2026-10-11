# Wakeup-latency probe: sleep 1 ms repeatedly for N seconds and report how late
# each wakeup was. Measures VM scheduling stalls, not lamp-live.
import time, sys
n = float(sys.argv[1]) if len(sys.argv) > 1 else 60
late = []
end = time.monotonic() + n
while time.monotonic() < end:
    t = time.monotonic(); time.sleep(0.001); late.append((time.monotonic() - t - 0.001) * 1000)
late.sort()
q = lambda p: late[min(len(late) - 1, int(p * len(late)))]
print(f"wakeups={len(late)} p50={q(.5):.2f}ms p99={q(.99):.2f}ms p99.9={q(.999):.2f}ms max={late[-1]:.1f}ms >10ms={sum(x>10 for x in late)} >30ms={sum(x>30 for x in late)} >100ms={sum(x>100 for x in late)}")
