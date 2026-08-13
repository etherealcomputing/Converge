# Converge time semantics

Converge is a time first language. The simulator follows a clocked model in the current slice.

## Execution model

1. Time advances in fixed steps `dt`.
2. Events are delivered into a time bucket queue indexed by step.
3. Within a step, layers are processed in source order and neurons by index.
4. Spike delivery uses connection delays measured in steps.

## Delays

1. Delays are measured in whole steps.
2. A sampled delay that doesn't land on the step grid is rounded to the nearest step. The
   simulator reports how many synapses it moved and by how much, under `delay_quantization`
   in the run summary. Rejecting off grid delays outright would make `Normal` and `Uniform`
   delay distributions unusable, and moving a time without saying so is worse than either.
3. One step is the floor. A spike can't be delivered in the step it was emitted, so a zero
   delay becomes one step. Floored synapses are counted separately from rounded ones.

## Step size

`decay` is `step / tau_m`. Past 1.0 the membrane flips sign every step, and at exactly 2.0 it
negates. A run whose step exceeds `tau_m` for any layer is refused rather than clamped.

## Determinism

Determinism is enforced by design:

1. The RNG is seeded from `seed` and is only used in defined places.
2. Ordering is stable and documented.
3. Unit conversion is explicit and rounded to integer nanoseconds.

The generator is SplitMix64. Every draw site takes its own stream keyed off `seed`, so adding
a draw site somewhere can't shift the numbers at another one. Samplers that take a log draw
from an open interval, so `ln(0)` can't reach a weight or a delay.

One limit worth naming: `Normal` uses Box-Muller, which calls `ln`, `sqrt` and `cos`. Those
aren't guaranteed bit identical across libm versions, so a program using `Normal` can differ
between platforms. Everything else is integer or exactly rounded. CI runs Linux only, so
treat cross platform bit equality as unverified rather than guaranteed.

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

