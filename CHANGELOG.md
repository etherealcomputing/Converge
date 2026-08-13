# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog and this project adheres to Semantic Versioning.

## Unreleased

### Added

- `converge-perturb` crate: four fault operators (`ei_scale`, `spike_jitter`, `gain`, `prune`)
  on a uniform severity axis, five invariants checked against an unperturbed baseline, and a
  sweep that reports the severity at first violation
- `converge perturb` command with `--out`, `--steps`, `--bisect` and `--operators`
- `docs/perturbation.md`
- `examples/envelope.cv`, a network with sustained activity in both layers for fault sweeps
- Public `elaborate` and `run` in `converge-sim`, so a pass can perturb a network between them
- Per-layer spike counts bucketed into 16 windows, and a non-finite membrane counter
- Voltage as a unit kind (`V`, `mV`, `uV`) with the volt as the canonical membrane unit
- Golden tests pinning CVIR output byte for byte

### Fixed

- LIF update applied the leak to charge arriving in the same step, so a unit input could never
  reach a unit threshold. `examples/poisson.cv` simulated to zero spikes
- Zero-delay synapses were delivered `queue_len` steps late instead of one, and `queue_len`
  depends on the longest delay anywhere in the network. Delays now floor at one step
- Off-grid delays were a hard error, which made `Normal` and `Uniform` delay distributions
  unusable and left `examples/hello.cv` unsimulatable. They round to the nearest step and the
  coercion is reported in the summary
- `v_th` read its unit and discarded it, so `1.0 V` and `1.0 mV` were the same threshold
- `tau_m` being a time and `w` being well formed were unchecked in the front end
- Unknown `neuron` and `connect` body keys were silently ignored
- The RNG stored the seed as its state, so the default `seed 0` produced exactly 0.0 as its
  first draw. Replaced with SplitMix64 and per-site streams
- Box-Muller could take `ln(0)`, putting an infinity into a weight or a delay
- A run whose step exceeds `tau_m` is refused instead of flipping the membrane sign each step

### Changed

- Simulator spike counts moved once, from the LIF ordering and RNG fixes above, and are now
  pinned by tests
- `SimSummary` gained `windows`, `nonfinite` and `delay_quantization`. CVIR is unchanged

## 0.1.0

### Added

- Converge Rust workspace with `converge-lang` and `converge-cli`
- Lexer and parser for a small Converge subset
- Basic semantic validation for neurons layers and connections
- `converge check` and `converge ast` developer commands
- `converge cvir` canonical JSON IR emitter
- Example programs in `examples/`
