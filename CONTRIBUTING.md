# Contributing to Converge

Thanks for helping. Converge is pre alpha, so small sharp changes win.

## Prerequisites

1. Rust stable installed
2. Minimum supported Rust is 1.92

## Build and test

```bash
cargo test
cargo run -p converge-cli -- check examples/hello.cv
```

## Formatting and linting

```bash
cargo fmt --all
cargo clippy --workspace --all-targets
```

CI enforces `cargo fmt --check` and `cargo clippy -D warnings` so keep it clean.

## How to add a syntax feature

Touchpoints are intentionally simple.

1. Add tokens if needed in `crates/converge-lang/src/lexer.rs`
2. Extend AST in `crates/converge-lang/src/ast.rs`
3. Parse it in `crates/converge-lang/src/parser.rs`
4. Validate semantics in `crates/converge-lang/src/validate.rs`
5. Emit stable IR in `crates/converge-lang/src/emit.rs`
6. Add an example in `examples/` and add or extend tests in `crates/converge-lang/src/parser.rs`

## What we care about

1. Deterministic behavior
2. Clear diagnostics
3. Stable IR output
4. Honest docs

