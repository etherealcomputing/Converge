#![forbid(unsafe_code)]

//! Chaos engineering for spiking networks.
//!
//! Neuromorphic systems fail dynamically, not by corrupting memory. A network doesn't crash
//! when its excitation and inhibition drift apart; it saturates, or it goes quiet, and it
//! keeps answering the whole time. Memory-safety fuzzing looks for the wrong class of bug.
//!
//! So the question isn't whether a network infers. It's how far you can push it before it
//! stops. This crate sweeps a scalar fault operator across its range, checks invariants at
//! each point against an unperturbed baseline, and reports the severity at first violation.
//! That number is the failure envelope, and it's a margin you can engineer against rather
//! than a bit you can only pass or fail.
//!
//! The reason this lives in Converge is determinism. A reproducible fault sweep is difficult
//! against a stochastic Python SNN library and straightforward against a simulator whose
//! output is a function of its seed.
//!
//! This is defensive. It perturbs Converge's own elaborated networks, and it has no business
//! anywhere near somebody else's system.
//!
//! ```no_run
//! use converge_lang::parser::parse_program;
//! use converge_perturb::{SweepConfig, envelope_json, sweep};
//!
//! let program = parse_program("...").unwrap();
//! let net = converge_sim::elaborate(&program).unwrap();
//! let env = sweep(&net, &SweepConfig::default()).unwrap();
//! print!("{}", envelope_json(&env));
//! ```

pub mod envelope;
pub mod invariant;
pub mod ops;

pub use envelope::{
    ENVELOPE_VERSION, Envelope, InvariantResult, OperatorResult, Outcome, Sample, SweepConfig,
    envelope_json, sweep,
};
pub use invariant::Invariant;
pub use ops::{Draws, Operator};

#[cfg(test)]
mod tests {
    use super::*;
    use converge_lang::parser::parse_program;
    use converge_sim::{Network, elaborate, run};

    /// A network with sustained activity in both layers, so the invariants have something to
    /// measure. Recurrence keeps the output layer going once it's driven.
    const FIXTURE: &str = r#"
neuron LIF { tau_m = 20 ms, v_th = 1.0 }
layer Sense[8] : LIF
layer Relay[4] : LIF
connect Sense -> Relay { w = Uniform(0.2, 0.6), d = 1 ms }
connect Relay -> Relay { w = -0.1, d = 2 ms }
stimulus Sense = Poisson(rate=400 Hz)
run for 200 ms step 1 ms
seed 42
"#;

    fn fixture() -> Network {
        elaborate(&parse_program(FIXTURE).expect("parse")).expect("elaborate")
    }

    fn config() -> SweepConfig {
        SweepConfig {
            grid_steps: 8,
            bisect_steps: 3,
            operators: ops::ALL.to_vec(),
        }
    }

    // Severity 0 is the identity for every operator. If it isn't, the whole sweep is measured
    // against the wrong baseline.
    #[test]
    fn severity_zero_is_the_identity() {
        let net = fixture();
        let draws = Draws::new(&net, net.seed);
        let baseline = run(&net).expect("run");

        for &op in ops::ALL {
            let mut perturbed = net.clone();
            ops::apply(op, &mut perturbed, 0.0, &draws);
            let after = run(&perturbed).expect("run");
            assert_eq!(
                after.total_spikes,
                baseline.total_spikes,
                "{} moved the network at severity 0",
                op.name()
            );
        }
    }

    // Common random numbers make ablation nested: whatever is cut at a lower severity stays
    // cut at a higher one. Without this the sweep measures resampling noise.
    #[test]
    fn prune_sets_are_nested() {
        let net = fixture();
        let draws = Draws::new(&net, net.seed);

        let surviving = |m: f64| {
            let mut p = net.clone();
            ops::apply(Operator::Prune, &mut p, m, &draws);
            let mut ids = Vec::new();
            for (ci, conn) in p.connections.iter().enumerate() {
                for (si, syn_list) in conn.synapses.iter().enumerate() {
                    for syn in syn_list {
                        ids.push((ci, si, syn.dst));
                    }
                }
            }
            ids
        };

        let mut previous = surviving(0.0);
        for step in 1..=8 {
            let current = surviving(step as f64 / 8.0);
            assert!(current.len() <= previous.len());
            for id in &current {
                assert!(previous.contains(id), "prune un-cut a synapse at {step}/8");
            }
            previous = current;
        }
    }

    #[test]
    fn prune_at_full_severity_removes_everything() {
        let net = fixture();
        let draws = Draws::new(&net, net.seed);
        let mut p = net.clone();
        ops::apply(Operator::Prune, &mut p, 1.0, &draws);
        assert_eq!(ops::synapse_count(&p), 0);
        assert!(ops::synapse_count(&net) > 0);
    }

    // Jitter has to stay on the step grid and respect the same one step delivery floor the
    // simulator applies, or run would deliver into a bucket it already drained.
    #[test]
    fn jitter_keeps_delays_deliverable() {
        let net = fixture();
        let draws = Draws::new(&net, net.seed);
        for step in 0..=8 {
            let mut p = net.clone();
            ops::apply(Operator::SpikeJitter, &mut p, step as f64 / 8.0, &draws);
            for conn in &p.connections {
                for syn_list in &conn.synapses {
                    for syn in syn_list {
                        assert!(syn.delay_steps >= 1);
                        assert!(syn.delay_steps <= p.steps);
                    }
                }
            }
            run(&p).expect("a jittered network still runs");
        }
    }

    #[test]
    fn disinhibit_only_touches_inhibitory_weights() {
        let net = fixture();
        let draws = Draws::new(&net, net.seed);
        let mut p = net.clone();
        ops::apply(Operator::EiScaleDisinhibit, &mut p, 1.0, &draws);

        for (conn, base) in p.connections.iter().zip(&net.connections) {
            for (syn_list, base_list) in conn.synapses.iter().zip(&base.synapses) {
                for (syn, base_syn) in syn_list.iter().zip(base_list) {
                    if base_syn.weight > 0.0 {
                        assert_eq!(syn.weight, base_syn.weight);
                    } else {
                        assert_eq!(syn.weight, 0.0);
                    }
                }
            }
        }
    }

    #[test]
    fn gain_moves_stimulus_amplitude_both_ways() {
        let net = fixture();
        let draws = Draws::new(&net, net.seed);

        let mut down = net.clone();
        ops::apply(Operator::GainAttenuate, &mut down, 1.0, &draws);
        assert_eq!(down.stimuli[0].amplitude, 0.0);

        let mut up = net.clone();
        ops::apply(Operator::GainAmplify, &mut up, 1.0, &draws);
        assert_eq!(up.stimuli[0].amplitude, ops::AMPLIFY_MAX);
    }

    // The whole subsystem rests on this. An envelope that isn't reproducible isn't evidence.
    #[test]
    fn the_envelope_is_reproducible() {
        let a = envelope_json(&sweep(&fixture(), &config()).expect("sweep"));
        let b = envelope_json(&sweep(&fixture(), &config()).expect("sweep"));
        assert_eq!(a, b);
    }

    // Removing every synapse and removing every drive both have to register somewhere. An
    // operator whose envelope never moves is measuring nothing.
    #[test]
    fn severe_faults_violate_something() {
        let env = sweep(&fixture(), &config()).expect("sweep");

        for op in [Operator::Prune, Operator::GainAttenuate] {
            let result = env
                .operators
                .iter()
                .find(|r| r.operator == op)
                .expect("operator in envelope");
            let violated = result
                .invariants
                .iter()
                .any(|i| matches!(i.outcome, Outcome::Violated { .. }));
            assert!(violated, "{} never violated anything", op.name());
        }
    }

    // A bound is only meaningful if it sits inside the swept range.
    #[test]
    fn reported_bounds_are_in_range() {
        let env = sweep(&fixture(), &config()).expect("sweep");
        for result in &env.operators {
            for ir in &result.invariants {
                if let Outcome::Violated { bound, .. } = ir.outcome {
                    assert!(
                        (0.0..=1.0).contains(&bound),
                        "{} / {} reported {bound}",
                        result.operator.name(),
                        ir.invariant.name()
                    );
                }
            }
        }
    }

    // A baseline that can't exercise an invariant has to say so. A vacuous pass and a real one
    // read identically in a report, and only one of them is evidence.
    #[test]
    fn an_inapplicable_invariant_is_not_a_pass() {
        let quiet = elaborate(
            &parse_program(
                r#"
neuron LIF { tau_m = 20 ms, v_th = 1.0 }
layer A[2] : LIF
layer B[2] : LIF
connect A -> B { w = 0.1, d = 1 ms }
run for 10 ms step 1 ms
seed 1
"#,
            )
            .expect("parse"),
        )
        .expect("elaborate");

        let env = sweep(&quiet, &config()).expect("sweep");
        let alive = env.operators[0]
            .invariants
            .iter()
            .find(|i| i.invariant == Invariant::Alive)
            .expect("alive invariant");
        assert!(matches!(alive.outcome, Outcome::NotApplicable));
    }

    #[test]
    fn sweeping_does_not_mutate_the_baseline() {
        let net = fixture();
        let before = ops::synapse_count(&net);
        let _ = sweep(&net, &config()).expect("sweep");
        assert_eq!(ops::synapse_count(&net), before);
    }

    #[test]
    fn operator_names_round_trip() {
        for &op in ops::ALL {
            assert_eq!(Operator::from_name(op.name()), Some(op));
        }
        assert_eq!(Operator::from_name("nope"), None);
    }
}
