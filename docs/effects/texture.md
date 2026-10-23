# Texture

Grain and other surface texture laid over the picture.

[All effects](README.md)

| Example | Effect | What it does | Parameters | Recipe |
| --- | --- | --- | --- | --- |
| <img src="../gallery/grain.jpg" width="160" alt="grain"> | `grain` | Film grain: brightness noise of standard deviation `amount` (0.08 visible, 0.2 heavy), clumped to `size` px. | `amount=0.08` `size=1.0` `seed=0` `truncate=4.0` `mode='reflect'` | [grain.toml](../../examples/effects/grain.toml) |

### `grain`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `amount` | number | fraction | 0 to 1 | `0.08` | Standard deviation of the added brightness, in 0..1 picture units: 0 = none, 0.08 is visible, 0.2 is heavy. |
| `size` | number | px | 0 to 50 | `1.0` | Grain size in px: up to 1 every pixel varies on its own; above 1 the noise is blurred (Gaussian sigma = size / 2) into specks about this wide, keeping its standard deviation. Larger = coarser, softer grain. |
| `seed` | whole number | count | 0 to 4294967295 | `0` | Random seed: the same seed always gives the same grain, another seed a different pattern. |
| `truncate` | number | ratio | 1 to 10 | `4.0` | Where the clumping blur's Gaussian is cut off, in sigmas (only when size > 1). Larger is more exact and slower. |
| `mode` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What the clumping blur sees beyond the picture's edge (only when size > 1): 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge, so the grain tiles seamlessly). |
