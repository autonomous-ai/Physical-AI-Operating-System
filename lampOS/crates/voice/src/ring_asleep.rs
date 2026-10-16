//! The asleep face while the lamp cannot hear (#712, `LAMP_RING_ASLEEP=on`).
//!
//! The ring worker lives and dies with each active cycle, and a muted lamp
//! has none, so its ring was dark: to a person that reads as off, or broken.
//! While the service waits out a mute or a closed privacy switch, this paints
//! the ring's asleep face ([`lamp_ring::asleep`]) from a thread of its own: a
//! faint warm glow at the bottom that breathes slowly. It is black and the
//! device is closed again before the wait returns, so the next cycle's ring
//! worker is the only writer.

use lamp_ring::{
    ChannelCeiling, FrameSink,
    asleep::{AsleepRing, PEAK},
};
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

/// The switch: the asleep face is shown only with `LAMP_RING_ASLEEP=on`.
pub const SWITCH: &str = "LAMP_RING_ASLEEP";
/// How long a wait lasts before the face shows. The wait at every cycle's
/// start, which only checks the privacy switch on the way to listening, is
/// over long before this, so a lamp that is not muted never shows it.
pub const AFTER: Duration = Duration::from_millis(1_500);
/// It fades in over this, as eyes close.
pub const FADE_IN: Duration = Duration::from_millis(1_500);
/// One breath, in and out: slower than the resting face's.
pub const BREATH: Duration = Duration::from_secs(6);
/// One frame this often: enough for a slow breath.
const FRAME: Duration = Duration::from_millis(50);
/// How often a waiting painter looks at its stop flag.
const STEP: Duration = Duration::from_millis(10);

/// Whether the switch is on in this process's environment.
pub fn enabled_from_environment() -> bool {
    enabled(std::env::var(SWITCH).ok().as_deref())
}

/// Whether the switch's value turns the face on: only `on`.
pub fn enabled(value: Option<&str>) -> bool {
    value == Some("on")
}

/// How far up a breath (0 to 1) the face is `since` it began to show.
pub fn breath_at(since: Duration) -> f32 {
    let phase = since.as_secs_f32() / BREATH.as_secs_f32();
    0.5 - 0.5 * (std::f32::consts::TAU * phase).cos()
}

/// The ceiling `since` the face began to show: rising from 0 to `ceiling`
/// over [`FADE_IN`], never above what the face itself reaches.
pub fn ceiling_at(since: Duration, ceiling: ChannelCeiling) -> ChannelCeiling {
    let top = ceiling.get().min(PEAK.red);
    let share = (since.as_secs_f32() / FADE_IN.as_secs_f32()).clamp(0.0, 1.0);
    ChannelCeiling::new(u16::from((f32::from(top) * share).round() as u8))
        .expect("a share of a valid ceiling is valid")
}

/// The asleep face painting from its own thread until stopped. Stopping, or
/// dropping, waits for the ring to be black and the device closed.
pub struct Asleep {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Asleep {
    /// Start painting on lamp-4ace's ring device under `ceiling`.
    #[cfg(target_os = "linux")]
    pub fn start(ceiling: u16) -> Option<Self> {
        let ceiling = ChannelCeiling::new(ceiling).ok()?;
        Some(Self::start_with(lamp_ring::linux::open_asleep, ceiling))
    }

    /// Start painting on whatever `open` gives, after [`AFTER`].
    pub fn start_with<S, F>(open: F, ceiling: ChannelCeiling) -> Self
    where
        S: FrameSink + 'static,
        F: FnOnce() -> io::Result<AsleepRing<S>> + Send + 'static,
    {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name("ring-asleep".into())
            .spawn(move || {
                if let Err(error) = paint(open, ceiling, &flag) {
                    eprintln!("lamp-voice ring: the asleep face stopped: {error}");
                }
            })
            .ok();
        Self { stop, thread }
    }

    /// Stop painting: the ring is black and the device closed on return.
    pub fn stop(mut self) {
        self.finish();
    }

    fn finish(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Asleep {
    fn drop(&mut self) {
        self.finish();
    }
}

/// Wait [`AFTER`]; unless stopped by then, open the ring and paint the
/// breathing face until stopped, then black.
fn paint<S: FrameSink>(
    open: impl FnOnce() -> io::Result<AsleepRing<S>>,
    ceiling: ChannelCeiling,
    stop: &AtomicBool,
) -> io::Result<()> {
    if !wait(AFTER, stop) {
        return Ok(());
    }
    let mut ring = open()?;
    let began = Instant::now();
    while !stop.load(Ordering::SeqCst) {
        let since = began.elapsed();
        if let Err(error) = ring.show(breath_at(since), ceiling_at(since, ceiling)) {
            let _ = ring.shutdown();
            return Err(error);
        }
        wait(FRAME, stop);
    }
    ring.shutdown()
}

/// Sleep `length` in short steps; false if stopped meanwhile.
fn wait(length: Duration, stop: &AtomicBool) -> bool {
    let until = Instant::now() + length;
    while Instant::now() < until {
        if stop.load(Ordering::SeqCst) {
            return false;
        }
        std::thread::sleep(STEP.min(until.saturating_duration_since(Instant::now())));
    }
    !stop.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lamp_ring::EncodedFrame;
    use std::sync::Mutex;

    #[derive(Clone, Default)]
    struct Frames(Arc<Mutex<Vec<EncodedFrame>>>);
    impl FrameSink for Frames {
        fn write_frame(&mut self, frame: &EncodedFrame) -> io::Result<()> {
            self.0.lock().unwrap().push(frame.clone());
            Ok(())
        }
    }

    fn ceiling(value: u16) -> ChannelCeiling {
        ChannelCeiling::new(value).unwrap()
    }

    #[test]
    fn a_wait_stopped_before_the_delay_never_opens_the_ring() {
        let opened = Arc::new(AtomicBool::new(false));
        let flag = opened.clone();
        let asleep = Asleep::start_with(
            move || {
                flag.store(true, Ordering::SeqCst);
                AsleepRing::new(Frames::default())
            },
            ceiling(48),
        );
        std::thread::sleep(Duration::from_millis(100));
        asleep.stop();
        assert!(!opened.load(Ordering::SeqCst));
    }

    #[test]
    fn a_long_wait_paints_and_ends_black_with_the_painter_gone() {
        let frames = Frames::default();
        let sink = frames.clone();
        let asleep = Asleep::start_with(move || AsleepRing::new(sink), ceiling(48));
        std::thread::sleep(AFTER + Duration::from_millis(400));
        asleep.stop();
        let frames = frames.0.lock().unwrap();
        // Black on opening, some faces, black on closing; nothing after.
        assert!(frames.len() >= 4, "{}", frames.len());
        assert_eq!(frames[0], EncodedFrame::black());
        assert_eq!(*frames.last().unwrap(), EncodedFrame::black());
        assert!(frames[1..frames.len() - 1]
            .iter()
            .any(|frame| *frame != EncodedFrame::black()));
    }

    #[test]
    fn it_fades_in_and_breathes_slowly() {
        assert_eq!(ceiling_at(Duration::ZERO, ceiling(48)).get(), 0);
        assert_eq!(ceiling_at(FADE_IN, ceiling(48)).get(), PEAK.red);
        assert_eq!(ceiling_at(FADE_IN * 3, ceiling(6)).get(), 6);
        assert!(breath_at(Duration::ZERO) < 0.01);
        assert!(breath_at(BREATH / 2) > 0.99);
        // A 50 ms frame never moves a breath by more than a few hundredths.
        let mut since = Duration::ZERO;
        while since < BREATH {
            let next = since + FRAME;
            assert!((breath_at(next) - breath_at(since)).abs() < 0.03);
            since = next;
        }
    }

    #[test]
    fn the_switch_is_off_unless_set_to_on() {
        assert!(enabled(Some("on")));
        for value in [None, Some(""), Some("off"), Some("ON"), Some("1")] {
            assert!(!enabled(value), "{value:?}");
        }
    }
}
