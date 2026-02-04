use std::collections::HashMap;

use crate::ast::{ConnectDef, Expr, Item, LayerDef, NeuronDef, Program, StimulusModel};
use crate::diagnostic::Diagnostic;
use crate::units::{expect_rate, expect_time, time_to_nanos};

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
            Item::Layer(LayerDef { neuron, .. }) => {
                if !neurons.contains_key(&neuron.name) {
                    diags.push(
                        Diagnostic::new(format!("unknown neuron type `{}`", neuron.name))
                            .with_span(neuron.span.clone()),
                    );
                }
            }
            Item::Connect(ConnectDef { src, dst, body }) => {
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
                    if assign.key.name == "d"
                        && let Err(diag) = validate_time_expr(&assign.value, "connection delay")
                    {
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

fn validate_time_expr(expr: &Expr, context: &str) -> Result<(), Diagnostic> {
    match expr {
        Expr::Number(q) => expect_time(q, context),
        Expr::Call(call) => {
            if call.name.name == "Normal" || call.name.name == "Uniform" {
                for arg in &call.args {
                    let expr = match arg {
                        crate::ast::CallArg::Positional(e) => e,
                        crate::ast::CallArg::Named { value, .. } => value,
                    };
                    if let Expr::Number(q) = expr {
                        expect_time(q, context)?;
                    } else {
                        return Err(Diagnostic::new("expected time quantity")
                            .with_span(call.name.span.clone()));
                    }
                }
                Ok(())
            } else {
                Err(Diagnostic::new("unsupported delay expression")
                    .with_span(call.name.span.clone()))
            }
        }
        _ => Err(Diagnostic::new("expected time quantity").with_span(span_of(expr))),
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
