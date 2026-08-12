use std::collections::HashMap;

use crate::ast::{Assign, ConnectDef, Expr, Item, LayerDef, NeuronDef, Program, StimulusModel};
use crate::diagnostic::Diagnostic;
use crate::units::{UnitKind, expect_rate, expect_time, expect_volts, time_to_nanos};

/// Keys a `neuron` body understands. Anything else is a typo, and a typo that gets ignored is
/// a parameter you think you set.
const NEURON_KEYS: &[&str] = &["tau_m", "v_th"];

/// Keys a `connect` body understands.
const CONNECT_KEYS: &[&str] = &["w", "d"];

pub fn validate(program: &Program) -> Result<(), Vec<Diagnostic>> {
    let mut diags = Vec::new();

    let mut neurons: HashMap<String, crate::diagnostic::Span> = HashMap::new();
    let mut layers: HashMap<String, (crate::diagnostic::Span, String)> = HashMap::new();
    let mut seed_count = 0;
    let mut run_count = 0;

    for item in &program.items {
        match item {
            Item::Neuron(NeuronDef { name, .. }) => {
                if neurons.contains_key(&name.name) {
                    diags.push(
                        Diagnostic::new(format!("duplicate neuron `{}`", name.name))
                            .with_span(name.span.clone()),
                    );
                } else {
                    neurons.insert(name.name.clone(), name.span.clone());
                }
            }
            Item::Layer(LayerDef { name, neuron, .. }) => {
                if layers.contains_key(&name.name) {
                    diags.push(
                        Diagnostic::new(format!("duplicate layer `{}`", name.name))
                            .with_span(name.span.clone()),
                    );
                } else {
                    layers.insert(name.name.clone(), (name.span.clone(), neuron.name.clone()));
                }
            }
            Item::Seed(_) => {
                seed_count += 1;
            }
            Item::Run(_) => {
                run_count += 1;
            }
            _ => {}
        }
    }

    if seed_count > 1 {
        diags.push(Diagnostic::new("only one `seed` statement is allowed"));
    }
    if run_count == 0 {
        diags.push(Diagnostic::new("missing `run` statement"));
    } else if run_count > 1 {
        diags.push(Diagnostic::new("only one `run` statement is allowed"));
    }

    for item in &program.items {
        match item {
            Item::Neuron(NeuronDef { body, .. }) => {
                check_keys(&mut diags, body, NEURON_KEYS, "neuron");
                for assign in body {
                    let res = match assign.key.name.as_str() {
                        "tau_m" => validate_quantity_expr(
                            &assign.value,
                            UnitKind::Time,
                            "membrane time constant",
                        ),
                        "v_th" => {
                            validate_quantity_expr(&assign.value, UnitKind::Voltage, "threshold")
                        }
                        _ => Ok(()),
                    };
                    if let Err(diag) = res {
                        diags.push(diag);
                    }
                }
            }
            Item::Layer(LayerDef { neuron, .. }) => {
                if !neurons.contains_key(&neuron.name) {
                    diags.push(
                        Diagnostic::new(format!("unknown neuron type `{}`", neuron.name))
                            .with_span(neuron.span.clone()),
                    );
                }
            }
            Item::Connect(ConnectDef { src, dst, body }) => {
                check_keys(&mut diags, body, CONNECT_KEYS, "connect");
                if !layers.contains_key(&src.name) {
                    diags.push(
                        Diagnostic::new(format!("unknown source layer `{}`", src.name))
                            .with_span(src.span.clone()),
                    );
                }
                if !layers.contains_key(&dst.name) {
                    diags.push(
                        Diagnostic::new(format!("unknown destination layer `{}`", dst.name))
                            .with_span(dst.span.clone()),
                    );
                }
                for assign in body {
                    let res = match assign.key.name.as_str() {
                        "d" => validate_quantity_expr(
                            &assign.value,
                            UnitKind::Time,
                            "connection delay",
                        ),
                        "w" => validate_quantity_expr(
                            &assign.value,
                            UnitKind::Voltage,
                            "connection weight",
                        ),
                        _ => Ok(()),
                    };
                    if let Err(diag) = res {
                        diags.push(diag);
                    }
                }
            }
            Item::Run(run) => {
                if let Err(diag) = expect_positive_time(&run.duration, "run duration") {
                    diags.push(diag);
                }
                if let Some(step) = &run.step
                    && let Err(diag) = expect_positive_time(step, "run step")
                {
                    diags.push(diag);
                }
            }
            Item::Stimulus(stim) => {
                if !layers.contains_key(&stim.layer.name) {
                    diags.push(
                        Diagnostic::new(format!("unknown stimulus layer `{}`", stim.layer.name))
                            .with_span(stim.layer.span.clone()),
                    );
                }
                match &stim.model {
                    StimulusModel::Poisson { rate } => {
                        if let Err(diag) = expect_rate(rate, "Poisson rate") {
                            diags.push(diag);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if diags.is_empty() { Ok(()) } else { Err(diags) }
}

fn check_keys(diags: &mut Vec<Diagnostic>, body: &[Assign], known: &[&str], what: &str) {
    for assign in body {
        if !known.contains(&assign.key.name.as_str()) {
            diags.push(
                Diagnostic::new(format!(
                    "unknown {what} key `{}`, expected one of {}",
                    assign.key.name,
                    known.join(", ")
                ))
                .with_span(assign.key.span.clone()),
            );
        }
    }
}

/// A quantity position, either written directly or sampled from a distribution. Every leaf has
/// to carry the expected kind.
fn validate_quantity_expr(expr: &Expr, kind: UnitKind, context: &str) -> Result<(), Diagnostic> {
    match expr {
        Expr::Number(q) => expect_kind(q, kind, context),
        Expr::Call(call) => {
            if call.name.name == "Normal" || call.name.name == "Uniform" {
                for arg in &call.args {
                    let expr = match arg {
                        crate::ast::CallArg::Positional(e) => e,
                        crate::ast::CallArg::Named { value, .. } => value,
                    };
                    if let Expr::Number(q) = expr {
                        expect_kind(q, kind, context)?;
                    } else {
                        return Err(Diagnostic::new(format!(
                            "expected {} quantity for {context}",
                            kind.label()
                        ))
                        .with_span(call.name.span.clone()));
                    }
                }
                Ok(())
            } else {
                Err(Diagnostic::new(format!("unsupported {context} expression"))
                    .with_span(call.name.span.clone()))
            }
        }
        _ => Err(
            Diagnostic::new(format!("expected {} quantity for {context}", kind.label()))
                .with_span(span_of(expr)),
        ),
    }
}

fn expect_kind(q: &crate::ast::Quantity, kind: UnitKind, context: &str) -> Result<(), Diagnostic> {
    match kind {
        UnitKind::Time => expect_time(q, context),
        UnitKind::Rate => expect_rate(q, context),
        UnitKind::Voltage => expect_volts(q, context),
    }
}

fn span_of(expr: &Expr) -> crate::diagnostic::Span {
    match expr {
        Expr::Number(q) => q.span.clone(),
        Expr::String(_) => crate::diagnostic::Span::new(0, 0),
        Expr::Ident(id) => id.span.clone(),
        Expr::Call(call) => call.name.span.clone(),
    }
}

fn expect_positive_time(q: &crate::ast::Quantity, context: &str) -> Result<(), Diagnostic> {
    let ns = time_to_nanos(q, context)?;
    if ns <= 0 {
        Err(Diagnostic::new(format!("{context} must be positive")).with_span(q.span.clone()))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::parse_program;

    fn diags(src: &str) -> Vec<String> {
        let program = parse_program(src).expect("parse");
        match super::validate(&program) {
            Ok(()) => Vec::new(),
            Err(ds) => ds.into_iter().map(|d| d.message).collect(),
        }
    }

    fn assert_reports(src: &str, needle: &str) {
        let found = diags(src);
        assert!(
            found.iter().any(|m| m.contains(needle)),
            "expected a diagnostic containing {needle:?}, got {found:?}"
        );
    }

    fn assert_clean(src: &str) {
        let found = diags(src);
        assert!(found.is_empty(), "expected no diagnostics, got {found:?}");
    }

    const OK: &str = r#"
neuron LIF { tau_m = 20 ms, v_th = 1.0 V }
layer A[2] : LIF
layer B[2] : LIF
connect A -> B { w = 0.5, d = 1 ms }
run for 10 ms step 1 ms
"#;

    #[test]
    fn accepts_a_well_formed_program() {
        assert_clean(OK);
    }

    // A key nobody reads is a parameter you think you set. These used to be dropped in
    // silence, both by the validator and by the simulator.
    #[test]
    fn unknown_neuron_key_is_reported() {
        assert_reports(
            "neuron LIF { tau_m = 20 ms, v_reset = 0.0 }\nlayer A[1] : LIF\nrun for 10 ms\n",
            "unknown neuron key `v_reset`",
        );
    }

    #[test]
    fn unknown_connect_key_is_reported() {
        assert_reports(
            "neuron LIF { tau_m = 20 ms }\nlayer A[1] : LIF\nlayer B[1] : LIF\nconnect A -> B { weight = 1.0 }\nrun for 10 ms\n",
            "unknown connect key `weight`",
        );
    }

    #[test]
    fn threshold_rejects_a_non_voltage_unit() {
        assert_reports(
            "neuron LIF { tau_m = 20 ms, v_th = 1.0 Hz }\nlayer A[1] : LIF\nrun for 10 ms\n",
            "unsupported voltage unit `Hz`",
        );
    }

    #[test]
    fn threshold_accepts_voltage_units_and_bare_numbers() {
        for v in ["1.0 V", "1000 mV", "1.0 uV", "1.0"] {
            assert_clean(&format!(
                "neuron LIF {{ tau_m = 20 ms, v_th = {v} }}\nlayer A[1] : LIF\nrun for 10 ms\n"
            ));
        }
    }

    // tau_m being a time was only ever enforced inside the simulator, so `check` passed and
    // `sim` failed.
    #[test]
    fn tau_m_must_be_a_time() {
        assert_reports(
            "neuron LIF { tau_m = 20 mV }\nlayer A[1] : LIF\nrun for 10 ms\n",
            "unsupported time unit `mV`",
        );
    }

    // `w` was completely unchecked in the front end.
    #[test]
    fn weight_rejects_a_non_voltage_unit() {
        assert_reports(
            "neuron LIF { tau_m = 20 ms }\nlayer A[1] : LIF\nlayer B[1] : LIF\nconnect A -> B { w = 1.0 ms }\nrun for 10 ms\n",
            "unsupported voltage unit `ms`",
        );
    }

    #[test]
    fn weight_checks_inside_a_distribution() {
        assert_reports(
            "neuron LIF { tau_m = 20 ms }\nlayer A[1] : LIF\nlayer B[1] : LIF\nconnect A -> B { w = Uniform(0.5 ms, 1.0 ms) }\nrun for 10 ms\n",
            "unsupported voltage unit `ms`",
        );
    }

    #[test]
    fn delay_still_requires_a_time_unit() {
        assert_reports(
            "neuron LIF { tau_m = 20 ms }\nlayer A[1] : LIF\nlayer B[1] : LIF\nconnect A -> B { d = 1.0 }\nrun for 10 ms\n",
            "missing unit for connection delay",
        );
    }

    #[test]
    fn both_examples_validate() {
        assert_clean(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/hello.cv"
        )));
        assert_clean(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/poisson.cv"
        )));
    }
}
