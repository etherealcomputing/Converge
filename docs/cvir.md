# CVIR 0.2

CVIR is the canonical JSON representation emitted by `converge cvir`.

## Top level

```json
{
  "cvir_version": "0.2",
  "items": [ ... ]
}
```

## Items

### Run

```json
{
  "kind": "run",
  "duration": { "value": 10, "unit": "ms" },
  "step": { "value": 1, "unit": "ms" },
  "seed": 0
}
```

### Stimulus

```json
{
  "kind": "stimulus",
  "layer": "Input",
  "model": {
    "type": "poisson",
    "rate": { "value": 50, "unit": "Hz" }
  }
}
```

### Neuron, layer, connect

These items are unchanged from 0.1 and are still emitted with their explicit fields.

