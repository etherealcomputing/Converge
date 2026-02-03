use crate::diagnostic::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub items: Vec<Item>,
}

impl Program {
    pub fn new(items: Vec<Item>) -> Self {
        Self { items }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Neuron(NeuronDef),
    Layer(LayerDef),
    Connect(ConnectDef),
    Run(RunStmt),
}

#[derive(Debug, Clone, PartialEq)]
pub struct NeuronDef {
    pub name: Ident,
    pub body: Vec<Assign>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayerDef {
    pub name: Ident,
    pub size: u64,
    pub neuron: Ident,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConnectDef {
    pub src: Ident,
    pub dst: Ident,
    pub body: Vec<Assign>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunStmt {
    pub duration: Quantity,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assign {
    pub key: Ident,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(Quantity),
    String(String),
    Ident(Ident),
    Call(Call),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub name: Ident,
    pub args: Vec<CallArg>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CallArg {
    Positional(Expr),
    Named { name: Ident, value: Expr },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

impl Ident {
    pub fn new(name: impl Into<String>, span: Span) -> Self {
        Self {
            name: name.into(),
            span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Quantity {
    pub value: f64,
    pub unit: Option<Ident>,
    pub span: Span,
}
