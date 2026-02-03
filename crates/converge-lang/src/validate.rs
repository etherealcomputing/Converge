use std::collections::HashMap;

use crate::ast::{ConnectDef, Item, LayerDef, NeuronDef, Program};
use crate::diagnostic::Diagnostic;

pub fn validate(program: &Program) -> Result<(), Vec<Diagnostic>> {
    let mut diags = Vec::new();

    let mut neurons: HashMap<String, crate::diagnostic::Span> = HashMap::new();
    let mut layers: HashMap<String, (crate::diagnostic::Span, String)> = HashMap::new();

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
            _ => {}
        }
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
            Item::Connect(ConnectDef { src, dst, .. }) => {
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
            }
            _ => {}
        }
    }

    if diags.is_empty() {
        Ok(())
    } else {
        Err(diags)
    }
}

