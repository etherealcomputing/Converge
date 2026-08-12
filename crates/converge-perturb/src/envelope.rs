//! The sweep, and the failure envelope it produces.
//!
//! The output is a margin, not a verdict. "Held to 0.42 and then `alive` broke" is something
//! you can engineer against; "failed" is not.

use converge_sim::{Network, SimError, SimSummary, run};

use crate::invariant::{self, Invariant};
use crate::ops::{self, Draws, Operator};

/// Format version for the envelope document.
pub const ENVELOPE_VERSION: &str = "0.1";

#[derive(Debug, Clone)]
pub struct SweepConfig {
    /// Grid points across `[0, 1]`, not counting severity 0.
    pub grid_steps: usize,
    /// Bisection rounds between the last holding point and the first breaking one.
    pub bisect_steps: usize,
    /// Operators to sweep, in the order given.
    pub operators: Vec<Operator>,
}

impl Default for SweepConfig {
    fn default() -> Self {
        SweepConfig {
            grid_steps: 20,
            bisect_steps: 6,
            operators: ops::ALL.to_vec(),
        }
    }
}

/// What happened to one invariant under one operator.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// The baseline can't exercise this invariant. Not a pass.
    NotApplicable,
    /// Held at every severity swept.
    Held,
    /// Broke. `bound` is the severity at first violation, tightened by bisection.
    Violated { bound: f64, measured: f64 },
}

#[derive(Debug, Clone)]
pub struct InvariantResult {
    pub invariant: Invariant,
    pub outcome: Outcome,
}

#[derive(Debug, Clone)]
pub struct Sample {
    pub severity: f64,
    pub total_spikes: u64,
    pub nonfinite: u64,
    pub synapses: usize,
}

#[derive(Debug, Clone)]
pub struct OperatorResult {
    pub operator: Operator,
    pub invariants: Vec<InvariantResult>,
    pub samples: Vec<Sample>,
}

#[derive(Debug, Clone)]
pub struct Envelope {
    pub seed: u64,
    pub grid_steps: usize,
    pub bisect_steps: usize,
    pub baseline: SimSummary,
    pub baseline_synapses: usize,
    pub operators: Vec<OperatorResult>,
}

/// Sweep every configured operator against `net`.
///
/// `net` is the baseline and is never mutated; each severity runs on a fresh clone.
pub fn sweep(net: &Network, config: &SweepConfig) -> Result<Envelope, SimError> {
    let baseline = run(net)?;
    let draws = Draws::new(net, net.seed);
    let grid_steps = config.grid_steps.max(1);

    let mut operators = Vec::with_capacity(config.operators.len());
    for &op in &config.operators {
        operators.push(sweep_one(
            net,
            &baseline,
            &draws,
            op,
            grid_steps,
            config.bisect_steps,
        )?);
    }

    Ok(Envelope {
        seed: net.seed,
        grid_steps,
        bisect_steps: config.bisect_steps,
        baseline_synapses: ops::synapse_count(net),
        baseline,
        operators,
    })
}

/// Everything one operator's sweep holds fixed.
struct Ctx<'a> {
    net: &'a Network,
    baseline: &'a SimSummary,
    draws: &'a Draws,
    op: Operator,
}

impl Ctx<'_> {
    /// Run the baseline perturbed to `m`. The baseline itself is never mutated.
    fn at(&self, m: f64) -> Result<(SimSummary, usize), SimError> {
        let mut perturbed = self.net.clone();
        ops::apply(self.op, &mut perturbed, m, self.draws);
        let synapses = ops::synapse_count(&perturbed);
        Ok((run(&perturbed)?, synapses))
    }
}

fn sweep_one(
    net: &Network,
    baseline: &SimSummary,
    draws: &Draws,
    op: Operator,
    grid_steps: usize,
    bisect_steps: usize,
) -> Result<OperatorResult, SimError> {
    let ctx = Ctx {
        net,
        baseline,
        draws,
        op,
    };
    let mut samples = Vec::with_capacity(grid_steps + 1);
    // First severity at which each invariant broke, as a grid index.
    let mut first_break: Vec<Option<(usize, f64)>> = vec![None; invariant::ALL.len()];

    for i in 0..=grid_steps {
        let m = i as f64 / grid_steps as f64;
        let (summary, synapses) = ctx.at(m)?;

        for (slot, &inv) in invariant::ALL.iter().enumerate() {
            if !inv.applicable(baseline) || first_break[slot].is_some() {
                continue;
            }
            let check = inv.check(baseline, &summary);
            if !check.holds {
                first_break[slot] = Some((i, check.measured));
            }
        }

        samples.push(Sample {
            severity: m,
            total_spikes: summary.total_spikes,
            nonfinite: summary.nonfinite,
            synapses,
        });
    }

    let mut invariants = Vec::with_capacity(invariant::ALL.len());
    for (slot, &inv) in invariant::ALL.iter().enumerate() {
        let outcome = if !inv.applicable(baseline) {
            Outcome::NotApplicable
        } else {
            match first_break[slot] {
                None => Outcome::Held,
                Some((idx, measured)) => {
                    let hi = idx as f64 / grid_steps as f64;
                    let lo = idx.saturating_sub(1) as f64 / grid_steps as f64;
                    let bound = if idx == 0 {
                        // Broke at severity 0, so the baseline itself doesn't satisfy it.
                        0.0
                    } else {
                        bisect(&ctx, inv, lo, hi, bisect_steps)?
                    };
                    Outcome::Violated { bound, measured }
                }
            }
        };
        invariants.push(InvariantResult {
            invariant: inv,
            outcome,
        });
    }

    Ok(OperatorResult {
        operator: op,
        invariants,
        samples,
    })
}

/// Narrow the bound between a severity that held and one that broke.
///
/// This only tightens a bracket the grid already found. A cliff that sits entirely inside one
/// grid interval, with both ends holding, stays invisible, which is why the grid resolution is
/// reported alongside the bound.
fn bisect(
    ctx: &Ctx<'_>,
    inv: Invariant,
    mut lo: f64,
    mut hi: f64,
    rounds: usize,
) -> Result<f64, SimError> {
    for _ in 0..rounds {
        let mid = 0.5 * (lo + hi);
        let (summary, _) = ctx.at(mid)?;
        if inv.check(ctx.baseline, &summary).holds {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Ok(hi)
}

/// Render an envelope as JSON. Hand written, like the rest of the toolchain's output.
///
/// Every array is walked in declared order and nothing iterates a map, so the same input
/// produces the same bytes.
pub fn envelope_json(env: &Envelope) -> String {
    let mut s = String::new();
    s.push_str("{\n");
    s.push_str(&format!(
        "  \"envelope_version\": \"{ENVELOPE_VERSION}\",\n"
    ));
    s.push_str(&format!("  \"seed\": {},\n", env.seed));
    s.push_str(&format!("  \"grid_steps\": {},\n", env.grid_steps));
    s.push_str(&format!("  \"bisect_steps\": {},\n", env.bisect_steps));
    s.push_str("  \"baseline\": {\n");
    s.push_str(&format!(
        "    \"total_spikes\": {},\n",
        env.baseline.total_spikes
    ));
    s.push_str(&format!("    \"nonfinite\": {},\n", env.baseline.nonfinite));
    s.push_str(&format!("    \"synapses\": {},\n", env.baseline_synapses));
    s.push_str("    \"layers\": [\n");
    for (idx, layer) in env.baseline.layers.iter().enumerate() {
        s.push_str(&format!(
            "      {{ \"name\": \"{}\", \"spikes\": {} }}",
            layer.name, layer.spikes
        ));
        if idx + 1 != env.baseline.layers.len() {
            s.push(',');
        }
        s.push('\n');
    }
    s.push_str("    ]\n");
    s.push_str("  },\n");

    s.push_str("  \"operators\": [\n");
    for (op_idx, result) in env.operators.iter().enumerate() {
        s.push_str("    {\n");
        s.push_str(&format!(
            "      \"operator\": \"{}\",\n",
            result.operator.name()
        ));
        s.push_str(&format!(
            "      \"mechanism\": \"{}\",\n",
            result.operator.description()
        ));
        s.push_str("      \"invariants\": [\n");
        for (inv_idx, ir) in result.invariants.iter().enumerate() {
            s.push_str("        { ");
            s.push_str(&format!("\"invariant\": \"{}\", ", ir.invariant.name()));
            match &ir.outcome {
                Outcome::NotApplicable => {
                    s.push_str("\"status\": \"not_applicable\", \"bound\": null");
                }
                Outcome::Held => {
                    s.push_str("\"status\": \"held\", \"bound\": null");
                }
                Outcome::Violated { bound, measured } => {
                    s.push_str(&format!(
                        "\"status\": \"violated\", \"bound\": {}, \"measured\": {}",
                        fmt_f64(*bound),
                        fmt_f64(*measured)
                    ));
                }
            }
            s.push_str(" }");
            if inv_idx + 1 != result.invariants.len() {
                s.push(',');
            }
            s.push('\n');
        }
        s.push_str("      ],\n");
        s.push_str("      \"samples\": [\n");
        for (sample_idx, sample) in result.samples.iter().enumerate() {
            s.push_str(&format!(
                "        {{ \"severity\": {}, \"total_spikes\": {}, \"nonfinite\": {}, \"synapses\": {} }}",
                fmt_f64(sample.severity),
                sample.total_spikes,
                sample.nonfinite,
                sample.synapses
            ));
            if sample_idx + 1 != result.samples.len() {
                s.push(',');
            }
            s.push('\n');
        }
        s.push_str("      ]\n");
        s.push_str("    }");
        if op_idx + 1 != env.operators.len() {
            s.push(',');
        }
        s.push('\n');
    }
    s.push_str("  ]\n");
    s.push_str("}\n");
    s
}

/// Fixed precision so a severity of 3/20 reads as 0.15 rather than trailing binary noise, and
/// so the bytes don't depend on how a float happens to print.
fn fmt_f64(v: f64) -> String {
    if v.is_finite() {
        format!("{v:.6}")
    } else if v.is_nan() {
        "\"NaN\"".to_string()
    } else if v.is_sign_positive() {
        "\"Infinity\"".to_string()
    } else {
        "\"-Infinity\"".to_string()
    }
}
