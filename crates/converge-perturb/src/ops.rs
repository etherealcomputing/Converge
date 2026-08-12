//! Fault operators.
//!
//! Each operator is a scalar knob on the dynamics of an elaborated network. Severity runs
//! `0.0` to `1.0`, and `0.0` is always the identity. A uniform axis across all of them is what
//! makes one operator's envelope comparable to another's.
//!
//! Operators are named for the mechanism they move. If a perturbation can't be written as a
//! scalar on an equation, it doesn't belong here.

use converge_sim::{Network, Rng, stream};

/// The operators, each in the direction it degrades.
///
/// Two mechanisms are two sided, so they get one entry per direction rather than a signed
/// axis. That keeps every severity on the same `[0, 1]` scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    /// Scale inhibitory weights down. Drives runaway excitation.
    EiScaleDisinhibit,
    /// Scale excitatory weights down. Drives the network silent.
    EiScaleDeexcite,
    /// Offset per-synapse delays. Drives desynchronization and timing collapse.
    SpikeJitter,
    /// Scale stimulus amplitude down. Drives the input being ignored.
    GainAttenuate,
    /// Scale stimulus amplitude up. Drives output that confirms itself regardless of input.
    GainAmplify,
    /// Ablate a fraction of synapses. Separates graceful degradation from a cliff edge.
    Prune,
}

/// Declared order. Output follows this, so it never depends on map iteration.
pub const ALL: &[Operator] = &[
    Operator::EiScaleDisinhibit,
    Operator::EiScaleDeexcite,
    Operator::SpikeJitter,
    Operator::GainAttenuate,
    Operator::GainAmplify,
    Operator::Prune,
];

/// Stimulus amplitude multiplier at severity 1.0 for [`Operator::GainAmplify`]. A pole at
/// severity 1.0 would put an infinity in the envelope, so the ramp is linear and bounded.
pub const AMPLIFY_MAX: f64 = 10.0;

impl Operator {
    /// Stable identifier. This is what lands in the envelope output, so it doesn't change.
    pub fn name(self) -> &'static str {
        match self {
            Operator::EiScaleDisinhibit => "ei_scale.disinhibit",
            Operator::EiScaleDeexcite => "ei_scale.deexcite",
            Operator::SpikeJitter => "spike_jitter",
            Operator::GainAttenuate => "gain.attenuate",
            Operator::GainAmplify => "gain.amplify",
            Operator::Prune => "prune",
        }
    }

    pub fn from_name(name: &str) -> Option<Operator> {
        ALL.iter().copied().find(|op| op.name() == name)
    }

    /// What the knob does, in one line.
    pub fn description(self) -> &'static str {
        match self {
            Operator::EiScaleDisinhibit => "inhibitory weights scaled by (1 - m)",
            Operator::EiScaleDeexcite => "excitatory weights scaled by (1 - m)",
            Operator::SpikeJitter => "delays offset by a normal with sigma = m * mean delay",
            Operator::GainAttenuate => "stimulus amplitude scaled by (1 - m)",
            Operator::GainAmplify => "stimulus amplitude scaled from 1x to 10x",
            Operator::Prune => "fraction m of synapses removed",
        }
    }
}

/// Per-synapse values drawn once and reused at every severity.
///
/// This is the difference between an envelope that measures the network and one that measures
/// sampling noise. Redrawing at each grid point would make a sweep wander for reasons that
/// have nothing to do with fragility. Holding the draws fixed makes prune sets nested (a
/// synapse cut at `m` stays cut at any larger `m`) and makes jitter one fixed pattern scaled
/// by severity, so severity is the only thing that moves.
pub struct Draws {
    /// Uniform `[0, 1)` per synapse, indexed `[connection][source][slot]`.
    prune_keys: Vec<Vec<Vec<f64>>>,
    /// Standard normal per synapse, same indexing.
    jitter_offsets: Vec<Vec<Vec<f64>>>,
}

impl Draws {
    /// Draw against the baseline structure. Operators always apply to a fresh clone of that
    /// same baseline, so the indexing lines up.
    pub fn new(net: &Network, seed: u64) -> Draws {
        let mut prune_rng = Rng::stream(seed, stream::PERTURB_PRUNE);
        let mut jitter_rng = Rng::stream(seed, stream::PERTURB_JITTER);

        let mut prune_keys = Vec::with_capacity(net.connections.len());
        let mut jitter_offsets = Vec::with_capacity(net.connections.len());

        for conn in &net.connections {
            let mut conn_prune = Vec::with_capacity(conn.synapses.len());
            let mut conn_jitter = Vec::with_capacity(conn.synapses.len());
            for syn_list in &conn.synapses {
                let mut p = Vec::with_capacity(syn_list.len());
                let mut j = Vec::with_capacity(syn_list.len());
                for _ in syn_list {
                    p.push(prune_rng.next_f64());
                    j.push(standard_normal(&mut jitter_rng));
                }
                conn_prune.push(p);
                conn_jitter.push(j);
            }
            prune_keys.push(conn_prune);
            jitter_offsets.push(conn_jitter);
        }

        Draws {
            prune_keys,
            jitter_offsets,
        }
    }
}

fn standard_normal(rng: &mut Rng) -> f64 {
    let (u1, u2) = (rng.next_f64_open(), rng.next_f64());
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// Apply `op` at `severity` to `net` in place.
///
/// `severity` is clamped to `[0, 1]`. At `0.0` every operator leaves the network untouched.
pub fn apply(op: Operator, net: &mut Network, severity: f64, draws: &Draws) {
    let m = severity.clamp(0.0, 1.0);
    if m == 0.0 {
        return;
    }

    match op {
        Operator::EiScaleDisinhibit => scale_weights(net, |w| w < 0.0, 1.0 - m),
        Operator::EiScaleDeexcite => scale_weights(net, |w| w > 0.0, 1.0 - m),
        Operator::GainAttenuate => {
            for s in &mut net.stimuli {
                s.amplitude *= 1.0 - m;
            }
        }
        Operator::GainAmplify => {
            let scale = 1.0 + m * (AMPLIFY_MAX - 1.0);
            for s in &mut net.stimuli {
                s.amplitude *= scale;
            }
        }
        Operator::SpikeJitter => jitter(net, m, draws),
        Operator::Prune => prune(net, m, draws),
    }
}

fn scale_weights(net: &mut Network, select: impl Fn(f64) -> bool, factor: f64) {
    for conn in &mut net.connections {
        for syn_list in &mut conn.synapses {
            for syn in syn_list.iter_mut() {
                if select(syn.weight) {
                    syn.weight *= factor;
                }
            }
        }
    }
}

fn jitter(net: &mut Network, m: f64, draws: &Draws) {
    let sigma = m * mean_delay_steps(net);
    if sigma <= 0.0 {
        return;
    }
    // A delay longer than the run never arrives, so there's no reason to size a ring past it.
    let ceiling = net.steps.max(1);

    for (ci, conn) in net.connections.iter_mut().enumerate() {
        for (si, syn_list) in conn.synapses.iter_mut().enumerate() {
            for (k, syn) in syn_list.iter_mut().enumerate() {
                let offset = draws.jitter_offsets[ci][si][k] * sigma;
                let shifted = (syn.delay_steps as f64 + offset).round();
                // Same floor the simulator applies: nothing arrives in the step it was sent.
                syn.delay_steps = (shifted.max(1.0) as usize).min(ceiling);
            }
        }
    }
}

fn mean_delay_steps(net: &Network) -> f64 {
    let mut total = 0u64;
    let mut count = 0u64;
    for conn in &net.connections {
        for syn_list in &conn.synapses {
            for syn in syn_list {
                total += syn.delay_steps as u64;
                count += 1;
            }
        }
    }
    if count == 0 {
        0.0
    } else {
        total as f64 / count as f64
    }
}

fn prune(net: &mut Network, m: f64, draws: &Draws) {
    for (ci, conn) in net.connections.iter_mut().enumerate() {
        for (si, syn_list) in conn.synapses.iter_mut().enumerate() {
            let keys = &draws.prune_keys[ci][si];
            let mut k = 0;
            // retain visits in order, so k tracks the original slot.
            syn_list.retain(|_| {
                let keep = keys[k] >= m;
                k += 1;
                keep
            });
        }
    }
}

/// Total synapses in a network. Used by tests and by the envelope report.
pub fn synapse_count(net: &Network) -> usize {
    net.connections
        .iter()
        .flat_map(|c| c.synapses.iter())
        .map(|s| s.len())
        .sum()
}
