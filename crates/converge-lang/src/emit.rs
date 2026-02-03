use crate::ast::*;

pub fn cvir_json(program: &Program) -> String {
    let mut w = JsonWriter::new();
    w.obj_begin();

    w.key("items");
    w.array_begin();
    for (idx, item) in program.items.iter().enumerate() {
        if idx != 0 {
            w.comma();
        }
        w.nl();
        emit_item(&mut w, item);
    }
    if !program.items.is_empty() {
        w.nl();
    }
    w.array_end();

    w.nl();
    w.obj_end();
    w.nl();
    w.finish()
}

fn emit_item(w: &mut JsonWriter, item: &Item) {
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
        Item::Run(d) => {
            w.kv_str("kind", "run");
            w.comma_nl();
            w.key("duration");
            emit_quantity(w, &d.duration);
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
    w.obj_begin();
    w.kv_f64("value", q.value);
    if let Some(u) = &q.unit {
        w.comma_nl();
        w.kv_str("unit", &u.name);
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
