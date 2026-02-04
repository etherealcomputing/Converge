use crate::ast::*;

pub fn cvir_json(program: &Program) -> String {
    let mut w = JsonWriter::new();
    w.obj_begin();

    w.kv_str("cvir_version", "0.2");
    w.comma_nl();
    w.key("items");
    w.array_begin();
    let seed = program
        .items
        .iter()
        .find_map(|item| match item {
            Item::Seed(s) => Some(s.value),
            _ => None,
        })
        .unwrap_or(0);

    let mut first = true;
    for item in program.items.iter() {
        if matches!(item, Item::Seed(_)) {
            continue;
        }
        if !first {
            w.comma();
        }
        first = false;
        w.nl();
        emit_item(&mut w, item, seed);
    }
    if !first {
        w.nl();
    }
    w.array_end();

    w.nl();
    w.obj_end();
    w.nl();
    w.finish()
}

fn emit_item(w: &mut JsonWriter, item: &Item, seed: u64) {
    w.obj_begin();
    match item {
        Item::Neuron(d) => {
            w.kv_str("kind", "neuron");
            w.comma_nl();
            w.kv_str("name", &d.name.name);
            w.comma_nl();
            w.key("body");
            emit_assigns(w, &d.body);
        }
        Item::Layer(d) => {
            w.kv_str("kind", "layer");
            w.comma_nl();
            w.kv_str("name", &d.name.name);
            w.comma_nl();
            w.kv_u64("size", d.size);
            w.comma_nl();
            w.kv_str("neuron", &d.neuron.name);
        }
        Item::Connect(d) => {
            w.kv_str("kind", "connect");
            w.comma_nl();
            w.kv_str("src", &d.src.name);
            w.comma_nl();
            w.kv_str("dst", &d.dst.name);
            w.comma_nl();
            w.key("body");
            emit_assigns(w, &d.body);
        }
        Item::Stimulus(d) => {
            w.kv_str("kind", "stimulus");
            w.comma_nl();
            w.kv_str("layer", &d.layer.name);
            w.comma_nl();
            w.key("model");
            emit_stimulus_model(w, &d.model);
        }
        Item::Run(d) => {
            w.kv_str("kind", "run");
            w.comma_nl();
            w.key("duration");
            emit_quantity(w, &d.duration);
            w.comma_nl();
            w.key("step");
            if let Some(step) = &d.step {
                emit_quantity(w, step);
            } else {
                emit_quantity_value(w, 1.0, Some("ms"));
            }
            w.comma_nl();
            w.kv_u64("seed", seed);
        }
        Item::Seed(_) => {}
    }
    w.obj_end();
}

fn emit_stimulus_model(w: &mut JsonWriter, model: &StimulusModel) {
    w.obj_begin();
    match model {
        StimulusModel::Poisson { rate } => {
            w.kv_str("type", "poisson");
            w.comma_nl();
            w.key("rate");
            emit_quantity(w, rate);
        }
    }
    w.obj_end();
}

fn emit_assigns(w: &mut JsonWriter, assigns: &[Assign]) {
    w.array_begin();
    for (idx, a) in assigns.iter().enumerate() {
        if idx != 0 {
            w.comma();
        }
        w.nl();
        w.obj_begin();
        w.kv_str("key", &a.key.name);
        w.comma_nl();
        w.key("value");
        emit_expr(w, &a.value);
        w.obj_end();
    }
    if !assigns.is_empty() {
        w.nl();
    }
    w.array_end();
}

fn emit_expr(w: &mut JsonWriter, e: &Expr) {
    match e {
        Expr::Number(q) => emit_quantity(w, q),
        Expr::String(s) => w.str(s),
        Expr::Ident(id) => {
            w.obj_begin();
            w.kv_str("ident", &id.name);
            w.obj_end();
        }
        Expr::Call(c) => {
            w.obj_begin();
            w.kv_str("call", &c.name.name);
            w.comma_nl();
            w.key("args");
            w.array_begin();
            for (idx, arg) in c.args.iter().enumerate() {
                if idx != 0 {
                    w.comma();
                }
                w.nl();
                match arg {
                    CallArg::Positional(e) => emit_expr(w, e),
                    CallArg::Named { name, value } => {
                        w.obj_begin();
                        w.kv_str("name", &name.name);
                        w.comma_nl();
                        w.key("value");
                        emit_expr(w, value);
                        w.obj_end();
                    }
                }
            }
            if !c.args.is_empty() {
                w.nl();
            }
            w.array_end();
            w.obj_end();
        }
    }
}

fn emit_quantity(w: &mut JsonWriter, q: &Quantity) {
    emit_quantity_value(w, q.value, q.unit.as_ref().map(|u| u.name.as_str()));
}

fn emit_quantity_value(w: &mut JsonWriter, value: f64, unit: Option<&str>) {
    w.obj_begin();
    w.kv_f64("value", value);
    if let Some(u) = unit {
        w.comma_nl();
        w.kv_str("unit", u);
    }
    w.obj_end();
}

struct JsonWriter {
    out: String,
    indent: usize,
    at_line_start: bool,
}

impl JsonWriter {
    fn new() -> Self {
        Self {
            out: String::new(),
            indent: 0,
            at_line_start: true,
        }
    }

    fn finish(self) -> String {
        self.out
    }

    fn write(&mut self, s: &str) {
        if self.at_line_start {
            for _ in 0..self.indent {
                self.out.push_str("  ");
            }
            self.at_line_start = false;
        }
        self.out.push_str(s);
    }

    fn nl(&mut self) {
        self.out.push('\n');
        self.at_line_start = true;
    }

    fn comma(&mut self) {
        self.write(",");
    }

    fn comma_nl(&mut self) {
        self.comma();
        self.nl();
    }

    fn obj_begin(&mut self) {
        self.write("{");
        self.indent += 1;
    }

    fn obj_end(&mut self) {
        self.indent = self.indent.saturating_sub(1);
        self.nl();
        self.write("}");
    }

    fn array_begin(&mut self) {
        self.write("[");
        self.indent += 1;
    }

    fn array_end(&mut self) {
        self.indent = self.indent.saturating_sub(1);
        self.nl();
        self.write("]");
    }

    fn key(&mut self, k: &str) {
        if !self.at_line_start {
            self.nl();
        }
        self.str(k);
        self.write(": ");
    }

    fn kv_str(&mut self, k: &str, v: &str) {
        self.key(k);
        self.str(v);
    }

    fn kv_u64(&mut self, k: &str, v: u64) {
        self.key(k);
        self.write(&v.to_string());
    }

    fn kv_f64(&mut self, k: &str, v: f64) {
        self.key(k);
        if v.is_finite() {
            self.write(&format!("{v}"));
        } else if v.is_nan() {
            self.str("NaN");
        } else if v.is_sign_positive() {
            self.str("Infinity");
        } else {
            self.str("-Infinity");
        }
    }

    fn str(&mut self, s: &str) {
        self.write("\"");
        for ch in s.chars() {
            match ch {
                '"' => self.write("\\\""),
                '\\' => self.write("\\\\"),
                '\n' => self.write("\\n"),
                '\r' => self.write("\\r"),
                '\t' => self.write("\\t"),
                c if c.is_control() => self.write(&format!("\\u{:04x}", c as u32)),
                c => self.out.push(c),
            }
        }
        self.write("\"");
    }
}
