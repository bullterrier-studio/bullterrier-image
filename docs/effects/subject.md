# Subject

Effects that work on the cut-out subject, apart from its background.

[All effects](README.md)

| Example | Effect | What it does | Parameters | Recipe |
| --- | --- | --- | --- | --- |
| <img src="../gallery/drop_shadow.jpg" width="160" alt="drop_shadow"> | `drop_shadow` | A soft shadow of the subject cast on the picture behind it, moved by `dx`, `dy` and softened by `blur`. | `dx=12.0` `dy=18.0` `blur=24.0` `color='#000000'` `opacity=0.6` +2 advanced | [drop_shadow.toml](../../examples/effects/drop_shadow.toml) |
| <img src="../gallery/fill.jpg" width="160" alt="fill"> | `fill` | Replace the whole picture with one colour. | `color='#000000'` | [fill.toml](../../examples/effects/fill.toml) |
| <img src="../gallery/layered_cutouts.jpg" width="160" alt="layered_cutouts"> | `layered_cutouts` | Copies of the subject's silhouette stacked behind it, each `offset` px farther along `angle`, like cut paper. | `layers=4` `offset=14.0` `angle=45.0` `color='#ff8a5b'` `end_color='#3b1f6b'` `opacity=1.0` +1 advanced | [layered_cutouts.toml](../../examples/effects/layered_cutouts.toml) |
| <img src="../gallery/long_shadow.jpg" width="160" alt="long_shadow"> | `long_shadow` | A flat-design long shadow: the subject's silhouette repeated along `angle` for `length` px, behind it. | `angle=45.0` `length=300.0` `color='#000000'` `opacity=0.5` `fade=1.0` +1 advanced | [long_shadow.toml](../../examples/effects/long_shadow.toml) |
| <img src="../gallery/outline.jpg" width="160" alt="outline"> | `outline` | A coloured stroke of `width` px around the subject's silhouette, with an optional soft halo of `glow` px. | `width=6.0` `color='#ffffff'` `position='outside'` `glow=0.0` `opacity=1.0` +1 advanced | [outline.toml](../../examples/effects/outline.toml) |
| <img src="../gallery/reflection.jpg" width="160" alt="reflection"> | `reflection` | A floor mirror: the picture above the subject's lowest row is mirrored about it, softened and fading with depth. | `height=0.4` `fade=0.7` `blur=2.0` `opacity=0.6` +3 advanced | [reflection.toml](../../examples/effects/reflection.toml) |
| <img src="../gallery/rim_light.jpg" width="160" alt="rim_light"> | `rim_light` | Light along the subject's edge from one side: a band of `width` px inside the edge, tinted `color`. | `angle=40.0` `color='#ff9a4d'` `width=12.0` `amount=1.0` `falloff=1.5` +4 advanced | [rim_light.toml](../../examples/effects/rim_light.toml) |
| <img src="../gallery/spotlight.jpg" width="160" alt="spotlight"> | `spotlight` | Light the subject and darken the rest: the further from the silhouette, the more the picture goes to `color`. | `amount=0.8` `falloff=0.8` `radius=200.0` `feather=10.0` `color='#000000'` +3 advanced | [spotlight.toml](../../examples/effects/spotlight.toml) |

### `drop_shadow`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `dx` | number | px | -500 to 500 | `12.0` | How far the shadow is moved to the right, in px at the recipe's size; negative moves it left. 0 with `dy` 0 hides the shadow behind the subject, leaving only what the blur spreads past its edge. |
| `dy` | number | px | -500 to 500 | `18.0` | How far the shadow is moved down, in px at the recipe's size; negative moves it up. |
| `blur` | number | px | 0 to 200 | `24.0` | Gaussian sigma of the shadow's soft edge in px: raising it makes the shadow larger and fainter at its edge; 0 = a hard-edged copy of the subject's silhouette. |
| `color` | colour |  |  | `'#000000'` | The colour of the shadow: '#rrggbb', '#rgb' or [r, g, b] 0..1. Black darkens what lies behind the subject; another colour tints it. |
| `opacity` | number | fraction | 0 to 1 | `0.6` | How much of the picture behind the subject the shadow replaces with its colour where it is fullest: 0 = no shadow, 1 = the picture is fully covered there. |

<details><summary>Advanced parameters (2)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `truncate` | number | ratio | 1 to 10 | `4.0` | Where the shadow's Gaussian is cut off, in sigmas: it reaches truncate x blur px. Larger is more exact and slower. |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'nearest'` | What the shadow sees beyond the picture's edge, when it is moved and softened: 'nearest' (the outermost row repeated, so a subject cut off by the border casts its shadow on without a gap), 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel) or 'wrap' (the opposite edge). |

</details>

### `fill`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `color` | colour |  |  | `'#000000'` | The colour that fills the whole picture: '#rrggbb', '#rgb' or [r, g, b] 0..1. |

### `layered_cutouts`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `layers` | whole number | count | 1 to 20 | `4` | How many copies of the silhouette are stacked behind the subject. Raising it makes a deeper stack. |
| `offset` | number | px | 0 to 200 | `14.0` | How far each layer lies beyond the one before it, in px at the recipe's size: the first is `offset` from the subject, the last `layers` x `offset`. 0 = every layer hidden behind the subject. |
| `angle` | number | degrees | -180 to 180 | `45.0` | The direction the layers step away in, clockwise as seen in the picture: 0 = to the right, 45 = to the lower right, 90 = down, 180 = to the left, -90 = up. |
| `color` | colour |  |  | `'#ff8a5b'` | The colour of the layer nearest the subject: '#rrggbb', '#rgb' or [r, g, b] 0..1. |
| `end_color` | colour |  |  | `'#3b1f6b'` | The colour of the farthest layer; the layers between step evenly from `color` to it. With one layer only `color` is used. |
| `opacity` | number | fraction | 0 to 1 | `1.0` | How much of what lies behind each layer it covers: 1 = fully (paper), below 1 the layers show through each other and the picture; 0 = no layers. |

<details><summary>Advanced parameters (1)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'nearest'` | What the layers see beyond the picture's edge when the silhouette is moved: 'nearest' (the outermost row repeated, so a subject cut off by the border has layers that go on past it), 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel) or 'wrap' (the opposite edge). |

</details>

### `long_shadow`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `angle` | number | degrees | -180 to 180 | `45.0` | The direction the shadow is cast, clockwise as seen in the picture: 0 = to the right, 45 = to the lower right, 90 = down, 180 = to the left, -90 = up. Angles that are not a multiple of 45 leave the shadow's edges about a pixel soft. |
| `length` | number | px | 0 to 5000 | `300.0` | How far the shadow reaches from the subject's outline, in px at the recipe's size; 0 = no shadow (the silhouette behind itself, which shows only as a faint fringe where the cut-out is soft). Raising it makes the shadow longer; the effect's time grows only with the logarithm of it. |
| `color` | colour |  |  | `'#000000'` | The colour of the shadow: '#rrggbb', '#rgb' or [r, g, b] 0..1. Black darkens what lies behind the subject; another colour tints it. |
| `opacity` | number | fraction | 0 to 1 | `0.5` | How much of the picture behind the subject the shadow replaces with its colour where it is fullest, at the subject: 0 = no shadow, 1 = the picture is fully covered there. |
| `fade` | number | fraction | 0 to 1 | `1.0` | How much of its opacity the shadow loses along its length, in a straight line: 0 = solid to the end, 0.5 = half as opaque at the end, 1 = fades out completely at `length`. |

<details><summary>Advanced parameters (1)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'nearest'` | What the shadow sees beyond the picture's edge when the silhouette is moved: 'nearest' (the outermost row repeated, so a subject cut off by the border casts its shadow on without a gap), 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel) or 'wrap' (the opposite edge). |

</details>

### `outline`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `width` | number | px | 0 to 200 | `6.0` | Thickness of the stroke in px at the recipe's size; raising it thickens the line. 0 = no stroke. |
| `color` | colour |  |  | `'#ffffff'` | The colour of the stroke and of its halo: '#rrggbb', '#rgb' or [r, g, b] 0..1. |
| `position` | one of 'outside', 'inside', 'center' |  |  | `'outside'` | Which side of the silhouette's edge the stroke lies on: 'outside' (around the subject, whose own pixels stay untouched), 'inside' (painted over the subject's rim) or 'center' (half of `width` on each side). |
| `glow` | number | px | 0 to 200 | `0.0` | How far a soft halo of the same colour reaches beyond the stroke, in px; it fades smoothly from the stroke to nothing. Raising it spreads the halo (a large value makes an aura). 0 = no halo. |
| `opacity` | number | fraction | 0 to 1 | `1.0` | How solid the stroke and halo are: 0 = invisible, 1 = fully covering the picture. |

<details><summary>Advanced parameters (1)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `matte_threshold` | number | fraction | 0 to 1 | `0.5` | The matte value, 0 to 1, from which a pixel belongs to the subject whose edge is outlined: 0.5 = more than half covered. Lowering it takes in more of a soft edge (hair, fur), raising it less. |

</details>

### `reflection`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `height` | number | fraction | 0 to 1 | `0.4` | How tall the reflection is, as a fraction of the subject's height: 0.4 = 40% of it is mirrored into the floor below the subject's lowest row, 1 = all of it; 0 = no reflection. Where the picture ends first, the reflection is cut there. |
| `fade` | number | fraction | 0 to 1 | `0.7` | How much of its strength the reflection loses between the floor line and its end, in a straight line: 0 = as strong at the end as at the floor line, 1 = fades out completely at its end. |
| `blur` | number | px | 0 to 100 | `2.0` | Gaussian sigma of the reflection's softness in px at the recipe's size: raising it makes a blurrier mirror, as on a rough floor; 0 = a sharp mirror image. |
| `opacity` | number | fraction | 0 to 1 | `0.6` | How much of the floor the reflection replaces at the floor line, where it is strongest: 0 = no reflection, 1 = a perfect mirror. |

<details><summary>Advanced parameters (3)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `threshold` | number | fraction | 0 to 1 | `0.5` | How much of a matte row, 0 to 1, must be subject for the row to count as part of it when its lowest and highest rows are found: 0.5 = a pixel of at least half. Lowering it counts faint fringes, so the floor line moves down and the subject's height grows. |
| `truncate` | number | ratio | 1 to 10 | `4.0` | Where the blur's Gaussian is cut off, in sigmas: it reaches truncate x blur px. Larger is more exact and slower. |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What the blur sees beyond the picture's edge, and beyond the floor line, where the reflection is mirrored: 'reflect' (mirrored, edge pixel repeated, which continues the mirror image), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated) or 'wrap' (the opposite edge). |

</details>

### `rim_light`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `angle` | number | degrees | -180 to 180 | `40.0` | The direction the light comes from, counter-clockwise from the right: 0 = from the right, 90 = from above, 180 or -180 = from the left, -90 = from below. Only edges whose outward direction faces it are lit. |
| `color` | colour |  |  | `'#ff9a4d'` | The colour of the light: '#rrggbb', '#rgb' or [r, g, b] 0..1. The default is a warm orange. |
| `width` | number | px | 1 to 200 | `12.0` | Depth of the lit band inside the subject's edge, in px at the recipe's size: raising it lets the light reach further in. |
| `amount` | number | ratio | 0 to 5 | `1.0` | How strong the light is where it is brightest (an edge that faces the light head-on): 0 = off, 1 = the colour at full strength, above 1 saturates sooner. |
| `falloff` | number | ratio | 0.1 to 10 | `1.5` | How the light fades from the edge into the band: it is (1 - depth / width)^falloff of its strength. 1 = evenly, above 1 stays close to the edge, below 1 carries further in. |

<details><summary>Advanced parameters (4)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `smoothing` | number | px | 0 to 50 | `2.0` | Gaussian sigma, in px, with which the silhouette is smoothed before the edge's direction is read: raising it evens out the jaggedness of a pixel edge (fur), at the cost of detail in small features. 0 uses the raw edge. |
| `truncate` | number | ratio | 1 to 10 | `4.0` | Where the `smoothing` blur is cut off, in sigmas: it reaches truncate x smoothing px. Larger is more exact and slower. |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What the `smoothing` blur sees beyond the picture's border: 'reflect' (mirrored, border pixel repeated), 'mirror' (mirrored about the border pixel), 'nearest' (the border pixel repeated) or 'wrap' (the opposite border). |
| `matte_threshold` | number | fraction | 0 to 1 | `0.5` | The matte value, 0 to 1, from which a pixel belongs to the subject whose edge is lit: 0.5 = more than half covered. Lowering it takes in more of a soft edge (hair, fur), raising it less. |

</details>

### `spotlight`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `amount` | number | fraction | 0 to 1 | `0.8` | How far the picture is darkened where it is darkest, far from the subject: 0 = unchanged, 1 = fully `color`. |
| `falloff` | number | ratio | 0.1 to 10 | `0.8` | How fast the darkening grows with distance from the subject: it is (distance / radius)^falloff of its full strength. 1 = evenly, below 1 darkens quickly near the subject, above 1 stays light near it and darkens late. |
| `radius` | number | px | 1 to 5000 | `200.0` | The distance from the subject's silhouette, in px at the recipe's size, at which the darkening reaches full strength; beyond it the picture stays that dark. Raising it spreads the light further. |
| `feather` | number | px | 0 to 200 | `10.0` | How softly the light spills from the subject, as a Gaussian sigma in px: the area around the silhouette stays lit through this much of its surroundings. Raising it makes a softer pool of light round the subject; 0 = the darkening starts right at the silhouette. |
| `color` | colour |  |  | `'#000000'` | The colour the surroundings darken toward: '#rrggbb', '#rgb' or [r, g, b] 0..1. Black darkens; a dark blue gives a night look. |

<details><summary>Advanced parameters (3)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `truncate` | number | ratio | 1 to 10 | `4.0` | Where the `feather` blur is cut off, in sigmas: it reaches truncate x feather px. Larger is more exact and slower. |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What the `feather` blur sees beyond the picture's border: 'reflect' (mirrored, border pixel repeated), 'mirror' (mirrored about the border pixel), 'nearest' (the border pixel repeated) or 'wrap' (the opposite border). |
| `matte_threshold` | number | fraction | 0 to 1 | `0.5` | The matte value, 0 to 1, from which a pixel belongs to the subject that stays lit: 0.5 = more than half covered. Lowering it takes in more of a soft edge (hair, fur), raising it less. |

</details>
