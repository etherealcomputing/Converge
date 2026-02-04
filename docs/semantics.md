# Converge time semantics

Converge is a time first language. The simulator follows a clocked model in the current slice.

## Execution model

1. Time advances in fixed steps `dt`.
2. Events are delivered into a time bucket queue indexed by step.
3. Within a step, layers are processed in source order and neurons by index.
4. Spike delivery uses connection delays measured in steps.

## Determinism

Determinism is enforced by design:

1. The RNG is seeded from `seed` and is only used in defined places.
2. Ordering is stable and documented.
3. Unit conversion is explicit and rounded to integer nanoseconds.

## LIF update rule

The current simulator implements a simple LIF update:

```
v = v + incoming
v = v + (-v) * (dt / tau_m)
if v >= v_th then spike and reset to 0
```

This is a minimal slice. It will evolve as new neuron models land.

