//! Invariants checked against a baseline run.
//!
//! Each one reports a measured scalar alongside its verdict. A boolean tells you the run
//! broke; the scalar tells you how close the last good one was.
//!
//! An invariant that can't apply to a given baseline says so rather than passing. A silent
//! pass and a vacuous pass look identical in a report, and only one of them is evidence.

use converge_sim::SimSummary;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invariant {
    /// No membrane value left the reals.
    Finite,
    /// Every layer that spiked at baseline still spikes.
    Alive,
    /// No layer's peak window rate ran away past a multiple of its baseline peak.
    Bounded,
    /// Total spike count stayed inside a band around the baseline total.
    RateBand,
    /// A layer that was active in every baseline window didn't drop a window entirely.
    Sustained,
}

/// Declared order. Output follows this.
pub const ALL: &[Invariant] = &[
    Invariant::Finite,
    Invariant::Alive,
    Invariant::Bounded,
    Invariant::RateBand,
    Invariant::Sustained,
];

/// Peak window rate is allowed to reach this multiple of baseline before `Bounded` breaks.
pub const RUNAWAY_FACTOR: f64 = 4.0;
/// Total spike count band for `RateBand`, as multiples of the baseline total.
pub const RATE_BAND: (f64, f64) = (0.25, 4.0);

impl Invariant {
    /// Stable identifier. This lands in the envelope output, so it doesn't change.
    pub fn name(self) -> &'static str {
        match self {
            Invariant::Finite => "finite",
            Invariant::Alive => "alive",
            Invariant::Bounded => "bounded",
            Invariant::RateBand => "rate_band",
            Invariant::Sustained => "sustained",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Invariant::Finite => "no non-finite membrane value",
            Invariant::Alive => "every layer active at baseline still spikes",
            Invariant::Bounded => "no layer peak window rate above 4x its baseline peak",
            Invariant::RateBand => "total spikes within 0.25x to 4x baseline",
            Invariant::Sustained => "no fully silent window in a layer active throughout baseline",
        }
    }

    /// Whether this invariant says anything about the given baseline. A baseline that never
    /// spikes can't lose activity, and a layer that was already intermittent can't be shown to
    /// have become intermittent.
    pub fn applicable(self, baseline: &SimSummary) -> bool {
        match self {
            Invariant::Finite => true,
            Invariant::Alive => baseline.layers.iter().any(|l| l.spikes > 0),
            Invariant::Bounded => baseline.total_spikes > 0,
            Invariant::RateBand => baseline.total_spikes > 0,
            Invariant::Sustained => baseline
                .layers
                .iter()
                .any(|l| !l.windows.is_empty() && l.windows.iter().all(|w| *w > 0)),
        }
    }

    /// Evaluate against a baseline. The measured value is the quantity the predicate is about,
    /// so a reader can see the margin and not just the verdict.
    pub fn check(self, baseline: &SimSummary, run: &SimSummary) -> Check {
        match self {
            Invariant::Finite => {
                let measured = run.nonfinite as f64;
                Check::new(measured, run.nonfinite == 0)
            }
            Invariant::Alive => {
                let silenced = baseline
                    .layers
                    .iter()
                    .zip(&run.layers)
                    .filter(|(b, r)| b.spikes > 0 && r.spikes == 0)
                    .count();
                Check::new(silenced as f64, silenced == 0)
            }
            Invariant::Bounded => {
                let mut worst: f64 = 0.0;
                for (b, r) in baseline.layers.iter().zip(&run.layers) {
                    let base_peak = b.windows.iter().copied().max().unwrap_or(0).max(1) as f64;
                    let peak = r.windows.iter().copied().max().unwrap_or(0) as f64;
                    worst = worst.max(peak / base_peak);
                }
                Check::new(worst, worst <= RUNAWAY_FACTOR)
            }
            Invariant::RateBand => {
                let base = baseline.total_spikes.max(1) as f64;
                let ratio = run.total_spikes as f64 / base;
                Check::new(ratio, ratio >= RATE_BAND.0 && ratio <= RATE_BAND.1)
            }
            Invariant::Sustained => {
                let mut dropped = 0usize;
                for (b, r) in baseline.layers.iter().zip(&run.layers) {
                    if b.windows.is_empty() || b.windows.contains(&0) {
                        continue;
                    }
                    dropped += r.windows.iter().filter(|w| **w == 0).count();
                }
                Check::new(dropped as f64, dropped == 0)
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Check {
    pub measured: f64,
    pub holds: bool,
}

impl Check {
    fn new(measured: f64, holds: bool) -> Check {
        Check { measured, holds }
    }
}
