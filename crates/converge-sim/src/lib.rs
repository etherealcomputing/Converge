use std::collections::HashMap;
use std::fmt;

use converge_lang::ast::{
    Assign, CallArg, ConnectDef, Expr, Item, NeuronDef, Program, StimulusDef, StimulusModel,
};
use converge_lang::units::{rate_to_hz, time_to_nanos};

#[derive(Debug, Clone)]
pub struct SimSummary {
    pub duration_ns: i64,
    pub step_ns: i64,
    pub seed: u64,
    pub total_spikes: u64,
    pub layers: Vec<LayerSummary>,
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
    let stimuli = collect_stimuli(program, &layer_index)?;
    let connections = build_connections(program, &layer_index, &mut layers, step_ns, seed)?;

    let mut rng = Rng::new(seed);
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

            let decay = step_ns as f64 / layer.tau_m_ns as f64;
            for i in 0..layer.size {
                layer.v[i] += (-layer.v[i]) * decay;
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
    s.push_str("  ]\n");
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

fn collect_neuron_defs(program: &Program) -> Result<HashMap<String, NeuronDef>, SimError> {
    let mut map = HashMap::new();
    for item in &program.items {
        if let Item::Neuron(def) = item {
            map.insert(def.name.name.clone(), def.clone());
        }
    }
    Ok(map)
}

fn build_layers(
    program: &Program,
    neuron_defs: &HashMap<String, NeuronDef>,
) -> Result<(Vec<LayerState>, HashMap<String, usize>), SimError> {
    let mut layers = Vec::new();
    let mut index = HashMap::new();

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
    layer_index: &HashMap<String, usize>,
) -> Result<HashMap<usize, f64>, SimError> {
    let mut map: HashMap<usize, f64> = HashMap::new();
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
    layer_index: &HashMap<String, usize>,
    layers: &mut [LayerState],
    step_ns: i64,
    seed: u64,
) -> Result<Vec<Connection>, SimError> {
    let mut rng = Rng::new(seed ^ 0x9E3779B97F4A7C15);
    let mut connections = Vec::new();

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

        let weight_dist = find_dist(body, "w", false)?;
        let delay_dist = find_dist(body, "d", true)?;

        let src_size = layers[src_idx].size;
        let dst_size = layers[dst_idx].size;
        let mut synapses = vec![Vec::with_capacity(dst_size); src_size];

        for syn_list in synapses.iter_mut() {
            for dst_i in 0..dst_size {
                let weight = sample_dist(&weight_dist, &mut rng);
                let delay_ns = sample_dist(&delay_dist, &mut rng);
                if delay_ns < 0.0 {
                    return Err(SimError {
                        message: "negative delay is not allowed".to_string(),
                    });
                }
                let delay_ns_i = delay_ns.round() as i64;
                if delay_ns_i % step_ns != 0 {
                    return Err(SimError {
                        message: "delay must be divisible by step".to_string(),
                    });
                }
                let delay_steps = (delay_ns_i / step_ns) as usize;
                syn_list.push(Synapse {
                    dst: dst_i,
                    weight,
                    delay_steps,
                });
            }
        }

        connections.push(Connection {
            src_layer: src_idx,
            dst_layer: dst_idx,
            synapses,
        });
    }

    Ok(connections)
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
                    v_th = q.value;
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

fn find_dist(body: &[Assign], key: &str, is_time: bool) -> Result<Dist, SimError> {
    let expr = body.iter().find(|a| a.key.name == key).map(|a| &a.value);
    match expr {
        Some(expr) => dist_from_expr(expr, is_time),
        None => Ok(if is_time {
            Dist::Const(0.0)
        } else {
            Dist::Const(1.0)
        }),
    }
}

fn dist_from_expr(expr: &Expr, is_time: bool) -> Result<Dist, SimError> {
    match expr {
        Expr::Number(q) => {
            let value = if is_time {
                time_to_nanos(q, "delay").map_err(to_err)? as f64
            } else {
                q.value
            };
            Ok(Dist::Const(value))
        }
        Expr::Call(call) => {
            let mut args = Vec::new();
            for arg in &call.args {
                let expr = match arg {
                    CallArg::Positional(e) => e,
                    CallArg::Named { value, .. } => value,
                };
                if let Expr::Number(q) = expr {
                    let value = if is_time {
                        time_to_nanos(q, "delay").map_err(to_err)? as f64
                    } else {
                        q.value
                    };
                    args.push(value);
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
            let (u1, u2) = (rng.next_f64(), rng.next_f64());
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

struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.state
    }

    fn next_f64(&mut self) -> f64 {
        let v = self.next_u64() >> 11;
        (v as f64) / ((1u64 << 53) as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use converge_lang::parser::parse_program;

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
}
