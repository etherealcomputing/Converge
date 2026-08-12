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

Units are parsed and checked. A unit belongs to exactly one kind and kinds don't convert into
each other.

| Kind | Units | Canonical base |
|---|---|---|
| time | `s`, `ms`, `us`, `ns` | integer nanoseconds |
| rate | `Hz`, `kHz` | Hz |
| voltage | `V`, `mV`, `uV` | volts |

Time and rate positions require a unit. Voltage positions don't: the volt is the canonical
membrane unit, so a bare number in a voltage position is volts. That's why `v_th = 1000 mV`
and `v_th = 1.0 V` are the same threshold, and why an unmarked weight doesn't change meaning
depending on how the threshold next to it was spelled.

Known gap: `w` is added straight into the membrane, so a full dimensional check between a
weight and a threshold would need a capacitance or a current in the neuron model. Both are
checked as voltages today and neither is checked against the other.

## Grammar (subset)

EBNF-ish notation:

```
program      = { item } ;

item         = neuron_def
             | layer_def
             | connect_def
             | stimulus_def
             | run_stmt
             | seed_stmt ;

neuron_def   = "neuron" ident "{" { assign ["," ] } "}" ;
layer_def    = "layer" ident "[" int "]" ":" ident ;
connect_def  = "connect" ident "->" ident "{" { assign ["," ] } "}" ;
run_stmt     = "run" "for" quantity [ "step" quantity ] ;
seed_stmt    = "seed" int ;
stimulus_def = "stimulus" ident "=" stimulus_model ;
stimulus_model = "Poisson" "(" "rate" "=" quantity ")" ;

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
- At most one `seed` statement.
- Exactly one `run` statement.
- Every `layer ... : NeuronType` refers to a defined `neuron`.
- Every `connect A -> B` refers to defined `layer`s.
- Every `stimulus L = ...` refers to a defined `layer`.
- `run` duration and step must use time units and must be positive.
- `stimulus` rate must use frequency units.
- `neuron` bodies accept `tau_m` and `v_th`. Any other key is an error.
- `connect` bodies accept `w` and `d`. Any other key is an error.
- `tau_m` and connection delay `d` must use time units.
- `v_th` and connection weight `w` must use voltage units, or no unit at all.
- `w` and `d` may be a `Normal` or `Uniform` distribution. Every argument is checked against
  the same kind as the key it belongs to.

Defaults:

- `run` step defaults to `1 ms` when omitted
- `seed` defaults to `0` when omitted

## Canonical IR (CVIR)

`converge cvir <file>` emits a stable JSON representation of the parsed program (spans omitted).
This is intentionally a stepping stone toward a future NIR-aligned interchange pipeline.

## Docs

1. `docs/semantics.md` time model and determinism rules
2. `docs/cvir.md` canonical IR schema and examples
3. `docs/voice.md` writing rules for project docs
4. `docs/brand.md` logo and asset guidance
