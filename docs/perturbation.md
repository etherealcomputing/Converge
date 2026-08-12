# Perturbation and the failure envelope

Neuromorphic systems fail dynamically. A network whose excitation and inhibition drift apart
doesn't crash and it doesn't corrupt memory. It saturates, or it goes quiet, and it keeps
answering the whole time. Memory safety fuzzing looks for the wrong class of bug here.

So the question isn't whether a network infers. It's how far you can push it before it stops.
`converge perturb` sweeps a fault operator across its range, checks invariants at each point
against an unperturbed baseline, and reports the severity at first violation. That number is
the failure envelope. It's a margin you can engineer against, which a pass/fail bit isn't.

This is defensive. It perturbs Converge's own elaborated networks. It doesn't ship offense
code and it has no business anywhere near somebody else's system.

## Why this belongs in Converge

Determinism. A fault sweep is only evidence if you can rerun it and get the same answer, and
the same seed has to mean the same numbers on every point of the grid. That's difficult
against a stochastic Python SNN library and straightforward against a simulator whose output
is a function of its seed.

## Running it

```bash
converge perturb examples/envelope.cv
converge perturb examples/envelope.cv --operators prune,gain.attenuate
converge perturb examples/envelope.cv --steps 40 --bisect 8 --out envelope.json
```

| Flag | Default | Meaning |
|---|---|---|
| `--out <path>` | stdout | Write the JSON somewhere instead of printing it |
| `--steps <n>` | 20 | Grid points across the severity range |
| `--bisect <n>` | 6 | Bisection rounds used to tighten each bound |
| `--operators <list>` | all | Comma separated subset, swept in the order given |

## Operators

Every operator is a scalar knob on the dynamics, with severity running `0.0` to `1.0` and
`0.0` always the identity. One axis for all of them is what makes one operator's envelope
comparable to another's. Two mechanisms are two sided, so they get one entry per direction
rather than a signed axis.

| Operator | Knob | Failure mode |
|---|---|---|
| `ei_scale.disinhibit` | inhibitory weights scaled by `1 - m` | runaway excitation |
| `ei_scale.deexcite` | excitatory weights scaled by `1 - m` | silent network |
| `spike_jitter` | delays offset by a normal with sigma `m * mean delay` | desynchronization, timing collapse |
| `gain.attenuate` | stimulus amplitude scaled by `1 - m` | input ignored |
| `gain.amplify` | stimulus amplitude scaled from 1x to 10x | output that confirms itself regardless of input |
| `prune` | fraction `m` of synapses removed | graceful degradation versus a cliff edge |

Operators are named for the mechanism they move. If a perturbation can't be written as a
scalar on an equation, it isn't one of these.

Jitter respects the same delivery rules the simulator does: offsets land on the step grid and
nothing drops below the one step floor. Delays are capped at the run length, since a delay
longer than the run never arrives.

## Common random numbers

Per-synapse ablation keys and timing offsets are drawn once, from streams keyed off the seed,
and reused at every severity. This is the difference between an envelope that measures the
network and one that measures sampling noise.

Two things fall out of it. Prune sets are nested: a synapse cut at `m` is still cut at any
larger `m`, so severity is the only thing separating one grid point from the next. And jitter
is one fixed pattern scaled by severity rather than a fresh pattern each time. Redrawing at
each grid point would make the sweep wander for reasons that have nothing to do with
fragility.

## Invariants

Each one is checked against the baseline run and reports a measured scalar next to its
verdict. A boolean tells you the run broke; the scalar tells you how close the last good one
was.

| Invariant | Holds when |
|---|---|
| `finite` | no membrane value left the reals |
| `alive` | every layer that spiked at baseline still spikes |
| `bounded` | no layer's peak window rate went above 4x its baseline peak |
| `rate_band` | total spikes stayed within 0.25x to 4x the baseline total |
| `sustained` | a layer active in every baseline window didn't drop a window entirely |

`bounded` and `sustained` read the per-window spike counts the simulator records, sixteen
equal windows across the run. Totals alone can't separate a steady rate from a long silence
with one burst at the end, and that difference is most of what stability means here.

An invariant the baseline can't exercise reports `not_applicable`, not `held`. A network that
never spikes can't be shown to have gone silent. A vacuous pass and a real one read
identically otherwise, and only one of them is evidence.

## Reading the output

```json
{
  "operator": "gain.attenuate",
  "invariants": [
    { "invariant": "rate_band", "status": "violated", "bound": 0.687500, "measured": 0.186151 },
    { "invariant": "sustained", "status": "violated", "bound": 0.796875, "measured": 9.000000 },
    { "invariant": "alive",     "status": "violated", "bound": 0.921875, "measured": 2.000000 }
  ]
}
```

Read that as: cut the drive by 69% and the output rate leaves its band. Cut it by 80% and
layers start dropping whole windows. Cut it by 92% and a layer goes silent outright. The
ordering is the useful part, because it says which way this network degrades first.

`status` is one of `held`, `violated` or `not_applicable`. `bound` is the severity at first
violation, tightened by bisection between the grid points that straddle it.

The sweep keeps going past the first violation instead of stopping, so an invariant that
breaks and then recovers stays visible in the samples.

## Limits worth knowing

The bound is only as good as the grid. Bisection tightens a bracket the grid already found; a
cliff that sits entirely inside one grid interval, with both ends holding, stays invisible.
`grid_steps` is reported in the output so the bound gets read as the bound it is. Raise
`--steps` when you care about the exact edge.

An operator whose envelope never moves is telling you something about the network, not
necessarily about itself. `spike_jitter` holding across the whole range on a rate coded
feedforward network is the correct answer, not a broken operator. Check the `samples` array
before concluding a knob does nothing.
