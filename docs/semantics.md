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

The current simulator implements a simple LIF update. The order is leak, integrate, fire:

```
v = v + (-v) * (dt / tau_m)     // leak the state carried in from the previous step
v = v + incoming                // synaptic charge, then stimulus
if v >= v_th then spike and reset to 0
```

The leak applies to the state carried in from the previous step, never to charge arriving in
the current one. Order matters here. Leaking fresh input in the step it lands means a unit
input can't reach a unit threshold, and the network sits silent no matter how hard you drive
it.

This is a minimal slice. It will evolve as new neuron models land.

