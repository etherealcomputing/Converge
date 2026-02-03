use crate::diagnostic::{Diagnostic, Span};

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Ident(String),
    Number(String),
    String(String),

    KwNeuron,
    KwLayer,
    KwConnect,
    KwRun,
    KwFor,

    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LParen,
    RParen,
    Colon,
    Comma,
    Eq,
    Arrow,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

pub fn lex(input: &str) -> Result<Vec<Token>, Diagnostic> {
    let mut lexer = Lexer::new(input);
    let mut tokens = Vec::new();
    while let Some(tok) = lexer.next_token()? {
        tokens.push(tok);
    }
    Ok(tokens)
}

struct Lexer<'a> {
    input: &'a str,
    bytes: &'a [u8],
    i: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            bytes: input.as_bytes(),
            i: 0,
        }
    }

    fn next_token(&mut self) -> Result<Option<Token>, Diagnostic> {
        self.skip_ws_and_comments();
        if self.i >= self.bytes.len() {
            return Ok(None);
        }

        let start = self.i;
        let b = self.bytes[self.i];
        let tok = match b {
            b'{' => {
                self.i += 1;
                TokenKind::LBrace
            }
            b'}' => {
                self.i += 1;
                TokenKind::RBrace
            }
            b'[' => {
                self.i += 1;
                TokenKind::LBracket
            }
            b']' => {
                self.i += 1;
                TokenKind::RBracket
            }
            b'(' => {
                self.i += 1;
                TokenKind::LParen
            }
            b')' => {
                self.i += 1;
                TokenKind::RParen
            }
            b':' => {
                self.i += 1;
                TokenKind::Colon
            }
            b',' => {
                self.i += 1;
                TokenKind::Comma
            }
            b'=' => {
                self.i += 1;
                TokenKind::Eq
            }
            b'-' if self.peek_is(b'>') => {
                self.i += 2;
                TokenKind::Arrow
            }
            b'"' => return self.lex_string(start).map(Some),
            b'0'..=b'9' | b'-' => return self.lex_number_or_ident().map(Some),
            _ => {
                if is_ident_start(b) {
                    return self.lex_ident().map(Some);
                }
                return Err(Diagnostic::new(format!(
                    "unexpected character '{}'",
                    self.input[self.i..].chars().next().unwrap_or('\u{FFFD}')
                ))
                .with_span(Span::new(self.i, self.i + 1)));
            }
        };

        Ok(Some(Token {
            kind: tok,
            span: Span::new(start, self.i),
        }))
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            while self.i < self.bytes.len() {
                let b = self.bytes[self.i];
                if b == b' ' || b == b'\n' || b == b'\r' || b == b'\t' {
                    self.i += 1;
                } else {
                    break;
                }
            }
            if self.i + 1 < self.bytes.len() && self.bytes[self.i] == b'/' && self.bytes[self.i + 1] == b'/' {
                self.i += 2;
                while self.i < self.bytes.len() && self.bytes[self.i] != b'\n' {
                    self.i += 1;
                }
                continue;
            }
            break;
        }
    }

    fn peek_is(&self, b: u8) -> bool {
        self.i + 1 < self.bytes.len() && self.bytes[self.i + 1] == b
    }

    fn lex_string(&mut self, start: usize) -> Result<Token, Diagnostic> {
        debug_assert_eq!(self.bytes[self.i], b'"');
        self.i += 1;
        let mut s = String::new();
        while self.i < self.bytes.len() {
            match self.bytes[self.i] {
                b'"' => {
                    self.i += 1;
                    return Ok(Token {
                        kind: TokenKind::String(s),
                        span: Span::new(start, self.i),
                    });
                }
                b'\\' => {
                    self.i += 1;
                    if self.i >= self.bytes.len() {
                        break;
                    }
                    let esc = self.bytes[self.i];
                    self.i += 1;
                    match esc {
                        b'"' => s.push('"'),
                        b'\\' => s.push('\\'),
                        b'n' => s.push('\n'),
                        b'r' => s.push('\r'),
                        b't' => s.push('\t'),
                        _ => {
                            return Err(Diagnostic::new("invalid string escape").with_span(Span::new(
                                self.i.saturating_sub(2),
                                self.i,
                            )));
                        }
                    }
                }
                _ => {
                    let ch = self.input[self.i..].chars().next().unwrap_or('\u{FFFD}');
                    self.i += ch.len_utf8();
                    s.push(ch);
                }
            }
        }
        Err(Diagnostic::new("unterminated string").with_span(Span::new(start, self.i)))
    }

    fn lex_ident(&mut self) -> Result<Token, Diagnostic> {
        let start = self.i;
        self.i += 1;
        while self.i < self.bytes.len() && is_ident_continue(self.bytes[self.i]) {
            self.i += 1;
        }
        let text = &self.input[start..self.i];
        let kind = match text {
            "neuron" => TokenKind::KwNeuron,
            "layer" => TokenKind::KwLayer,
            "connect" => TokenKind::KwConnect,
            "run" => TokenKind::KwRun,
            "for" => TokenKind::KwFor,
            _ => TokenKind::Ident(text.to_string()),
        };
        Ok(Token {
            kind,
            span: Span::new(start, self.i),
        })
    }

    fn lex_number_or_ident(&mut self) -> Result<Token, Diagnostic> {
        let start = self.i;
        if self.bytes[self.i] == b'-' {
            self.i += 1;
            if self.i >= self.bytes.len() || !self.bytes[self.i].is_ascii_digit() {
                // It's just '-' and not a number; let caller handle as unexpected.
                return Err(Diagnostic::new("unexpected '-'").with_span(Span::new(start, self.i)));
            }
        }
        while self.i < self.bytes.len() && self.bytes[self.i].is_ascii_digit() {
            self.i += 1;
        }
        if self.i < self.bytes.len() && self.bytes[self.i] == b'.' {
            self.i += 1;
            while self.i < self.bytes.len() && self.bytes[self.i].is_ascii_digit() {
                self.i += 1;
            }
        }
        let text = self.input[start..self.i].to_string();
        Ok(Token {
            kind: TokenKind::Number(text),
            span: Span::new(start, self.i),
        })
    }
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_continue(b: u8) -> bool {
    is_ident_start(b) || b.is_ascii_digit()
}

