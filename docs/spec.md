# Converge language spec (0.1 / pre‑α)

This spec intentionally describes **what the current compiler front‑end accepts** (parser + validator), not the final vision.

> Converge is designed for neuromorphic–classical hybrids, but the host language and hardware backends are not implemented yet.

## Lexical structure

- **Whitespace**: spaces/newlines/tabs separate tokens.
- **Line comments**: `// ...` to end-of-line.
- **Identifiers**: `[A-Za-z_][A-Za-z0-9_]*`
- **Strings**: `"..."`
  - Supported escapes: `\"`, `\\`, `\n`, `\r`, `\t`
- **Numbers**: decimal integers and floats, with optional leading `-`.

## Units (syntax)

A number may be followed by an identifier interpreted as a **unit token**:

```converge
tau_m = 20 ms
v_th  = 1.0 V
run for 10 ms
```

Units are currently *parsed* and carried into IR, but not yet dimension-checked.

## Grammar (subset)

EBNF-ish notation:

```
program      = { item } ;

item         = neuron_def
             | layer_def
             | connect_def
             | run_stmt ;

neuron_def   = "neuron" ident "{" { assign ["," ] } "}" ;
layer_def    = "layer" ident "[" int "]" ":" ident ;
connect_def  = "connect" ident "->" ident "{" { assign ["," ] } "}" ;
run_stmt     = "run" "for" quantity ;

assign       = ident "=" expr ;

expr         = quantity
             | string
             | ident
             | call ;

call         = ident "(" [ call_arg { "," call_arg } ["," ] ] ")" ;
call_arg     = expr
             | ident "=" expr ;

quantity     = number [ ident ] ;
```

## Validation rules (current)

The `check` command enforces:

- Neuron definitions are unique by name.
- Layer definitions are unique by name.
- Every `layer ... : NeuronType` refers to a defined `neuron`.
- Every `connect A -> B` refers to defined `layer`s.

## Canonical IR (CVIR)

`converge cvir <file>` emits a stable JSON representation of the parsed program (spans omitted).
This is intentionally a stepping stone toward a future NIR-aligned interchange pipeline.

