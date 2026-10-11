//! How much of what the microphone picked up is what the speaker was given.
//!
//! One observer sits beside the echo canceller and sees the same three
//! streams in the same order: the PCM the speaker device accepted, the
//! microphone block before cancellation and the same block after it. For
//! every capture block it reports measurements, never a verdict:
//!
//! - whether accepted speaker PCM was recently non-silent;
//! - the level of the capture before and after cancellation;
//! - once an alignment between capture and render has been found, the share
//!   of the capture's energy, and of the cancelled signal's energy, that the
//!   render at that alignment explains.
//!
//! The second share is the one a speech detector cannot see for itself: a
//! detector fed the cancelled signal fires on leaked playback exactly as it
//! fires on a person. A high share says that what remains is still mostly
//! Lamp's own voice. A low share says it is something the render does not
//! explain: a person, another device, or noise. Which of those it is, and
//! whether Lamp was being addressed, this module cannot know.
//!
//! No alignment, no claim: while the render is silent, or active but not yet
//! matched, the echo part of the evidence is absent rather than guessed.
//!
//! Everything is integer arithmetic on bounded rings at a 4 kHz analysis
//! rate. There is no allocation after construction, no I/O and no clock.
use crate::{CAPTURE_SAMPLES, CaptureBlock, RENDER_SAMPLES, RenderBlock};
use serde::{Deserialize, Serialize};

/// Analysis rate. Both streams are reduced to it by plain averaging, six
/// render samples or four capture samples to one, and each result is then
/// averaged with the one before it. The two streams get the same frequency
/// response, and little is left near half the analysis rate, where a delay
/// that is not a whole number of samples could not be followed.
pub const ANALYSIS_RATE: u32 = 4_000;
/// Analysis samples per 10 ms block of either stream.
const BLOCK: usize = 40;
/// Samples compared: the last 120 ms of capture.
const WINDOW: usize = 12 * BLOCK;
/// Render history kept, a little over one second.
const HISTORY: usize = 4_096;
/// Longest delay searched from accepted render to its capture: 400 ms.
const MAX_DELAY: i64 = 40 * BLOCK as i64;
/// Alignments tried per capture block while searching. With the three tried
/// around a known alignment this bounds the work of one block.
const SWEEP: i64 = 32;
/// A window quieter than this RMS, in 16-bit sample units, is silence
/// (-60 dB relative to full scale).
const SILENT_RMS: u64 = 33;
/// Share of capture energy an alignment must explain to be adopted, and to
/// count as still holding, in percent.
pub const MATCH_PERCENT: u8 = 50;
/// How far a candidate must beat the current alignment to replace it.
const REPLACE_MARGIN_PERCENT: u8 = 15;
/// Blocks of recent render activity that are remembered: 500 ms.
pub const ACTIVITY_BLOCKS: u8 = 50;

/// RMS level in tenths of a decibel relative to full scale, measured in the
/// analysis band (below about 2 kHz) over the last 120 ms and about the mean.
/// Digital silence is reported as [`Level::FLOOR`], not as negative infinity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Level(i16);
impl Level {
    pub const FLOOR: Self = Self(-960);
    pub fn tenths_of_db(self) -> i16 {
        self.0
    }
    /// From the energy of one analysis window about its mean.
    fn of(energy: f64) -> Self {
        if energy <= 0.0 {
            return Self::FLOOR;
        }
        let db = 10.0 * (energy / WINDOW as f64 / (32_768.0 * 32_768.0)).log10();
        Self(((db * 10.0).round() as i16).max(Self::FLOOR.0))
    }
}

/// What the render at the matched alignment accounts for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EchoMatch {
    /// Level of the accepted render over the matched window.
    pub render_level: Level,
    /// Percent of the capture's energy, before cancellation, explained by
    /// the render at this alignment.
    pub capture_explained: u8,
    /// The same for the signal after cancellation: how much of what the
    /// speech detector and the provider receive is still playback.
    pub residual_explained: u8,
    /// Capture blocks since this alignment last explained at least
    /// [`MATCH_PERCENT`] of the capture. Zero means this block.
    pub alignment_age_blocks: u16,
}

/// Measurements for one capture block. See the module text for what they do
/// and do not establish.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EchoEvidence {
    /// Capture blocks observed since construction or reset, this one included.
    pub blocks: u32,
    /// Of the last [`ACTIVITY_BLOCKS`] render blocks, how many were not silent.
    pub render_active_blocks: u8,
    /// Capture level before cancellation, over the last 120 ms.
    pub capture_level: Level,
    /// Level after cancellation, over the same 120 ms.
    pub residual_level: Level,
    /// Present only while non-silent render is matched to the capture.
    /// Absent means "not determined", never "no echo".
    pub echo: Option<EchoMatch>,
}

/// Render history. The length is a power of two so indexing is a mask.
struct History {
    samples: [i16; HISTORY],
    /// Samples ever written.
    total: u64,
}
impl History {
    fn push(&mut self, sample: i16) {
        self.samples[(self.total as usize) & (HISTORY - 1)] = sample;
        self.total += 1;
    }
    /// The sample with absolute index `index`, which must still be held.
    fn at(&self, index: u64) -> i64 {
        debug_assert!(index < self.total && self.total - index <= HISTORY as u64);
        i64::from(self.samples[(index as usize) & (HISTORY - 1)])
    }
}

/// The last 120 ms of one capture stream, oldest first.
struct Window {
    samples: [i16; WINDOW],
}
impl Window {
    fn push(&mut self, block: [i16; BLOCK]) {
        self.samples.copy_within(BLOCK.., 0);
        self.samples[WINDOW - BLOCK..].copy_from_slice(&block);
    }
    fn sum(&self) -> i64 {
        self.samples.iter().map(|&sample| i64::from(sample)).sum()
    }
    /// Energy about the mean, so a microphone offset is not counted as signal.
    fn energy(&self) -> f64 {
        let squares: i64 = self
            .samples
            .iter()
            .map(|&sample| i64::from(sample) * i64::from(sample))
            .sum();
        centred(squares, self.sum(), self.sum())
    }
}

/// `sum(a*b) - sum(a)*sum(b)/n` over one window: a covariance times `n`.
fn centred(products: i64, a: i64, b: i64) -> f64 {
    products as f64 - a as f64 * b as f64 / WINDOW as f64
}

struct Alignment {
    /// Render index minus capture index of matching samples, as a difference
    /// of absolute sample counts. Constant while both streams are continuous
    /// and their clocks agree; it moves by one when they drift.
    offset: i64,
    /// Capture blocks since it last explained [`MATCH_PERCENT`].
    age_blocks: u16,
}

/// Reduces one stream to the analysis rate. See [`ANALYSIS_RATE`].
#[derive(Default)]
struct Reducer {
    previous: i32,
}
impl Reducer {
    fn push(&mut self, group: &[i16]) -> i16 {
        let mean = group.iter().map(|&sample| i32::from(sample)).sum::<i32>() / group.len() as i32;
        let reduced = (mean + self.previous) / 2;
        self.previous = mean;
        reduced as i16
    }
}

pub struct EchoObserver {
    render: History,
    raw: Window,
    clean: Window,
    reducers: [Reducer; 3],
    /// Analysis samples of capture ever observed.
    captured: u64,
    blocks: u32,
    /// One bit per recent render block, newest in bit zero.
    activity: u64,
    alignment: Option<Alignment>,
    /// How far the search has got: steps behind the newest render while
    /// nothing is matched, steps outward from the alignment once one is.
    sweep: i64,
    latest: Option<EchoEvidence>,
}
impl Default for EchoObserver {
    fn default() -> Self {
        Self::new()
    }
}

/// The render under one alignment of the capture window.
struct Compared {
    /// Render energy about its mean.
    render: f64,
    /// Percent of the capture's energy the render explains.
    explained: u8,
}

const SILENT_ENERGY: f64 = (SILENT_RMS * SILENT_RMS * WINDOW as u64) as f64;

impl EchoObserver {
    pub fn new() -> Self {
        Self {
            render: History {
                samples: [0; HISTORY],
                total: 0,
            },
            raw: Window {
                samples: [0; WINDOW],
            },
            clean: Window {
                samples: [0; WINDOW],
            },
            reducers: Default::default(),
            captured: 0,
            blocks: 0,
            activity: 0,
            alignment: None,
            sweep: 0,
            latest: None,
        }
    }
    /// Forget everything. Call wherever the echo canceller is reset.
    pub fn reset(&mut self) {
        *self = Self::new();
    }
    /// The evidence of the most recent capture block, if any since reset.
    pub fn latest(&self) -> Option<EchoEvidence> {
        self.latest
    }

    /// One block of PCM the speaker device accepted, in acceptance order.
    pub fn render(&mut self, samples: &RenderBlock) {
        let mut energy = 0u64;
        for group in samples.chunks_exact(RENDER_SAMPLES / BLOCK) {
            let reduced = self.reducers[0].push(group);
            energy += (i32::from(reduced) * i32::from(reduced)) as u64;
            self.render.push(reduced);
        }
        let active = energy >= SILENT_RMS * SILENT_RMS * BLOCK as u64;
        self.activity = (self.activity << 1) | u64::from(active);
    }

    /// One microphone block before and after cancellation.
    pub fn capture(&mut self, raw: &CaptureBlock, clean: &CaptureBlock) -> EchoEvidence {
        let reduce = |reducer: &mut Reducer, block: &CaptureBlock| -> [i16; BLOCK] {
            let mut reduced = [0; BLOCK];
            for (target, group) in reduced
                .iter_mut()
                .zip(block.chunks_exact(CAPTURE_SAMPLES / BLOCK))
            {
                *target = reducer.push(group);
            }
            reduced
        };
        self.raw.push(reduce(&mut self.reducers[1], raw));
        self.clean.push(reduce(&mut self.reducers[2], clean));
        self.captured += BLOCK as u64;
        self.blocks = self.blocks.saturating_add(1);
        let (raw_energy, clean_energy) = (self.raw.energy(), self.clean.energy());
        let echo = self.matched(raw_energy);
        let evidence = EchoEvidence {
            blocks: self.blocks,
            render_active_blocks: (self.activity & ((1 << ACTIVITY_BLOCKS) - 1)).count_ones() as u8,
            capture_level: Level::of(raw_energy),
            residual_level: Level::of(clean_energy),
            echo,
        };
        self.latest = Some(evidence);
        evidence
    }

    /// Offsets at which the whole capture window has render to compare with:
    /// from the longest delay searched up to the newest render (no delay).
    fn offsets(&self) -> Option<(i64, i64)> {
        if self.captured < WINDOW as u64 {
            return None;
        }
        let newest = self.render.total as i64 - self.captured as i64;
        let first = self.captured as i64 - WINDOW as i64;
        let oldest_held = (self.render.total as i64 - HISTORY as i64).max(0);
        let low = (newest - MAX_DELAY).max(oldest_held - first);
        (low <= newest).then_some((low, newest))
    }

    /// Compare one capture window with the render at `offset`, which must be
    /// inside [`Self::offsets`].
    fn compare(&self, capture: &Window, energy: f64, offset: i64) -> Compared {
        let first = (self.captured as i64 - WINDOW as i64 + offset) as u64;
        let (mut cross, mut squares, mut sum) = (0i64, 0i64, 0i64);
        for (index, &sample) in capture.samples.iter().enumerate() {
            let reference = self.render.at(first + index as u64);
            cross += i64::from(sample) * reference;
            squares += reference * reference;
            sum += reference;
        }
        let render = centred(squares, sum, sum);
        let cross = centred(cross, capture.sum(), sum);
        let explained = if energy <= 0.0 || render <= 0.0 {
            0
        } else {
            (cross * cross / (energy * render) * 100.0)
                .round()
                .clamp(0.0, 100.0) as u8
        };
        Compared { render, explained }
    }

    /// As [`Self::compare`], with the render allowed one analysis sample
    /// either side of `offset` as well. A true delay is rarely a whole number
    /// of samples, and a single alignment would then understate the share.
    /// The three are fitted together by least squares; the share reported is
    /// the capture energy that fit accounts for.
    fn fit(&self, capture: &Window, energy: f64, offset: i64, low: i64, newest: i64) -> Compared {
        if offset - 1 < low || offset + 1 > newest {
            return self.compare(capture, energy, offset);
        }
        let first = (self.captured as i64 - WINDOW as i64 + offset - 1) as u64;
        let (mut cross, mut sums, mut products) = ([0i64; 3], [0i64; 3], [[0i64; 3]; 3]);
        for (index, &sample) in capture.samples.iter().enumerate() {
            let taps: [i64; 3] =
                std::array::from_fn(|tap| self.render.at(first + (index + tap) as u64));
            for row in 0..3 {
                cross[row] += i64::from(sample) * taps[row];
                sums[row] += taps[row];
                for column in row..3 {
                    products[row][column] += taps[row] * taps[column];
                }
            }
        }
        let capture_sum = capture.sum();
        let wanted: [f64; 3] =
            std::array::from_fn(|row| centred(cross[row], capture_sum, sums[row]));
        let mut render = [[0f64; 3]; 3];
        for row in 0..3 {
            for column in row..3 {
                render[row][column] = centred(products[row][column], sums[row], sums[column]);
                render[column][row] = render[row][column];
            }
        }
        let centre = render[1][1];
        // A touch of ridge keeps a nearly periodic render solvable.
        let ridge = (render[0][0] + render[1][1] + render[2][2]) * 1e-6 + 1.0;
        let Some(accounted) = accounted_for(render, wanted, ridge) else {
            return self.compare(capture, energy, offset);
        };
        let explained = if energy <= 0.0 {
            0
        } else {
            (accounted / energy * 100.0).round().clamp(0.0, 100.0) as u8
        };
        Compared {
            render: centre,
            explained,
        }
    }

    /// The best of `offsets` at which the render is not silent.
    fn best(&self, energy: f64, offsets: impl Iterator<Item = i64>) -> Option<(i64, u8)> {
        offsets
            .map(|offset| (offset, self.compare(&self.raw, energy, offset)))
            .filter(|(_, compared)| compared.render >= SILENT_ENERGY)
            .map(|(offset, compared)| (offset, compared.explained))
            .max_by_key(|&(_, explained)| explained)
    }

    /// The `step`th alignment of the search. With nothing matched it walks
    /// back from the newest render. Around a held alignment it alternates
    /// outward, so one that drifted while the speaker was silent is found
    /// again at once. `None` once the whole range has been covered.
    fn candidate(&self, low: i64, newest: i64, step: i64) -> Option<Option<i64>> {
        let offset = match &self.alignment {
            None => newest - step,
            // The alignment and its two neighbours are always tried.
            Some(alignment) if step % 2 == 0 => alignment.offset - 2 - step / 2,
            Some(alignment) => alignment.offset + 2 + step / 2,
        };
        let exhausted = match &self.alignment {
            None => offset < low,
            Some(_) => 2 + step / 2 > newest - low,
        };
        (!exhausted).then(|| (low..=newest).contains(&offset).then_some(offset))
    }

    fn matched(&mut self, raw_energy: f64) -> Option<EchoMatch> {
        let (low, newest) = self.offsets()?;
        if let Some(alignment) = self.alignment.as_mut() {
            alignment.age_blocks = alignment.age_blocks.saturating_add(1);
            if !(low..=newest).contains(&alignment.offset) {
                // The streams moved apart further than is searched.
                self.alignment = None;
                self.sweep = 0;
            }
        }
        // A silent capture matches nothing and must not unseat an alignment.
        if raw_energy >= SILENT_ENERGY {
            // Follow slow clock drift around the alignment already held.
            let held = self.alignment.as_ref().and_then(|alignment| {
                let near = alignment.offset - 1..=alignment.offset + 1;
                self.best(
                    raw_energy,
                    near.filter(|offset| (low..=newest).contains(offset)),
                )
            });
            // Keep looking elsewhere as well: the first match may be a weaker
            // reflection, and a restarted stream moves the true alignment.
            let mut tried = [None; SWEEP as usize];
            for (slot, step) in tried.iter_mut().zip(self.sweep..) {
                match self.candidate(low, newest, step) {
                    Some(offset) => *slot = offset,
                    None => break,
                }
            }
            self.sweep = match self.candidate(low, newest, self.sweep + SWEEP) {
                Some(_) => self.sweep + SWEEP,
                None => 0,
            };
            let found = self.best(raw_energy, tried.into_iter().flatten());
            let adopt = match (held, found) {
                (Some((_, held)), Some((offset, found)))
                    if found >= MATCH_PERCENT
                        && found >= held.saturating_add(REPLACE_MARGIN_PERCENT) =>
                {
                    Some(offset)
                }
                (Some((offset, held)), _) if held >= MATCH_PERCENT => Some(offset),
                (None, Some((offset, found))) if found >= MATCH_PERCENT => Some(offset),
                _ => None,
            };
            if let Some(offset) = adopt {
                if self
                    .alignment
                    .as_ref()
                    .is_none_or(|held| (held.offset - offset).abs() > 1)
                {
                    self.sweep = 0;
                }
                self.alignment = Some(Alignment {
                    offset,
                    age_blocks: 0,
                });
            }
        }
        let alignment = self.alignment.as_ref()?;
        let raw = self.fit(&self.raw, raw_energy, alignment.offset, low, newest);
        if raw.render < SILENT_ENERGY {
            return None;
        }
        let clean = self.fit(
            &self.clean,
            self.clean.energy(),
            alignment.offset,
            low,
            newest,
        );
        Some(EchoMatch {
            render_level: Level::of(raw.render),
            capture_explained: raw.explained,
            residual_explained: clean.explained,
            alignment_age_blocks: alignment.age_blocks,
        })
    }
}

/// `wanted' * inverse(render + ridge) * wanted` for a symmetric positive
/// matrix, by Cholesky factorization. `None` when it is not positive.
fn accounted_for(mut render: [[f64; 3]; 3], wanted: [f64; 3], ridge: f64) -> Option<f64> {
    for (index, row) in render.iter_mut().enumerate() {
        row[index] += ridge;
    }
    let mut lower = [[0f64; 3]; 3];
    for row in 0..3 {
        for column in 0..=row {
            let sum: f64 = (0..column).map(|k| lower[row][k] * lower[column][k]).sum();
            if row == column {
                let pivot = render[row][row] - sum;
                if pivot <= 0.0 {
                    return None;
                }
                lower[row][column] = pivot.sqrt();
            } else {
                lower[row][column] = (render[row][column] - sum) / lower[column][column];
            }
        }
    }
    // Forward substitution: |solve(lower, wanted)|^2 is the quadratic form.
    let mut solved = [0f64; 3];
    for row in 0..3 {
        let sum: f64 = (0..row).map(|k| lower[row][k] * solved[k]).sum();
        solved[row] = (wanted[row] - sum) / lower[row][row];
    }
    Some(solved.iter().map(|value| value * value).sum())
}
