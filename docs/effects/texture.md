# Texture

Grain and other surface texture laid over the picture.

[All effects](README.md)

| Example | Effect | What it does | Parameters | Recipe |
| --- | --- | --- | --- | --- |
| <img src="../gallery/dust_scratches.jpg" width="160" alt="dust_scratches"> | `dust_scratches` | Draw light and dark specks, hairs and scratches at random places over the picture. | `density=0.6` `size=8.0` `scratches=2` `light='#ffffff'` `dark='#000000'` `seed=0` +3 advanced | [dust_scratches.toml](../../examples/effects/dust_scratches.toml) |
| <img src="../gallery/grain.jpg" width="160" alt="grain"> | `grain` | Film grain or sensor noise: noise of standard deviation `amount` at mid-grey, clumped to `size` px, heavier where `balance` says. | `amount=0.08` `balance=0.0` `size=1.0` `color=False` `distribution='gaussian'` `seed=0` +2 advanced | [grain.toml](../../examples/effects/grain.toml) |
| <img src="../gallery/paper_texture.jpg" width="160" alt="paper_texture"> | `paper_texture` | Lay a paper, canvas or linen texture over the picture: each pixel is multiplied by 1 + `amount` x texture. | `kind='paper'` `amount=0.1` `scale=6.0` `octaves=3` `spacing=6.0` `seed=0` +3 advanced | [paper_texture.toml](../../examples/effects/paper_texture.toml) |
| <img src="../gallery/pixel_grid.jpg" width="160" alt="pixel_grid"> | `pixel_grid` | The picture on an LCD: each `cell` px cell split into red, green and blue stripes with a dark `gap` between. | `cell=4` `gap=0.15` `amount=0.4` `keep_brightness=False` | [pixel_grid.toml](../../examples/effects/pixel_grid.toml) |
| <img src="../gallery/scanlines.jpg" width="160" alt="scanlines"> | `scanlines` | Dark horizontal lines every `spacing` px, each `thickness` x `spacing` thick, darkening by `darkness`. | `spacing=3.0` `thickness=0.5` `darkness=0.35` `offset=0.0` `softness=0.0` | [scanlines.toml](../../examples/effects/scanlines.toml) |
| <img src="../gallery/torn_edge.jpg" width="160" alt="torn_edge"> | `torn_edge` | Tear the border away: each side's pixels closer than a wandering distance of about `width` px become `fill`. | `width=24.0` `roughness=0.5` `scale=24.0` `octaves=3` `persistence=0.4` `lacunarity=2.0` `fill='#f5f0e6'` `seed=0` | [torn_edge.toml](../../examples/effects/torn_edge.toml) |

### `dust_scratches`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `density` | number | count | 0 to 5 | `0.6` | How many specks and hairs there are: marks per 1000 square px of picture (a 32 x 32 px area), so the same density looks alike on any picture size. 0 = none, 0.6 is a light dusting, 3 is filthy. |
| `size` | number | px | 1 to 50 | `8.0` | The size of the biggest dust mark in px: a speck's diameter or a hair's length. Most marks are smaller (speck radii are random, with many tiny ones); larger = bigger blotches and longer hairs. |
| `scratches` | whole number | count | 0 to 100 | `2` | How many scratches: thin, long, nearly vertical lines, each running over half to all of the picture's height. 0 = none. |
| `light` | colour |  |  | `'#ffffff'` | The colour of the light marks (dust on a dark print, scratches into the emulsion): '#rrggbb', '#rgb' or [r, g, b] 0..1. Each mark is light or dark with equal chance. |
| `dark` | colour |  |  | `'#000000'` | The colour of the dark marks (dust on the scanner glass): '#rrggbb', '#rgb' or [r, g, b] 0..1. |
| `seed` | whole number | count | 0 to 4294967295 | `0` | Random seed: the same seed always puts the same marks in the same places, another seed others. |

<details><summary>Advanced parameters (3)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `hairs` | number | fraction | 0 to 1 | `0.15` | The share of the dust marks drawn as hairs (short thin lines in any direction) instead of round specks: 0 = only specks, 1 = only hairs. |
| `angle` | number | degrees | 0 to 45 | `4.0` | The most a scratch may lean away from vertical, in degrees: each scratch gets a random lean up to this, to either side. 0 = all exactly vertical. |
| `width` | number | px | 0.5 to 10 | `1.0` | How wide a scratch is in px. Larger = bolder scratches. |

</details>

A step of `dust_scratches` has `opacity = 0.8` unless it sets its own.

### `grain`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `amount` | number (drivable) | fraction | 0 to 1 | `0.08` | Standard deviation of the grain at mid-grey, in 0..1 picture units: 0 = none, 0.08 is visible, 0.2 is heavy. With `balance` the grain at black and at white is up to 2 x this. |
| `balance` | number (drivable) | ratio | -1 to 1 | `0.0` | Where the grain is heavier, from -1 to 1: -1 = only in the shadows, 0 = even, 1 = only in the highlights. The grain at brightness L (0 black, 1 white) is `amount` x (1 + `balance` x (2 L - 1)), so the shadows get `amount` x (1 - `balance`) and the highlights `amount` x (1 + `balance`). Real film is grainiest in the shadows (a negative balance). |
| `size` | number | px | 0 to 50 | `1.0` | Grain size in px: up to 1 every pixel varies on its own; above 1 the noise is blurred (Gaussian sigma = size / 2) into specks about this wide, keeping its standard deviation. Larger = coarser, softer grain. |
| `color` | true or false |  |  | `False` | True gives every channel its own grain (coloured speckle, like a high-ISO sensor); False adds the same grain to all three (brightness only, grey speckle). |
| `distribution` | one of 'gaussian', 'uniform' |  |  | `'gaussian'` | Where the noise values come from: 'gaussian' (bell curve: most values small, a few large) or 'uniform' (every value up to a limit of about 1.7 x the deviation equally likely: an even, bounded speckle). Both have the standard deviation set by `amount` and `balance`, and both are clumped by `size`. |
| `seed` | whole number | count | 0 to 4294967295 | `0` | Random seed: the same seed always gives the same grain, another seed a different pattern. |

<details><summary>Advanced parameters (2)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `truncate` | number | ratio | 1 to 10 | `4.0` | Where the clumping blur's Gaussian is cut off, in sigmas (only when size > 1). Larger is more exact and slower. |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What the clumping blur sees beyond the picture's edge (only when size > 1): 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge, so the grain tiles seamlessly). |

</details>

Presets (`preset="name"` in a recipe step or `fx`):

| Preset | Description |
| --- | --- |
| `film_grain` | Film grain: grey speckle that is heaviest in the shadows and lightest in the highlights, as on a photographic negative. |
| `noise` | Plain sensor noise: independent colour speckle of the same strength in shadows and highlights. |

### `paper_texture`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `kind` | one of 'paper', 'canvas', 'linen' |  |  | `'paper'` | The surface: 'paper' (random fibres only), 'canvas' (an even plain weave of threads `spacing` px apart, with the fibres mixed in) or 'linen' (the same weave, but every thread has its own random strength, so it looks uneven). |
| `amount` | number | fraction | 0 to 1 | `0.1` | How strong the texture is: the standard deviation of the brightness change as a fraction of each pixel's own value (the texture multiplies the picture, so black stays black and the average brightness is kept). 0 = unchanged, 0.1 is visible, 0.3 is heavy; values lighten and darken alike, and the result is clipped to 0..1. |
| `scale` | number | px | 1 to 256 | `6.0` | Size of the coarsest fibres in px, the spacing of the first noise lattice. Larger = broader, softer mottling; small values give a fine tooth. |
| `octaves` | whole number | count | 1 to 8 | `3` | How many layers of noise are summed for the fibres, each finer than the last (by `lacunarity`) and weaker (by `persistence`). 1 = smooth blotches, more = more fine detail. Layers finer than one pixel are dropped. |
| `spacing` | number | px | 3 to 64 | `6.0` | The thread pitch of 'canvas' and 'linen' in px: the distance from one thread to the next. Ignored for 'paper'. Larger = coarser weave. |
| `seed` | whole number | count | 0 to 4294967295 | `0` | Random seed: the same seed always gives the same texture (fibres and, for 'linen', thread strengths), another seed a different one. |

<details><summary>Advanced parameters (3)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `persistence` | number | ratio | 0 to 1 | `0.5` | How much weaker each finer layer of fibres is than the one before: each layer's weight is this to the power of its number. 0 = only the coarsest layer, 1 = every layer equally strong (very rough). |
| `lacunarity` | number | ratio | 1 to 4 | `2.0` | How much finer each layer of fibres is than the one before: the lattice spacing is divided by this per layer. 1 = every layer the same size, 2 = each half as coarse, larger = detail drops off faster in size. |
| `fibre` | number | fraction | 0 to 1 | `0.3` | For 'canvas' and 'linen', the share of the texture's variance that comes from the random fibres rather than the weave: 0 = a pure, regular weave, 1 = no weave (the same as 'paper'). Ignored for 'paper'. |

</details>

### `pixel_grid`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `cell` | whole number | px | 3 to 32 | `4` | Width and height in px of one grid cell, which holds one red, one green and one blue stripe, each a third of the cell wide. Larger = a coarser grid; the smallest, 3, gives stripes one pixel wide. |
| `gap` | number | fraction | 0 to 1 | `0.15` | Width of the dark lines between sub-pixels, as a fraction of a stripe's width (cell / 3): the same lines run between the stripes and between rows of cells, half of each inside its own sub-pixel. 0 = stripes touch, 1 = nothing is lit and the grid is black. Raising it darkens the picture and shows the grid. |
| `amount` | number | fraction | 0 to 1 | `0.4` | How far the picture is shown through the grid: 0 = the picture unchanged, 1 = the picture as the grid lights it (each channel only inside its own sub-pixels, a third of the picture's brightness or less), in between a mix of the two. |
| `keep_brightness` | true or false |  |  | `False` | Whether the grid is brightened by the share it darkens the picture on average, so the average brightness is kept and raising `amount` or `gap` shows the grid without dimming the picture (a lit stripe is lifted above the picture's own value and clips at 1 where that is already bright; with `gap` 1 and `amount` 1 nothing is lit and the picture is black). False = the grid only ever darkens, as an LCD does. |

### `scanlines`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `spacing` | number | px | 1 to 64 | `3.0` | Distance in px from the top of one line to the top of the next. A spacing that is not a whole number drifts against the pixel rows and the lines are anti-aliased. Larger = fewer, wider-set lines. |
| `thickness` | number | fraction | 0 to 1 | `0.5` | How thick each line is, as a fraction of `spacing`: 0 = no line (the picture is unchanged), 0.5 = lines and gaps equally wide, 1 = the lines touch and the whole picture is darkened alike. A line that covers only part of a pixel row darkens that row in proportion. |
| `darkness` | number | fraction | 0 to 1 | `0.35` | How much a line darkens what is under it: 0 = nothing (the picture is unchanged), 0.35 is a visible screen, 1 = black lines. A line multiplies the picture by 1 - darkness, so colours keep their ratios. |
| `offset` | number | px | 0 to 64 | `0.0` | How far down in px the first line starts from the top of the picture (the middle of its soft edge, when `softness` is above 0). Offsets of more than `spacing` repeat the pattern: the lines are at offset + k x spacing. |
| `softness` | number | fraction | 0 to 1 | `0.0` | How softly a line fades in and out: 0 = hard edges, each row darkened by the share of it a line covers; 1 = as soft as the lines and gaps allow, so with a `thickness` of 0.5 the darkness swells and fades along sin^2, darkest in the middle of a line and nothing in the middle of a gap, as on a CRT. In between, each edge fades over `softness` x the narrower of a line and a gap, either side of the hard edge. It keeps the average darkening of the picture: only its shape changes. |

### `torn_edge`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `width` | number | px | 0 to 200 | `24.0` | How far in the tear is from each side on average, in px: 0 = no tear. Larger = more of the border torn away. |
| `roughness` | number | fraction | 0 to 1 | `0.5` | How ragged the tear is: the standard deviation of its distance from the side as a fraction of `width`. 0 = a straight cut exactly `width` in, 0.3 is a gentle tear, 1 wanders between the very edge and twice the width. |
| `scale` | number | px | 1 to 200 | `24.0` | Size of the tear's broadest bays and tongues along the side, in px (the spacing of the first noise lattice). Larger = long slow waves, small = a jagged edge. |
| `octaves` | whole number | count | 1 to 8 | `3` | How many layers of detail make up the tear, each finer than the last (by `lacunarity`) and weaker (by `persistence`). 1 = smooth waves, more = a fibrous, ragged edge. Layers finer than a pixel are dropped. |
| `persistence` | number | ratio | 0 to 1 | `0.4` | How much weaker each finer layer of the tear is than the one before. 0 = only the broadest layer, 1 = every layer equally strong (very jagged). |
| `lacunarity` | number | ratio | 1 to 4 | `2.0` | How much finer each layer of the tear is than the one before: the lattice spacing is divided by this per layer. 1 = every layer the same size, larger = detail much finer than the waves. |
| `fill` | colour |  |  | `'#f5f0e6'` | The paper colour that replaces the torn-away border: '#rrggbb', '#rgb' or [r, g, b] 0..1. |
| `seed` | whole number | count | 0 to 4294967295 | `0` | Random seed: the same seed always tears the same line (each side its own), another seed a different one. |
