# Subject

Effects that work on the cut-out subject, apart from its background.

[All effects](README.md)

| Example | Effect | What it does | Parameters | Recipe |
| --- | --- | --- | --- | --- |
| <img src="../gallery/disperse.jpg" width="160" alt="disperse"> | `disperse` | The picture at one moment of its dispersal: `progress` 0 = untouched, 1 = finished. | `cell=5` `progress=0.0` `top=1.0` `outline=1.0` `sweep=0.0` `reach=1.0` `wind=0.2` `angle=10.0` `seed=0` +31 advanced | [disperse.toml](../../examples/effects/disperse.toml) |
| <img src="../gallery/drop_shadow.jpg" width="160" alt="drop_shadow"> | `drop_shadow` | A soft shadow of the subject cast on the picture behind it, moved by `dx`, `dy` and softened by `blur`. | `dx=12.0` `dy=18.0` `blur=24.0` `color='#000000'` +2 advanced | [drop_shadow.toml](../../examples/effects/drop_shadow.toml) |
| <img src="../gallery/evaporate.jpg" width="160" alt="evaporate"> | `evaporate` | Dissolve the picture into pieces that float away, thin out and fade. | `cell=5` `start=0.6` `outline=0.0` `outward=0.0` `rise=0.3` `wind=0.0` `vanish=0.3` `seed=0` +15 advanced | [evaporate.toml](../../examples/effects/evaporate.toml) |
| <img src="../gallery/fill.jpg" width="160" alt="fill"> | `fill` | Replace the whole picture with one colour. | `color='#000000'` | [fill.toml](../../examples/effects/fill.toml) |
| <img src="../gallery/layered_cutouts.jpg" width="160" alt="layered_cutouts"> | `layered_cutouts` | Copies of the subject's silhouette stacked behind it, each `offset` px farther along `angle`, like cut paper. | `layers=4` `offset=14.0` `angle=45.0` `color='#ff8a5b'` `end_color='#3b1f6b'` `layer_opacity=1.0` +1 advanced | [layered_cutouts.toml](../../examples/effects/layered_cutouts.toml) |
| <img src="../gallery/long_shadow.jpg" width="160" alt="long_shadow"> | `long_shadow` | A flat-design long shadow: the subject's silhouette repeated along `angle` for `length` px, behind it. | `angle=45.0` `length=300.0` `color='#000000'` `fade=1.0` +1 advanced | [long_shadow.toml](../../examples/effects/long_shadow.toml) |
| <img src="../gallery/outline.jpg" width="160" alt="outline"> | `outline` | A coloured stroke of `width` px around the subject's silhouette, with an optional soft halo of `glow` px. | `width=6.0` `color='#ffffff'` `position='outside'` `glow=0.0` +1 advanced | [outline.toml](../../examples/effects/outline.toml) |
| <img src="../gallery/reflection.jpg" width="160" alt="reflection"> | `reflection` | A floor mirror: the picture above the subject's lowest row is mirrored about it, softened and fading with depth. | `height=0.4` `fade=0.7` `blur=2.0` +3 advanced | [reflection.toml](../../examples/effects/reflection.toml) |
| <img src="../gallery/rim_light.jpg" width="160" alt="rim_light"> | `rim_light` | Light along the subject's edge from one side: a band of `width` px inside the edge, tinted `color`. | `angle=40.0` `color='#ff9a4d'` `width=12.0` `amount=1.0` `falloff=1.5` +4 advanced | [rim_light.toml](../../examples/effects/rim_light.toml) |
| <img src="../gallery/spotlight.jpg" width="160" alt="spotlight"> | `spotlight` | Light the subject and darken the rest: the further from the silhouette, the more the picture goes to `color`. | `amount=0.8` `falloff=0.8` `radius=200.0` `feather=10.0` `color='#000000'` +3 advanced | [spotlight.toml](../../examples/effects/spotlight.toml) |

### `disperse`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `cell` | whole number | px | 1 to 64 | `5` | Character width in px; a piece is one character (its height comes from `font`). Use the same value as the ascii step before this one, so whole characters move. |
| `progress` | number | fraction | 0 to 1 | `0.0` | The moment shown: 0 = untouched, 1 = finished (every piece that goes has gone). |
| `top` | number | ratio | 0 to 10 | `1.0` | Weight of height in the order pieces leave: higher pieces go first. 0 = height does not matter. |
| `outline` | number | ratio | 0 to 10 | `1.0` | Weight of closeness to the outline (and the left, right and top borders) in the order: pieces near the edge go before the middle. 0 = closeness does not matter. |
| `sweep` | number | ratio | -1 to 1 | `0.0` | Weight of side in the order: positive starts at the left, negative at the right; its size is how much it matters next to `top` and `outline`. |
| `reach` | number | fraction | 0 to 1 | `1.0` | How much of the figure ever goes, in leaving order: 0.4 = the first 40% disperses and the rest stays, 1 = everything goes. |
| `wind` | number | fraction | -1 to 1 | `0.2` | The farthest any piece is carried by the time it vanishes, as a fraction of the picture width: positive blows right, negative left. Keep it under a third, or pieces cross too much of the picture in one go. |
| `angle` | number | degrees | -180 to 180 | `10.0` | Direction of the wind above the horizontal (on the side `wind` blows to); negative blows downward. |
| `seed` | whole number | count | 0 to 2147483647 | `0` | Random seed: each piece's fate is fixed by it, so one seed always gives the same pieces the same fates. |

<details><summary>Advanced parameters (31)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `font` | text or none |  |  | `None` | Path to the monospaced .ttf/.ttc whose character height sizes the pieces, or None for the system font; use the same as the ascii step. A path that cannot be opened fails when the effect runs. |
| `hover` | number | fraction | 0 to 1 | `0.0` | Share of its journey a piece floats slowly along the wind before it goes (about 0..0.3 looks right). The outline starts at once, deeper pieces later (see `hover_spread`). 0 = off. |
| `outward` | number | fraction | 0 to 1 | `0.0` | How much a piece is also thrown away from the edge it came from (by up to `outward_distance`); pieces then fly in different directions, so leave it at 0 when everything should travel together on the wind. |
| `ambient` | number | ratio | 0 to 1 | `0.0` | Depth of the slow shimmer of the pieces that stay, as a share of their brightness (about 0..0.1 looks right; brightening stops at white). 0 = off. |
| `outline_depth` | number | fraction | 0.01 to 1 | `0.15` | Distance from the outline (fraction of the picture width) beyond which a piece counts as fully inside, for `outline` and `hover_spread`. Raising it makes closeness to the edge matter deeper in. |
| `jitter` | number | fraction | 0 to 1 | `0.2` | Random share of each piece's place in the leaving order: 0 = a clean front, raising it makes it ragged. |
| `anticipation` | number | fraction | 0.01 to 0.2 | `0.06` | Wind-up time before a piece goes, as a share of `progress`: it eases back by `pullback` over this time. The first piece goes only after it, so every piece has one. |
| `pullback` | number | fraction | 0 to 1 | `0.15` | How far a piece eases back against its journey before it goes (anticipation), as a share of the journey. 0 = no wind-up. |
| `flight` | number | fraction | 0.01 to 0.5 | `0.2` | Average time from detaching to vanishing, as a share of `progress`. Shorter flights keep fewer pieces in the air at once (about 0.1 keeps it to a third on a side-to-side sweep). |
| `flight_variation` | number | fraction | 0 to 0.9 | `0.4` | How much each piece's flight differs from `flight`: 0.4 = from 0.6x to 1.4x. 0 = all the same. |
| `finish` | number | fraction | 0.5 to 1 | `0.98` | The progress by which the last piece has vanished (a little under 1 leaves nothing behind at the end). If `anticipation` and the longest flight do not fit before it, all pieces go at once. |
| `hover_spread` | number | fraction | 0 to 1 | `0.45` | With `hover`: how far into `progress` a piece `outline_depth` inside starts to float (the outline starts at 0). 0 = everything floats at once. |
| `bob` | number | fraction | 0 to 0.05 | `0.004` | Height of the gentle up-and-down of a hovering piece, as a fraction of the picture height. |
| `bob_cycles` | number | cycles | 0 to 20 | `3.0` | Number of ups and downs while a piece hovers. |
| `angle_spread` | number | degrees | 0 to 180 | `12.0` | Range of each piece's own direction around the wind's `angle` (it differs by up to half of this). |
| `distance_minimum` | number | fraction | 0 to 1 | `0.35` | The shortest journey as a share of the longest (`wind`), so speeds differ; 1 = all travel equally far. |
| `outward_distance` | number | fraction | 0 to 1 | `0.12` | How far `outward` 1 throws a piece away from its edge, as a fraction of the picture's width (sideways) and height (up and down). |
| `acceleration` | number | ratio | 0 to 10 | `0.5` | How a piece speeds up on its way (an exit accelerates): it ends 1 + 2 x acceleration times as fast as it starts. 0 = steady speed. |
| `sway` | number | fraction | 0 to 0.2 | `0.012` | Largest sideways wobble of a flying piece, as a fraction of the picture width; it grows during the flight. 0 = none. |
| `sway_cycles` | number | cycles | 0 to 20 | `1.5` | Number of wobbles during a piece's flight. |
| `falloff` | number | ratio | 0 to 10 | `1.5` | How a flying piece fades: brightness is (1 - share of flight)^falloff. 0 = no fade (it just vanishes at the end), 1 = even, above 1 fades quickly at first. |
| `flare` | number | ratio | 0 to 2 | `0.4` | Extra brightness at the peak of the glow as a piece leaves (0.4 = 1.4x); brightening stops at white. 0 = no glow. |
| `flare_span` | number | fraction | 0.01 to 1 | `0.2` | Share of a piece's flight that the glow lasts; it peaks halfway through that. |
| `ambient_phase` | number | cycles | 0 to 1 | `0.0` | Where the shimmer wave is in its cycle: 0 and 1 are the same picture, 0.5 is half a cycle on. Raising it moves the wave against the wind; with `ambient` at 0 it does nothing. |
| `ambient_wavelength` | number | fraction | 0.01 to 2 | `0.2` | Length of one shimmer wave as a fraction of the picture width; a shorter one packs more waves across the picture. |
| `open_bottom` | true or false |  |  | `True` | True: the bottom border is not an edge for `outline` and `hover` (a portrait is cut off there). False: it counts like the other borders. |
| `normal_smoothing` | number | count | 0 to 10 | `1.0` | Blur, in characters (a Gaussian sigma), of the outline before `outward` reads its direction: raising it makes neighbouring pieces fly in more similar directions; 0 uses the raw outline. |
| `normal_truncate` | number | ratio | 1 to 10 | `4.0` | Where the `normal_smoothing` blur is cut off, in sigmas: it reaches this many times `normal_smoothing` characters from the outline, so the pieces' direction `outward` is read from that far out. Larger is more exact and slower, smaller shortens the reach. Nothing changes with `normal_smoothing` at 0. |
| `edge` | one of 'reflect', 'constant', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What the `normal_smoothing` blur sees beyond the picture's border: 'reflect' (mirrored, border row repeated; a border stays an edge), 'mirror' (mirrored about the border row), 'nearest' (the border row repeated), 'wrap' (the opposite border), 'constant' (open space, as if nothing were there, which blurs the outline toward the border). |
| `matte_threshold` | number | fraction | 0 to 1 | `0.5` | With a matte: how much of a piece, 0 to 1, the matte must cover on average for the piece to belong to the figure that `outline` and `hover` work from; 0.5 = more than half. Lowering it takes in more of a soft edge (hair, fur), raising it less. Ignored without a matte. |
| `background_threshold` | number | fraction | 0 to 1 | `0.02` | How bright, 0 to 1, some pixel of a piece must be for the piece to count as picture rather than black background: a piece no brighter never flies and is drawn black once the dispersal (or the shimmer) has begun. Raising it drops dim, noisy pieces; without a matte it also decides the figure's outline. |

</details>

### `drop_shadow`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `dx` | number | px | -500 to 500 | `12.0` | How far the shadow is moved to the right, in px at the recipe's size; negative moves it left. 0 with `dy` 0 hides the shadow behind the subject, leaving only what the blur spreads past its edge. |
| `dy` | number | px | -500 to 500 | `18.0` | How far the shadow is moved down, in px at the recipe's size; negative moves it up. |
| `blur` | number | px | 0 to 200 | `24.0` | Gaussian sigma of the shadow's soft edge in px: raising it makes the shadow larger and fainter at its edge; 0 = a hard-edged copy of the subject's silhouette. |
| `color` | colour |  |  | `'#000000'` | The colour of the shadow: '#rrggbb', '#rgb' or [r, g, b] 0..1. Black darkens what lies behind the subject; another colour tints it. |

<details><summary>Advanced parameters (2)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `truncate` | number | ratio | 1 to 10 | `4.0` | Where the shadow's Gaussian is cut off, in sigmas: it reaches truncate x blur px. Larger is more exact and slower. |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'nearest'` | What the shadow sees beyond the picture's edge, when it is moved and softened: 'nearest' (the outermost row repeated, so a subject cut off by the border casts its shadow on without a gap), 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel) or 'wrap' (the opposite edge). |

</details>

A step of `drop_shadow` has `opacity = 0.6` unless it sets its own.

### `evaporate`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `cell` | whole number | px | 1 to 64 | `5` | Character width in px; a piece is one character (its height comes from `font`). Use the same value as the ascii step before this one, so whole characters fly away. |
| `start` | number | fraction | 0 to 1 | `0.6` | Height (fraction of the picture, 0 = top) above which pieces begin to detach from the top down. 0 switches the top-down dissolve off. |
| `outline` | number | fraction | 0 to 1 | `0.0` | Width of the band (fraction of the picture width) along the figure's outline, and the left, right and top borders, where pieces detach; stronger nearer the edge. 0 switches it off. Raising it eats further in. |
| `outward` | number | fraction | 0 to 1 | `0.0` | How much a piece turns away from the edge it came from (the left of the head sheds to the left): 0 = straight up, 1 = as much sideways as up. |
| `rise` | number | fraction | 0 to 1 | `0.3` | Farthest a piece climbs, as a fraction of the picture height (at least one character row). Raising it spreads the smoke higher. |
| `wind` | number | ratio | -5 to 5 | `0.0` | Sideways drift in characters per row climbed: positive to the right, negative to the left. |
| `vanish` | number | fraction | 0 to 1 | `0.3` | Share of detached pieces already gone before they climb (0..1); `thinning` removes more with height. |
| `seed` | whole number | count | 0 to 2147483647 | `0` | Random seed: the same seed always gives the same dissolve, another seed another one. |

<details><summary>Advanced parameters (15)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `font` | text or none |  |  | `None` | Path to the monospaced .ttf/.ttc whose character height sizes the pieces, or None for the system font; use the same as the ascii step. A path that cannot be opened fails when the effect runs. |
| `ramp` | number | fraction | 0.01 to 1 | `0.3` | Height of the band (fraction of the picture) above `start` over which the chance of detaching rises smoothly from 0 to 1; above the band every piece has detached. Raising it spreads the dissolve out. |
| `rise_gamma` | number | ratio | 0 to 5 | `0.6` | Gamma of the climb against how strongly a piece is pulled away (0..1): a piece climbs up to `rise` x pull^rise_gamma. Below 1 even weakly pulled pieces climb far, above 1 only the strongly pulled ones. |
| `rise_minimum` | number | fraction | 0 to 1 | `0.1` | Shortest climb as a share of a piece's longest one; 1 makes every piece climb the full way. |
| `rise_skew` | number | ratio | 0 to 5 | `1.5` | Exponent on each piece's random share of its climb: above 1 most pieces stay low and a few go high, 1 spreads them evenly, below 1 most go high. |
| `thinning` | number | fraction | 0 to 1 | `0.5` | Extra share of pieces gone by the time they climb the full `rise` (on top of `vanish`): raising it makes the smoke sparser higher up. |
| `falloff` | number | ratio | 0 to 5 | `1.5` | How fast a piece dims as it climbs: brightness is (1 - climbed share of `rise`)^falloff. 0 keeps full brightness, 1 dims evenly, above 1 dims quickly at first. |
| `sway` | number | ratio | 0 to 1 | `0.12` | Random sideways sway in characters per row climbed (a standard deviation): raising it makes the smoke spread wider the higher it goes. |
| `sway_minimum` | number | count | 0 to 5 | `0.3` | Sideways sway every detached piece has, however little it climbs, in characters (a standard deviation). |
| `open_bottom` | true or false |  |  | `True` | True: the bottom border is not an edge for `outline` (a portrait is simply cut off there). False: it sheds like the other borders. |
| `normal_smoothing` | number | count | 0 to 10 | `1.0` | Blur, in characters (a Gaussian sigma), of the outline before `outward` reads its direction: raising it makes neighbouring pieces fly in more similar directions; 0 uses the raw outline. |
| `normal_truncate` | number | ratio | 1 to 10 | `4.0` | Where the `normal_smoothing` blur is cut off, in sigmas: it reaches this many times `normal_smoothing` characters from the outline, so the pieces' direction `outward` is read from that far out. Larger is more exact and slower, smaller shortens the reach. Nothing changes with `normal_smoothing` at 0. |
| `edge` | one of 'reflect', 'constant', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What the `normal_smoothing` blur sees beyond the picture's border: 'reflect' (mirrored, border row repeated; a border stays an edge), 'mirror' (mirrored about the border row), 'nearest' (the border row repeated), 'wrap' (the opposite border), 'constant' (open space, as if nothing were there, which blurs the outline toward the border). |
| `matte_threshold` | number | fraction | 0 to 1 | `0.5` | With a matte: how much of a piece, 0 to 1, the matte must cover on average for the piece to belong to the figure that `outline` sheds from; 0.5 = more than half. Lowering it takes in more of a soft edge (hair, fur), raising it less. Ignored without a matte. |
| `background_threshold` | number | fraction | 0 to 1 | `0.02` | How bright, 0 to 1, some pixel of a piece must be for the piece to count as picture rather than black background: a piece no brighter never drifts (it simply vanishes if it detaches). Raising it leaves dim, noisy pieces behind; without a matte it also decides the figure's outline. |

</details>

Presets (`preset="name"` in a recipe step or `fx`):

| Preset | Description |
| --- | --- |
| `halo` | The top of the head and the sides shed outward, symmetric: a halo of dissolving pieces. |
| `windblown` | The edges dissolve and everything is blown to the right, like dust in a wind. |
| `disintegrate` | Heavy: the whole outline and the top come apart, in long plumes in every direction. |

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
| `layer_opacity` | number | fraction | 0 to 1 | `1.0` | How much of what lies behind each layer it covers: 1 = fully (paper), below 1 the layers show through each other and the picture; 0 = no layers. Not the step's `opacity`: each layer is laid at this over the ones beyond it, so where layers overlap they compound (two layers at 0.5 cover 0.75), while the step's `opacity` mixes the finished stack in once. |

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
| `fade` | number | fraction | 0 to 1 | `1.0` | How much of its opacity the shadow loses along its length, in a straight line: 0 = solid to the end, 0.5 = half as opaque at the end, 1 = fades out completely at `length`. |

<details><summary>Advanced parameters (1)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'nearest'` | What the shadow sees beyond the picture's edge when the silhouette is moved: 'nearest' (the outermost row repeated, so a subject cut off by the border casts its shadow on without a gap), 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel) or 'wrap' (the opposite edge). |

</details>

A step of `long_shadow` has `opacity = 0.5` unless it sets its own.

### `outline`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `width` | number | px | 0 to 200 | `6.0` | Thickness of the stroke in px at the recipe's size; raising it thickens the line. 0 = no stroke. |
| `color` | colour |  |  | `'#ffffff'` | The colour of the stroke and of its halo: '#rrggbb', '#rgb' or [r, g, b] 0..1. |
| `position` | one of 'outside', 'inside', 'center' |  |  | `'outside'` | Which side of the silhouette's edge the stroke lies on: 'outside' (around the subject, whose own pixels stay untouched), 'inside' (painted over the subject's rim) or 'center' (half of `width` on each side). |
| `glow` | number | px | 0 to 200 | `0.0` | How far a soft halo of the same colour reaches beyond the stroke, in px; it fades smoothly from the stroke to nothing. Raising it spreads the halo (a large value makes an aura). 0 = no halo. |

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

<details><summary>Advanced parameters (3)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `threshold` | number | fraction | 0 to 1 | `0.5` | How much of a matte row, 0 to 1, must be subject for the row to count as part of it when its lowest and highest rows are found: 0.5 = a pixel of at least half. Lowering it counts faint fringes, so the floor line moves down and the subject's height grows. |
| `truncate` | number | ratio | 1 to 10 | `4.0` | Where the blur's Gaussian is cut off, in sigmas: it reaches truncate x blur px. Larger is more exact and slower. |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What the blur sees beyond the picture's edge, and beyond the floor line, where the reflection is mirrored: 'reflect' (mirrored, edge pixel repeated, which continues the mirror image), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated) or 'wrap' (the opposite edge). |

</details>

A step of `reflection` has `opacity = 0.6` unless it sets its own.

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
