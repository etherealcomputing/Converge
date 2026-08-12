#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;

use converge_lang::ast::{
    Assign, CallArg, ConnectDef, Expr, Item, NeuronDef, Program, StimulusDef, StimulusModel,
};
use converge_lang::units::{UnitKind, rate_to_hz, time_to_nanos, volts};

#[derive(Debug, Clone)]
pub struct SimSummary {
    pub duration_ns: i64,
    pub step_ns: i64,
    pub seed: u64,
    pub total_spikes: u64,
    pub layers: Vec<LayerSummary>,
    pub delay_quantization: DelayQuantization,
}

/// What the step grid did to the delays it was handed. A delay is a time, and a coercion you
/// can't see is a coercion you can't trust, so the simulator reports what it moved.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DelayQuantization {
    /// Synapses whose sampled delay wasn't already a whole number of steps.
    pub synapses_rounded: u64,
    /// Largest absolute rounding error applied, in nanoseconds.
    pub max_error_ns: i64,
    /// Synapses whose delay rounded to zero steps and were raised to the one step floor.
    pub synapses_floored: u64,
}

#[derive(Debug, Clone)]
pub struct LayerSummary {
    pub name: String,
    pub size: u64,
    pub spikes: u64,
}

#[derive(Debug)]
pub struct SimError {
    pub message: String,
}

impl fmt::Display for SimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SimError {}

pub fn simulate(program: &Program) -> Result<SimSummary, SimError> {
    let seed = program
        .items
        .iter()
        .find_map(|item| match item {
            Item::Seed(s) => Some(s.value),
            _ => None,
        })
        .unwrap_or(0);

    let run = program
        .items
        .iter()
        .find_map(|item| match item {
            Item::Run(run) => Some(run),
            _ => None,
        })
        .ok_or_else(|| SimError {
            message: "missing run statement".to_string(),
        })?;

    let duration_ns = time_to_nanos(&run.duration, "run duration").map_err(to_err)?;
    let step_ns = match &run.step {
        Some(step) => time_to_nanos(step, "run step").map_err(to_err)?,
        None => 1_000_000,
    };

    if duration_ns <= 0 || step_ns <= 0 {
        return Err(SimError {
            message: "duration and step must be positive".to_string(),
        });
    }
    if duration_ns % step_ns != 0 {
        return Err(SimError {
            message: "duration must be divisible by step".to_string(),
        });
    }

    let steps = (duration_ns / step_ns) as usize;

    let neuron_defs = collect_neuron_defs(program)?;
    let (mut layers, layer_index) = build_layers(program, &neuron_defs)?;

    // decay is step/tau_m. Past 1.0 the membrane flips sign every step and at exactly 2.0 it
    // negates. That isn't a leaky integrator, so refuse the run rather than clamp and pretend.
    for layer in &layers {
        if step_ns > layer.tau_m_ns {
            return Err(SimError {
                message: format!(
                    "run step ({} ns) exceeds tau_m ({} ns) for layer `{}`",
                    step_ns, layer.tau_m_ns, layer.name
                ),
            });
        }
    }

    let stimuli = collect_stimuli(program, &layer_index)?;
    let (connections, delay_quantization) =
        build_connections(program, &layer_index, &mut layers, step_ns, seed)?;

    let mut rng = Rng::stream(seed, stream::STIMULUS);
    let mut total_spikes = 0u64;

    let max_delay = connections
        .iter()
        .flat_map(|c| c.synapses.iter().flatten().map(|s| s.delay_steps))
        .max()
        .unwrap_or(0);
    let queue_len = max_delay + 1;

    let mut queues: Vec<Vec<Vec<f64>>> = layers
        .iter()
        .map(|layer| vec![vec![0.0; layer.size]; queue_len])
        .collect();

    for step in 0..steps {
        let bucket = step % queue_len;
        let mut spiked: Vec<Vec<usize>> = vec![Vec::new(); layers.len()];

        for (layer_idx, layer) in layers.iter_mut().enumerate() {
            // Leak, integrate, fire. The leak applies to the state carried in from the
            // previous step, not to charge arriving in this one. Leaking fresh input in the
            // same step it lands means a unit input can never reach a unit threshold.
            let decay = step_ns as f64 / layer.tau_m_ns as f64;
            for v in layer.v.iter_mut() {
                *v += (-*v) * decay;
            }

            let incoming = &mut queues[layer_idx][bucket];
            for (i, incoming_val) in incoming.iter_mut().enumerate() {
                layer.v[i] += *incoming_val;
                *incoming_val = 0.0;
            }

            if let Some(rate_hz) = stimuli.get(&layer_idx) {
                let p = rate_hz * (step_ns as f64 / 1_000_000_000.0);
                if p > 1.0 {
                    return Err(SimError {
                        message: "stimulus rate too high for step".to_string(),
                    });
                }
                for i in 0..layer.size {
                    if rng.next_f64() < p {
                        layer.v[i] += 1.0;
                    }
                }
            }

            for i in 0..layer.size {
                if layer.v[i] >= layer.v_th {
                    layer.v[i] = 0.0;
                    layer.spikes += 1;
                    total_spikes += 1;
                    spiked[layer_idx].push(i);
                }
            }
        }

        for conn in &connections {
            if spiked[conn.src_layer].is_empty() {
                continue;
            }
            for &src_i in &spiked[conn.src_layer] {
                for syn in &conn.synapses[src_i] {
                    let target_bucket = (bucket + syn.delay_steps) % queue_len;
                    queues[conn.dst_layer][target_bucket][syn.dst] += syn.weight;
                }
            }
        }
    }

    let layers_summary = layers
        .iter()
        .map(|l| LayerSummary {
            name: l.name.clone(),
            size: l.size as u64,
            spikes: l.spikes,
        })
        .collect();

    Ok(SimSummary {
        duration_ns,
        step_ns,
        seed,
        total_spikes,
        layers: layers_summary,
        delay_quantization,
    })
}

pub fn summary_json(summary: &SimSummary) -> String {
    let mut s = String::new();
    s.push_str("{\n");
    s.push_str(&format!("  \"duration_ns\": {},\n", summary.duration_ns));
    s.push_str(&format!("  \"step_ns\": {},\n", summary.step_ns));
    s.push_str(&format!("  \"seed\": {},\n", summary.seed));
    s.push_str(&format!("  \"total_spikes\": {},\n", summary.total_spikes));
    s.push_str("  \"layers\": [\n");
    for (idx, layer) in summary.layers.iter().enumerate() {
        s.push_str("    {\n");
        s.push_str(&format!("      \"name\": \"{}\",\n", layer.name));
        s.push_str(&format!("      \"size\": {},\n", layer.size));
        s.push_str(&format!("      \"spikes\": {}\n", layer.spikes));
        s.push_str("    }");
        if idx + 1 != summary.layers.len() {
            s.push(',');
        }
        s.push('\n');
    }
    s.push_str("  ],\n");
    s.push_str("  \"delay_quantization\": {\n");
    s.push_str(&format!(
        "    \"synapses_rounded\": {},\n",
        summary.delay_quantization.synapses_rounded
    ));
    s.push_str(&format!(
        "    \"max_error_ns\": {},\n",
        summary.delay_quantization.max_error_ns
    ));
    s.push_str(&format!(
        "    \"synapses_floored\": {}\n",
        summary.delay_quantization.synapses_floored
    ));
    s.push_str("  }\n");
    s.push_str("}\n");
    s
}

#[derive(Clone)]
struct LayerState {
    name: String,
    size: usize,
    tau_m_ns: i64,
    v_th: f64,
    v: Vec<f64>,
    spikes: u64,
}

#[derive(Clone)]
struct Connection {
    src_layer: usize,
    dst_layer: usize,
    synapses: Vec<Vec<Synapse>>,
}

#[derive(Clone)]
struct Synapse {
    dst: usize,
    weight: f64,
    delay_steps: usize,
}

#[derive(Clone)]
enum Dist {
    Const(f64),
    Uniform(f64, f64),
    Normal(f64, f64),
}

fn collect_neuron_defs(program: &Program) -> Result<BTreeMap<String, NeuronDef>, SimError> {
    let mut map = BTreeMap::new();
    for item in &program.items {
        if let Item::Neuron(def) = item {
            map.insert(def.name.name.clone(), def.clone());
        }
    }
    Ok(map)
}

fn build_layers(
    program: &Program,
    neuron_defs: &BTreeMap<String, NeuronDef>,
) -> Result<(Vec<LayerState>, BTreeMap<String, usize>), SimError> {
    let mut layers = Vec::new();
    let mut index = BTreeMap::new();

    for item in &program.items {
        if let Item::Layer(def) = item {
            let neuron = neuron_defs.get(&def.neuron.name).ok_or_else(|| SimError {
                message: format!("unknown neuron type `{}`", def.neuron.name),
            })?;
            let params = lif_params(neuron)?;

            let size = def.size as usize;
            index.insert(def.name.name.clone(), layers.len());
            layers.push(LayerState {
                name: def.name.name.clone(),
                size,
                tau_m_ns: params.tau_m_ns,
                v_th: params.v_th,
                v: vec![0.0; size],
                spikes: 0,
            });
        }
    }

    Ok((layers, index))
}

fn collect_stimuli(
    program: &Program,
    layer_index: &BTreeMap<String, usize>,
) -> Result<BTreeMap<usize, f64>, SimError> {
    let mut map: BTreeMap<usize, f64> = BTreeMap::new();
    for item in &program.items {
        if let Item::Stimulus(StimulusDef { layer, model }) = item {
            let idx = *layer_index.get(&layer.name).ok_or_else(|| SimError {
                message: format!("unknown stimulus layer `{}`", layer.name),
            })?;
            let rate = match model {
                StimulusModel::Poisson { rate } => {
                    rate_to_hz(rate, "Poisson rate").map_err(to_err)?
                }
            };
            let entry = map.entry(idx).or_insert(0.0);
            *entry += rate;
        }
    }
    Ok(map)
}

fn build_connections(
    program: &Program,
    layer_index: &BTreeMap<String, usize>,
    layers: &mut [LayerState],
    step_ns: i64,
    seed: u64,
) -> Result<(Vec<Connection>, DelayQuantization), SimError> {
    let mut rng = Rng::stream(seed, stream::STRUCTURE);
    let mut connections = Vec::new();
    let mut quantization = DelayQuantization::default();

    for item in &program.items {
        let Item::Connect(ConnectDef { src, dst, body }) = item else {
            continue;
        };
        let src_idx = *layer_index.get(&src.name).ok_or_else(|| SimError {
            message: format!("unknown source layer `{}`", src.name),
        })?;
        let dst_idx = *layer_index.get(&dst.name).ok_or_else(|| SimError {
            message: format!("unknown destination layer `{}`", dst.name),
        })?;

        let weight_dist = find_dist(body, "w", UnitKind::Voltage)?;
        let delay_dist = find_dist(body, "d", UnitKind::Time)?;

        let src_size = layers[src_idx].size;
        let dst_size = layers[dst_idx].size;
        let mut synapses = vec![Vec::with_capacity(dst_size); src_size];

        for syn_list in synapses.iter_mut() {
            for dst_i in 0..dst_size {
                let weight = sample_dist(&weight_dist, &mut rng);
                let delay_ns = sample_dist(&delay_dist, &mut rng);
                if !weight.is_finite() {
                    return Err(SimError {
                        message: "weight distribution produced a non-finite value".to_string(),
                    });
                }
                if !delay_ns.is_finite() {
                    return Err(SimError {
                        message: "delay distribution produced a non-finite value".to_string(),
                    });
                }
                if delay_ns < 0.0 {
                    return Err(SimError {
                        message: "negative delay is not allowed".to_string(),
                    });
                }
                let delay_ns_i = delay_ns.round() as i64;

                // Round to the nearest whole step and count the coercion. Rejecting delays
                // that don't land exactly on the grid would make Normal and Uniform delay
                // distributions unusable, but a silently moved time is worse. Report it.
                let mut steps = (delay_ns_i as f64 / step_ns as f64).round() as i64;
                let error_ns = (steps * step_ns - delay_ns_i).abs();
                if error_ns > 0 {
                    quantization.synapses_rounded += 1;
                    quantization.max_error_ns = quantization.max_error_ns.max(error_ns);
                }

                // A spike can't be delivered in the step it was emitted, so one step is the
                // floor. This is a separate rule from grid rounding and is counted separately.
                if steps < 1 {
                    steps = 1;
                    quantization.synapses_floored += 1;
                }

                syn_list.push(Synapse {
                    dst: dst_i,
                    weight,
                    delay_steps: steps as usize,
                });
            }
        }

        connections.push(Connection {
            src_layer: src_idx,
            dst_layer: dst_idx,
            synapses,
        });
    }

    Ok((connections, quantization))
}

fn lif_params(neuron: &NeuronDef) -> Result<LifParams, SimError> {
    let mut tau_m_ns = 20_000_000;
    let mut v_th = 1.0;
    for assign in &neuron.body {
        match assign.key.name.as_str() {
            "tau_m" => {
                if let Expr::Number(q) = &assign.value {
                    tau_m_ns = time_to_nanos(q, "tau_m").map_err(to_err)?;
                    if tau_m_ns <= 0 {
                        return Err(SimError {
                            message: "tau_m must be positive".to_string(),
                        });
                    }
                } else {
                    return Err(SimError {
                        message: "tau_m must be a time quantity".to_string(),
                    });
                }
            }
            "v_th" => {
                if let Expr::Number(q) = &assign.value {
                    // The volt is the canonical membrane unit. A bare number is volts, so
                    // `1000 mV` and `1 V` land on the same threshold.
                    v_th = volts(q, "v_th").map_err(to_err)?;
                } else {
                    return Err(SimError {
                        message: "v_th must be a number".to_string(),
                    });
                }
            }
            _ => {}
        }
    }
    Ok(LifParams { tau_m_ns, v_th })
}

struct LifParams {
    tau_m_ns: i64,
    v_th: f64,
}

fn find_dist(body: &[Assign], key: &str, kind: UnitKind) -> Result<Dist, SimError> {
    let expr = body.iter().find(|a| a.key.name == key).map(|a| &a.value);
    match expr {
        Some(expr) => dist_from_expr(expr, kind),
        None => Ok(match kind {
            // No delay written means deliver as soon as the grid allows, which is one step.
            UnitKind::Time => Dist::Const(0.0),
            _ => Dist::Const(1.0),
        }),
    }
}

/// Canonical base per kind: integer nanoseconds for time, volts for a weight.
fn canonical(q: &converge_lang::ast::Quantity, kind: UnitKind) -> Result<f64, SimError> {
    match kind {
        UnitKind::Time => Ok(time_to_nanos(q, "delay").map_err(to_err)? as f64),
        UnitKind::Voltage => volts(q, "weight").map_err(to_err),
        UnitKind::Rate => rate_to_hz(q, "rate").map_err(to_err),
    }
}

fn dist_from_expr(expr: &Expr, kind: UnitKind) -> Result<Dist, SimError> {
    match expr {
        Expr::Number(q) => Ok(Dist::Const(canonical(q, kind)?)),
        Expr::Call(call) => {
            let mut args = Vec::new();
            for arg in &call.args {
                let expr = match arg {
                    CallArg::Positional(e) => e,
                    CallArg::Named { value, .. } => value,
                };
                if let Expr::Number(q) = expr {
                    args.push(canonical(q, kind)?);
                } else {
                    return Err(SimError {
                        message: "distribution arguments must be numbers".to_string(),
                    });
                }
            }
            if args.len() != 2 {
                return Err(SimError {
                    message: "distribution requires two arguments".to_string(),
                });
            }
            match call.name.name.as_str() {
                "Uniform" => Ok(Dist::Uniform(args[0], args[1])),
                "Normal" => Ok(Dist::Normal(args[0], args[1])),
                _ => Err(SimError {
                    message: "unsupported distribution".to_string(),
                }),
            }
        }
        _ => Err(SimError {
            message: "expected number or distribution".to_string(),
        }),
    }
}

fn sample_dist(dist: &Dist, rng: &mut Rng) -> f64 {
    match dist {
        Dist::Const(v) => *v,
        Dist::Uniform(a, b) => a + (b - a) * rng.next_f64(),
        Dist::Normal(mu, sigma) => {
            // Box-Muller. u1 comes from the open interval so the log can't blow up.
            let (u1, u2) = (rng.next_f64_open(), rng.next_f64());
            let z0 = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
            mu + z0 * sigma
        }
    }
}

fn to_err(diag: converge_lang::diagnostic::Diagnostic) -> SimError {
    SimError {
        message: diag.message,
    }
}

/// Stream keys. Every draw site takes its own stream, so adding one can't shift the numbers
/// at another. Values are arbitrary; only distinctness matters.
pub mod stream {
    /// Weight and delay sampling when connections are built.
    pub const STRUCTURE: u64 = 1;
    /// Per-step stimulus draws.
    pub const STIMULUS: u64 = 2;
}

/// SplitMix64. Deterministic, portable, and no dependency.
///
/// This replaced a bare LCG that stored the seed as its state directly. That version had a
/// degenerate start: with the default `seed 0` its first output was exactly 1, so the first
/// `next_f64` was exactly 0.0 and neuron 0 of a stimulated layer always fired on step 0.
pub struct Rng {
    state: u64,
}

impl Rng {
    /// An independent stream for `seed`. Use the constants in [`stream`] for `key`.
    pub fn stream(seed: u64, key: u64) -> Self {
        // Mix on the way in so neighbouring seeds and small keys don't start out correlated.
        let state = mix64(seed ^ key.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        Self { state }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix64(self.state)
    }

    /// Uniform on `[0, 1)`.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform on the open interval `(0, 1)`. Samplers that take a log need this; `ln(0)` is
    /// not a value we want reaching a weight or a delay.
    pub fn next_f64_open(&mut self) -> f64 {
        ((self.next_u64() >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }
}

fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use converge_lang::parser::parse_program;

    const POISSON: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/poisson.cv"
    ));
    const HELLO: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/hello.cv"
    ));

    #[test]
    fn deterministic_summary() {
        let src = r#"
neuron LIF { tau_m = 10 ms, v_th = 1.0 }
layer Input[2] : LIF
layer Output[2] : LIF
connect Input -> Output { w = 1.0, d = 1 ms }
stimulus Input = Poisson(rate=50 Hz)
run for 10 ms step 1 ms
seed 42
"#;
        let program = parse_program(src).expect("parse");
        let a = simulate(&program).expect("sim");
        let b = simulate(&program).expect("sim");
        assert_eq!(a.total_spikes, b.total_spikes);
        assert_eq!(a.layers[0].spikes, b.layers[0].spikes);
    }

    // A saturated stimulus (p == 1.0) injects exactly v_th every step, so the neuron has to
    // fire every step. Under a leak applied to fresh input this lands at 0.95 and never fires,
    // which is the regression this pins.
    #[test]
    fn unit_input_reaches_unit_threshold() {
        let src = r#"
neuron LIF { tau_m = 20 ms, v_th = 1.0 }
layer Only[1] : LIF
stimulus Only = Poisson(rate=1 kHz)
run for 10 ms step 1 ms
seed 7
"#;
        let program = parse_program(src).expect("parse");
        let summary = simulate(&program).expect("sim");
        assert_eq!(summary.total_spikes, 10);
    }

    // Drive has to reach the downstream layer, not just the stimulated one.
    #[test]
    fn drive_propagates_downstream() {
        let program = parse_program(POISSON).expect("parse");
        let summary = simulate(&program).expect("sim");
        assert_eq!(summary.total_spikes, 6);
        assert_eq!(summary.layers[0].name, "Input");
        assert_eq!(summary.layers[0].spikes, 2);
        assert_eq!(summary.layers[1].name, "Output");
        assert_eq!(summary.layers[1].spikes, 4);
        assert_eq!(summary.delay_quantization, DelayQuantization::default());
    }

    fn sim(src: &str) -> SimSummary {
        let program = parse_program(src).expect("parse");
        simulate(&program).expect("sim")
    }

    // An omitted `d` is a zero delay, and zero rounds up to the one step floor. It used to
    // land in a ring bucket that had already been drained this step, so it arrived queue_len
    // steps later instead of one.
    #[test]
    fn omitted_delay_is_one_step() {
        let omitted = sim(r#"
neuron LIF { tau_m = 20 ms, v_th = 1.0 }
layer A[2] : LIF
layer B[2] : LIF
connect A -> B { w = 1.0 }
stimulus A = Poisson(rate=1 kHz)
run for 10 ms step 1 ms
seed 3
"#);
        let explicit = sim(r#"
neuron LIF { tau_m = 20 ms, v_th = 1.0 }
layer A[2] : LIF
layer B[2] : LIF
connect A -> B { w = 1.0, d = 1 ms }
stimulus A = Poisson(rate=1 kHz)
run for 10 ms step 1 ms
seed 3
"#);
        assert_eq!(omitted.total_spikes, explicit.total_spikes);
        assert_eq!(omitted.layers[1].spikes, explicit.layers[1].spikes);
        assert_eq!(omitted.delay_quantization.synapses_floored, 4);
        assert_eq!(explicit.delay_quantization.synapses_floored, 0);
    }

    // The ring is sized from the longest delay in the whole network. A long-delay connection
    // somewhere else must not change when a short-delay connection delivers.
    #[test]
    fn long_delay_elsewhere_does_not_shift_short_delay() {
        let alone = sim(r#"
neuron LIF { tau_m = 20 ms, v_th = 1.0 }
layer A[2] : LIF
layer B[2] : LIF
connect A -> B { w = 1.0, d = 1 ms }
stimulus A = Poisson(rate=1 kHz)
run for 10 ms step 1 ms
seed 5
"#);
        let with_long = sim(r#"
neuron LIF { tau_m = 20 ms, v_th = 1.0 }
layer A[2] : LIF
layer B[2] : LIF
layer C[2] : LIF
connect A -> B { w = 1.0, d = 1 ms }
connect A -> C { w = 1.0, d = 5 ms }
stimulus A = Poisson(rate=1 kHz)
run for 10 ms step 1 ms
seed 5
"#);
        assert_eq!(alone.layers[1].name, "B");
        assert_eq!(with_long.layers[1].name, "B");
        assert_eq!(alone.layers[1].spikes, with_long.layers[1].spikes);
    }

    // Delays off the step grid are rounded to it and the coercion is reported, not hidden and
    // not fatal. hello.cv draws its delays from a Normal, so effectively none of them land on
    // the grid, and the whole file used to be unsimulatable because of it.
    #[test]
    fn off_grid_delays_are_rounded_and_reported() {
        let summary = sim(HELLO);
        assert_eq!(summary.delay_quantization.synapses_rounded, 8);
        assert!(summary.delay_quantization.max_error_ns > 0);
        assert!(summary.delay_quantization.max_error_ns < summary.step_ns);
    }

    // decay is step/tau_m. Past 1.0 the membrane flips sign every step, so refuse the run.
    #[test]
    fn step_may_not_exceed_tau_m() {
        let program = parse_program(
            r#"
neuron Fast { tau_m = 1 ms, v_th = 1.0 }
layer A[1] : Fast
run for 10 ms step 2 ms
"#,
        )
        .expect("parse");
        let err = simulate(&program).expect_err("should reject");
        assert!(err.message.contains("exceeds tau_m"), "{}", err.message);
    }

    // The old LCG stored the seed as its state, so seed 0 produced exactly 0.0 as its first
    // draw and neuron 0 always fired on step 0.
    #[test]
    fn default_seed_has_no_degenerate_first_draw() {
        let mut rng = Rng::stream(0, stream::STIMULUS);
        let first = rng.next_f64();
        assert!(first > 0.0 && first < 1.0, "{first}");
    }

    #[test]
    fn streams_are_independent() {
        let mut a = Rng::stream(42, stream::STRUCTURE);
        let mut b = Rng::stream(42, stream::STIMULUS);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn open_interval_never_yields_zero() {
        let mut rng = Rng::stream(0, stream::STRUCTURE);
        for _ in 0..10_000 {
            let v = rng.next_f64_open();
            assert!(v > 0.0 && v < 1.0, "{v}");
        }
    }
}
