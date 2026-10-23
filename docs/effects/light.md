# Light

Glow, bloom and local contrast: they add or shape the light in the picture.

[All effects](README.md)

| Example | Effect | What it does | Parameters | Recipe |
| --- | --- | --- | --- | --- |
| <img src="../gallery/clarity.jpg" width="160" alt="clarity"> | `clarity` | Local contrast: exaggerate how each area differs from its surroundings, so features stand out even when the whole face sits in a narrow band of brightness. | `radius=30.0` `amount=1.0` `truncate=4.0` `mode='reflect'` `constant_value=0.0` | [clarity.toml](../../examples/effects/clarity.toml) |
| <img src="../gallery/glow.jpg" width="160" alt="glow"> | `glow` | Soft bloom around the bright parts: what is brighter than `threshold` is blurred by `radius` px and screened over the picture by `amount`. | `radius=24.0` `amount=0.6` `threshold=0.55` `truncate=4.0` `mode='reflect'` `constant_value=0.0` | [glow.toml](../../examples/effects/glow.toml) |

### `clarity`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `radius` | number | px | 0 to 500 | `30.0` | Size of each area's 'surroundings': the Gaussian sigma of the blur, in px at the recipe's size. Larger compares bigger regions; 0 turns the effect off. |
| `amount` | number | ratio | 0 to 5 | `1.0` | How strongly each area's difference from its surroundings is exaggerated: 0 = off, 1 = doubled. |
| `truncate` | number | ratio | 1 to 10 | `4.0` | Where the Gaussian is cut off, in sigmas: the blur reaches truncate x radius px. Larger is more exact and slower. |
| `mode` | one of 'reflect', 'constant', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What the blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge), 'constant' (constant_value). |
| `constant_value` | number | fraction | 0 to 1 | `0.0` | The brightness (0..1) assumed beyond the edge when mode is 'constant'; ignored otherwise. |

### `glow`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `radius` | number | px | 0 to 500 | `24.0` | Gaussian sigma of the halo in px at the recipe's size: larger spreads the light further and softer; 0 keeps the light on the bright parts themselves. |
| `amount` | number | ratio | 0 to 10 | `0.6` | How strongly the halo is screened over the picture: 0 = off, 1 = the halo at full brightness; above 1 it saturates to white sooner. |
| `threshold` | number | fraction | 0 to 0.99 | `0.55` | The brightness (0..1, per channel) where the glow starts: only values above it light the halo, rising to full at 1. Raising it limits the glow to the brightest highlights. |
| `truncate` | number | ratio | 1 to 10 | `4.0` | Where the halo's Gaussian is cut off, in sigmas: the halo reaches truncate x radius px. Larger is more exact and slower. |
| `mode` | one of 'reflect', 'constant', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What the halo's blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge), 'constant' (constant_value, so the glow fades toward the edge when it is 0). |
| `constant_value` | number | fraction | 0 to 1 | `0.0` | The highlight (0..1) assumed beyond the edge when mode is 'constant'; ignored otherwise. |
