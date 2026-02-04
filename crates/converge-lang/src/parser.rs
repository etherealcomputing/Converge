use crate::ast::*;
use crate::diagnostic::{Diagnostic, Span};
use crate::lexer::{Token, TokenKind, lex};

pub fn parse_program(src: &str) -> Result<Program, Diagnostic> {
    let tokens = lex(src)?;
    let mut p = Parser::new(&tokens);
    let mut items = Vec::new();
    while !p.is_eof() {
        items.push(p.parse_item()?);
    }
    Ok(Program::new(items))
}

struct Parser<'a> {
    tokens: &'a [Token],
    i: usize,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self { tokens, i: 0 }
    }

    fn is_eof(&self) -> bool {
        self.i >= self.tokens.len()
    }

    fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.i)
    }

    fn bump(&mut self) -> Option<&'a Token> {
        let t = self.tokens.get(self.i);
        if t.is_some() {
            self.i += 1;
        }
        t
    }

    fn expect(
        &mut self,
        expected: fn(&TokenKind) -> bool,
        what: &'static str,
    ) -> Result<&'a Token, Diagnostic> {
        let t = self
            .peek()
            .ok_or_else(|| Diagnostic::new(format!("expected {what}, found end of input")))?;
        if expected(&t.kind) {
            Ok(self.bump().unwrap())
        } else {
            Err(Diagnostic::new(format!("expected {what}")).with_span(t.span.clone()))
        }
    }

    fn parse_item(&mut self) -> Result<Item, Diagnostic> {
        match self.peek().map(|t| &t.kind) {
            Some(TokenKind::KwNeuron) => Ok(Item::Neuron(self.parse_neuron_def()?)),
            Some(TokenKind::KwLayer) => Ok(Item::Layer(self.parse_layer_def()?)),
            Some(TokenKind::KwConnect) => Ok(Item::Connect(self.parse_connect_def()?)),
            Some(TokenKind::KwStimulus) => Ok(Item::Stimulus(self.parse_stimulus_def()?)),
            Some(TokenKind::KwRun) => Ok(Item::Run(self.parse_run_stmt()?)),
            Some(TokenKind::KwSeed) => Ok(Item::Seed(self.parse_seed_stmt()?)),
            Some(_) => {
                let t = self.bump().unwrap();
                Err(Diagnostic::new("unexpected token at top-level").with_span(t.span.clone()))
            }
            None => Err(Diagnostic::new("unexpected end of input")),
        }
    }

    fn parse_neuron_def(&mut self) -> Result<NeuronDef, Diagnostic> {
        self.expect(|k| matches!(k, TokenKind::KwNeuron), "`neuron`")?;
        let name = self.parse_ident("neuron name")?;
        self.expect(|k| matches!(k, TokenKind::LBrace), "`{`")?;
        let body = self.parse_assign_block()?;
        Ok(NeuronDef { name, body })
    }

    fn parse_layer_def(&mut self) -> Result<LayerDef, Diagnostic> {
        self.expect(|k| matches!(k, TokenKind::KwLayer), "`layer`")?;
        let name = self.parse_ident("layer name")?;
        self.expect(|k| matches!(k, TokenKind::LBracket), "`[`")?;
        let size = self.parse_u64("layer size")?;
        self.expect(|k| matches!(k, TokenKind::RBracket), "`]`")?;
        self.expect(|k| matches!(k, TokenKind::Colon), "`:`")?;
        let neuron = self.parse_ident("neuron type")?;
        Ok(LayerDef { name, size, neuron })
    }

    fn parse_connect_def(&mut self) -> Result<ConnectDef, Diagnostic> {
        self.expect(|k| matches!(k, TokenKind::KwConnect), "`connect`")?;
        let src = self.parse_ident("source layer")?;
        self.expect(|k| matches!(k, TokenKind::Arrow), "`->`")?;
        let dst = self.parse_ident("destination layer")?;
        self.expect(|k| matches!(k, TokenKind::LBrace), "`{`")?;
        let body = self.parse_assign_block()?;
        Ok(ConnectDef { src, dst, body })
    }

    fn parse_run_stmt(&mut self) -> Result<RunStmt, Diagnostic> {
        self.expect(|k| matches!(k, TokenKind::KwRun), "`run`")?;
        self.expect(|k| matches!(k, TokenKind::KwFor), "`for`")?;
        let duration = self.parse_quantity("duration")?;
        let step = if matches!(self.peek().map(|t| &t.kind), Some(TokenKind::KwStep)) {
            self.bump();
            Some(self.parse_quantity("step")?)
        } else {
            None
        };
        Ok(RunStmt { duration, step })
    }

    fn parse_seed_stmt(&mut self) -> Result<SeedStmt, Diagnostic> {
        let kw = self.expect(|k| matches!(k, TokenKind::KwSeed), "`seed`")?;
        let value = self.parse_u64("seed value")?;
        Ok(SeedStmt {
            value,
            span: kw.span.clone(),
        })
    }

    fn parse_stimulus_def(&mut self) -> Result<StimulusDef, Diagnostic> {
        self.expect(|k| matches!(k, TokenKind::KwStimulus), "`stimulus`")?;
        let layer = self.parse_ident("layer name")?;
        self.expect(|k| matches!(k, TokenKind::Eq), "`=`")?;
        let expr = self.parse_expr()?;
        let call = match expr {
            Expr::Call(call) => call,
            _ => {
                return Err(
                    Diagnostic::new("expected stimulus model call").with_span(layer.span.clone())
                );
            }
        };
        let model = match call.name.name.as_str() {
            "Poisson" => {
                let mut rate = None;
                for arg in call.args {
                    if let CallArg::Named { name, value } = arg
                        && name.name == "rate"
                    {
                        match value {
                            Expr::Number(q) => rate = Some(q),
                            _ => {
                                return Err(Diagnostic::new("rate must be a quantity")
                                    .with_span(name.span.clone()));
                            }
                        }
                    }
                }
                let rate = rate.ok_or_else(|| {
                    Diagnostic::new("Poisson stimulus requires rate").with_span(layer.span.clone())
                })?;
                StimulusModel::Poisson { rate }
            }
            _ => {
                return Err(
                    Diagnostic::new("unknown stimulus model").with_span(call.name.span.clone())
                );
            }
        };
        Ok(StimulusDef { layer, model })
    }

    fn parse_assign_block(&mut self) -> Result<Vec<Assign>, Diagnostic> {
        let mut assigns = Vec::new();
        while let Some(t) = self.peek() {
            match &t.kind {
                TokenKind::RBrace => {
                    self.bump();
                    break;
                }
                TokenKind::Ident(_) | TokenKind::KwRate => {
                    let key = self.parse_ident("field name")?;
                    self.expect(|k| matches!(k, TokenKind::Eq), "`=`")?;
                    let value = self.parse_expr()?;
                    // Optional commas to support single-line blocks.
                    if matches!(self.peek().map(|t| &t.kind), Some(TokenKind::Comma)) {
                        self.bump();
                    }
                    assigns.push(Assign { key, value });
                }
                _ => {
                    let t = self.bump().unwrap();
                    return Err(
                        Diagnostic::new("unexpected token in block").with_span(t.span.clone())
                    );
                }
            }
        }
        Ok(assigns)
    }

    fn parse_expr(&mut self) -> Result<Expr, Diagnostic> {
        let t = self
            .peek()
            .ok_or_else(|| Diagnostic::new("expected expression, found end of input"))?;
        match &t.kind {
            TokenKind::Number(_) => Ok(Expr::Number(self.parse_quantity("number")?)),
            TokenKind::String(_) => {
                let s = match self.bump().unwrap().kind.clone() {
                    TokenKind::String(s) => s,
                    _ => unreachable!(),
                };
                Ok(Expr::String(s))
            }
            TokenKind::Ident(_) | TokenKind::KwRate => {
                let ident = self.parse_ident("identifier")?;
                if matches!(self.peek().map(|t| &t.kind), Some(TokenKind::LParen)) {
                    Ok(Expr::Call(self.parse_call_after_name(ident)?))
                } else {
                    Ok(Expr::Ident(ident))
                }
            }
            _ => Err(Diagnostic::new("unexpected token in expression").with_span(t.span.clone())),
        }
    }

    fn parse_call_after_name(&mut self, name: Ident) -> Result<Call, Diagnostic> {
        self.expect(|k| matches!(k, TokenKind::LParen), "`(`")?;
        let mut args = Vec::new();
        while let Some(t) = self.peek() {
            match &t.kind {
                TokenKind::RParen => {
                    self.bump();
                    break;
                }
                TokenKind::Comma => {
                    self.bump();
                }
                TokenKind::Ident(_) | TokenKind::KwRate => {
                    // Lookahead for named args: ident '=' ...
                    let save = self.i;
                    let name_tok = self.bump().unwrap().clone();
                    if matches!(self.peek().map(|t| &t.kind), Some(TokenKind::Eq)) {
                        self.bump();
                        let value = self.parse_expr()?;
                        let arg_name = match name_tok.kind {
                            TokenKind::Ident(s) => Ident::new(s, name_tok.span.clone()),
                            TokenKind::KwRate => Ident::new("rate", name_tok.span.clone()),
                            _ => unreachable!(),
                        };
                        args.push(CallArg::Named {
                            name: arg_name,
                            value,
                        });
                    } else {
                        // Not named; rewind and parse as expr.
                        self.i = save;
                        let e = self.parse_expr()?;
                        args.push(CallArg::Positional(e));
                    }
                }
                _ => {
                    let e = self.parse_expr()?;
                    args.push(CallArg::Positional(e));
                }
            }
        }
        Ok(Call { name, args })
    }

    fn parse_ident(&mut self, what: &'static str) -> Result<Ident, Diagnostic> {
        let t = self.expect(
            |k| matches!(k, TokenKind::Ident(_) | TokenKind::KwRate),
            what,
        )?;
        match &t.kind {
            TokenKind::Ident(s) => Ok(Ident::new(s.clone(), t.span.clone())),
            TokenKind::KwRate => Ok(Ident::new("rate", t.span.clone())),
            _ => unreachable!(),
        }
    }

    fn parse_u64(&mut self, what: &'static str) -> Result<u64, Diagnostic> {
        let t = self.expect(|k| matches!(k, TokenKind::Number(_)), what)?;
        let s = match &t.kind {
            TokenKind::Number(s) => s.as_str(),
            _ => unreachable!(),
        };
        s.parse::<u64>().map_err(|_| {
            Diagnostic::new(format!("invalid integer for {what}")).with_span(t.span.clone())
        })
    }

    fn parse_quantity(&mut self, what: &'static str) -> Result<Quantity, Diagnostic> {
        let t = self.expect(|k| matches!(k, TokenKind::Number(_)), what)?;
        let num_str = match &t.kind {
            TokenKind::Number(s) => s.as_str(),
            _ => unreachable!(),
        };
        let value = num_str.parse::<f64>().map_err(|_| {
            Diagnostic::new(format!("invalid number for {what}")).with_span(t.span.clone())
        })?;

        // Optional unit: an identifier immediately after the number.
        let unit = match self.peek().map(|t| &t.kind) {
            Some(TokenKind::Ident(_)) => Some(self.parse_ident("unit")?),
            _ => None,
        };

        let end = unit.as_ref().map(|u| u.span.end).unwrap_or(t.span.end);
        let span = Span::new(t.span.start, end);

        Ok(Quantity { value, unit, span })
    }
}

pub fn format_diagnostic(src: &str, diag: &Diagnostic) -> String {
    match &diag.span {
        None => diag.to_string(),
        Some(span) => {
            let mut line_start = 0usize;
            let mut line_no = 1usize;
            for (idx, ch) in src.char_indices() {
                if idx >= span.start {
                    break;
                }
                if ch == '\n' {
                    line_no += 1;
                    line_start = idx + 1;
                }
            }

            let line_end = src[line_start..]
                .find('\n')
                .map(|off| line_start + off)
                .unwrap_or(src.len());
            let line = &src[line_start..line_end];

            let col = span.start.saturating_sub(line_start) + 1;
            let caret_len = (span.end.saturating_sub(span.start)).max(1);

            let mut out = String::new();
            out.push_str(&format!("error: {}\n", diag.message));
            out.push_str(&format!("  --> line {line_no}, col {col}\n"));
            out.push_str("   |\n");
            out.push_str(&format!("{line_no:>3} | {line}\n"));
            out.push_str("   | ");
            for _ in 1..col {
                out.push(' ');
            }
            for _ in 0..caret_len {
                out.push('^');
            }
            out.push('\n');
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_program;
    use crate::ast::Item;
    use crate::validate::validate;

    const HELLO: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/hello.cv"
    ));
    const POISSON: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/poisson.cv"
    ));

    #[test]
    fn parses_and_validates_example() {
        let program = parse_program(HELLO).expect("example should parse");
        validate(&program).expect("example should validate");
    }

    #[test]
    fn parses_and_validates_poisson_example() {
        let program = parse_program(POISSON).expect("example should parse");
        validate(&program).expect("example should validate");
    }

    #[test]
    fn validation_fails_for_unknown_neuron_type() {
        let src = r#"
neuron LIF { tau_m = 10 ms }
layer X[1] : NoSuchNeuron
run for 1 ms
"#;

        let program = parse_program(src).expect("src should parse");
        let diags = validate(&program).expect_err("validation should fail");
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("unknown neuron type"))
        );
    }

    #[test]
    fn parses_seed_and_step() {
        let src = r#"
neuron LIF { tau_m = 10 ms }
layer X[1] : LIF
seed 7
run for 2 ms step 1 ms
"#;
        let program = parse_program(src).expect("parse");
        validate(&program).expect("validate");
        assert!(
            program
                .items
                .iter()
                .any(|item| matches!(item, Item::Seed(_)))
        );
    }
}
