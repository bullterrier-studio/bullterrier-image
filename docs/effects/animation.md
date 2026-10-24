# Animation

Effects that change over time, such as particles dispersing or evaporating.

[All effects](README.md)

| Example | Effect | What it does | Parameters | Recipe |
| --- | --- | --- | --- | --- |
| <img src="../gallery/disperse.jpg" width="160" alt="disperse"> | `disperse` | The picture at one moment of its dispersal: `progress` 0 = untouched, 1 = finished. | `cell=5` `font=None` `progress=0.0` `top=1.0` `outline=1.0` `sweep=0.0` `reach=1.0` `wind=0.2` `angle=10.0` `hover=0.0` `outward=0.0` `ambient=0.0` `clock=0.0` `seed=0` `outline_depth=0.15` `jitter=0.2` `anticipation=0.06` `pullback=0.15` `flight=0.2` `flight_variation=0.4` `finish=0.98` `hover_spread=0.45` `bob=0.004` `bob_cycles=3.0` `angle_spread=12.0` `distance_minimum=0.35` `outward_distance=0.12` `acceleration=0.5` `sway=0.012` `sway_cycles=1.5` `falloff=1.5` `flare=0.4` `flare_span=0.2` `ambient_period=3.0` `ambient_wavelength=0.2` `open_bottom=True` `normal_smoothing=1.0` `normal_truncate=4.0` `edge='reflect'` `matte_threshold=0.5` `background_threshold=0.02` | [disperse.toml](../../examples/effects/disperse.toml) |
| <img src="../gallery/evaporate.jpg" width="160" alt="evaporate"> | `evaporate` | Dissolve the picture into pieces that float away, thin out and fade. | `cell=5` `font=None` `start=0.6` `end=0.3` `outline=0.0` `outward=0.0` `rise=0.3` `wind=0.0` `vanish=0.3` `seed=0` `rise_gamma=0.6` `rise_minimum=0.1` `rise_skew=1.5` `thinning=0.5` `falloff=1.5` `sway=0.12` `sway_minimum=0.3` `open_bottom=True` `normal_smoothing=1.0` `normal_truncate=4.0` `edge='reflect'` `matte_threshold=0.5` `background_threshold=0.02` | [evaporate.toml](../../examples/effects/evaporate.toml) |

### `disperse`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `cell` | whole number | px | 1 to 64 | `5` | Character width in px; a piece is one character (its height comes from `font`). Use the same value as the ascii step before this one, so whole characters move. |
| `font` | text or none |  |  | `None` | Path to the monospaced .ttf/.ttc whose character height sizes the pieces, or None for the system font; use the same as the ascii step. A path that cannot be opened fails when the effect runs. |
| `progress` | number | fraction | 0 to 1 | `0.0` | The moment shown: 0 = untouched, 1 = finished (every piece that goes has gone). Animate it. |
| `top` | number | ratio | 0 to 10 | `1.0` | Weight of height in the order pieces leave: higher pieces go first. 0 = height does not matter. |
| `outline` | number | ratio | 0 to 10 | `1.0` | Weight of closeness to the outline (and the left, right and top borders) in the order: pieces near the edge go before the middle. 0 = closeness does not matter. |
| `sweep` | number | ratio | -1 to 1 | `0.0` | Weight of side in the order: positive starts at the left, negative at the right; its size is how much it matters next to `top` and `outline`. |
| `reach` | number | fraction | 0 to 1 | `1.0` | How much of the figure ever goes, in leaving order: 0.4 = the first 40% disperses and the rest stays, 1 = everything goes. |
| `wind` | number | fraction | -1 to 1 | `0.2` | The farthest any piece is carried by the time it vanishes, as a fraction of the picture width: positive blows right, negative left. Keep it under a third, or pieces cross too much of the picture in one go. |
| `angle` | number | degrees | -180 to 180 | `10.0` | Direction of the wind above the horizontal (on the side `wind` blows to); negative blows downward. |
| `hover` | number | fraction | 0 to 1 | `0.0` | Share of its journey a piece floats slowly along the wind before it goes (about 0..0.3 looks right). The outline starts at once, deeper pieces later (see `hover_spread`). 0 = off. |
| `outward` | number | fraction | 0 to 1 | `0.0` | How much a piece is also thrown away from the edge it came from (by up to `outward_distance`); pieces then fly in different directions, so leave it at 0 when everything should travel together on the wind. |
| `ambient` | number | ratio | 0 to 1 | `0.0` | Depth of the slow shimmer of the pieces that stay, as a share of their brightness (about 0..0.1 looks right; brightening stops at white). 0 = off. |
| `clock` | number | seconds | 0 to 3600 | `0.0` | The time the shimmer runs on: animate it over the whole clip (`over = "clip"`) so the picture is alive even while nothing detaches. |
| `seed` | whole number | count | 0 to 2147483647 | `0` | Random seed: each piece's fate is fixed by it, so every frame of one seed fits together. |
| `outline_depth` | number | fraction | 0.01 to 1 | `0.15` | Distance from the outline (fraction of the picture width) beyond which a piece counts as fully inside, for `outline` and `hover_spread`. Raising it makes closeness to the edge matter deeper in. |
| `jitter` | number | fraction | 0 to 1 | `0.2` | Random share of each piece's place in the leaving order: 0 = a clean front, raising it makes it ragged. |
| `anticipation` | number | fraction | 0.01 to 0.2 | `0.06` | Wind-up time before a piece goes, as a share of the clip: it eases back by `pullback` over this time. The first piece goes only after it, so every piece has one. |
| `pullback` | number | fraction | 0 to 1 | `0.15` | How far a piece eases back against its journey before it goes (anticipation), as a share of the journey. 0 = no wind-up. |
| `flight` | number | fraction | 0.01 to 0.5 | `0.2` | Average time from detaching to vanishing, as a share of the clip. Shorter flights keep fewer pieces in the air at once (about 0.1 keeps it to a third on a side-to-side sweep). |
| `flight_variation` | number | fraction | 0 to 0.9 | `0.4` | How much each piece's flight differs from `flight`: 0.4 = from 0.6x to 1.4x. 0 = all the same. |
| `finish` | number | fraction | 0.5 to 1 | `0.98` | The progress by which the last piece has vanished (a little under 1 gives a clean last frame). If `anticipation` and the longest flight do not fit before it, all pieces go at once. |
| `hover_spread` | number | fraction | 0 to 1 | `0.45` | With `hover`: how far into the clip a piece `outline_depth` inside starts to float (the outline starts at 0). 0 = everything floats at once. |
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
| `ambient_period` | number | seconds | 0.1 to 60 | `3.0` | Seconds per shimmer cycle (breathing is 2-4 s). |
| `ambient_wavelength` | number | fraction | 0.01 to 2 | `0.2` | Length of one shimmer wave as a fraction of the picture width; with the period it sets how fast the wave travels (against the wind). |
| `open_bottom` | true or false |  |  | `True` | True: the bottom border is not an edge for `outline` and `hover` (a portrait is cut off there). False: it counts like the other borders. |
| `normal_smoothing` | number | count | 0 to 10 | `1.0` | Blur, in characters (a Gaussian sigma), of the outline before `outward` reads its direction: raising it makes neighbouring pieces fly in more similar directions; 0 uses the raw outline. |
| `normal_truncate` | number | ratio | 1 to 10 | `4.0` | Where the `normal_smoothing` blur is cut off, in sigmas: it reaches this many times `normal_smoothing` characters from the outline, so the pieces' direction `outward` is read from that far out. Larger is more exact and slower, smaller shortens the reach. Nothing changes with `normal_smoothing` at 0. |
| `edge` | one of 'reflect', 'constant', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What the `normal_smoothing` blur sees beyond the picture's border: 'reflect' (mirrored, border row repeated; a border stays an edge), 'mirror' (mirrored about the border row), 'nearest' (the border row repeated), 'wrap' (the opposite border), 'constant' (open space, as if nothing were there, which blurs the outline toward the border). |
| `matte_threshold` | number | fraction | 0 to 1 | `0.5` | With a matte: how much of a piece, 0 to 1, the matte must cover on average for the piece to belong to the figure that `outline` and `hover` work from; 0.5 = more than half. Lowering it takes in more of a soft edge (hair, fur), raising it less. Ignored without a matte. |
| `background_threshold` | number | fraction | 0 to 1 | `0.02` | How bright, 0 to 1, some pixel of a piece must be for the piece to count as picture rather than black background: a piece no brighter never flies and is drawn black once the dispersal (or the shimmer) has begun. Raising it drops dim, noisy pieces; without a matte it also decides the figure's outline. |

### `evaporate`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `cell` | whole number | px | 1 to 64 | `5` | Character width in px; a piece is one character (its height comes from `font`). Use the same value as the ascii step before this one, so whole characters fly away. |
| `font` | text or none |  |  | `None` | Path to the monospaced .ttf/.ttc whose character height sizes the pieces, or None for the system font; use the same as the ascii step. A path that cannot be opened fails when the effect runs. |
| `start` | number | fraction | 0 to 1 | `0.6` | Height (fraction of the picture, 0 = top) above which pieces begin to detach from the top down. 0 switches the top-down dissolve off. |
| `end` | number | fraction | 0 to 1 | `0.3` | Height (fraction of the picture, 0 = top) above which every piece has detached. Between `end` and `start` the chance rises smoothly. |
| `outline` | number | fraction | 0 to 1 | `0.0` | Width of the band (fraction of the picture width) along the figure's outline, and the left, right and top borders, where pieces detach; stronger nearer the edge. 0 switches it off. Raising it eats further in. |
| `outward` | number | fraction | 0 to 1 | `0.0` | How much a piece turns away from the edge it came from (the left of the head sheds to the left): 0 = straight up, 1 = as much sideways as up. |
| `rise` | number | fraction | 0 to 1 | `0.3` | Farthest a piece climbs, as a fraction of the picture height (at least one character row). Raising it spreads the smoke higher. |
| `wind` | number | ratio | -5 to 5 | `0.0` | Sideways drift in characters per row climbed: positive to the right, negative to the left. |
| `vanish` | number | fraction | 0 to 1 | `0.3` | Share of detached pieces already gone before they climb (0..1); `thinning` removes more with height. |
| `seed` | whole number | count | 0 to 2147483647 | `0` | Random seed: the same seed always gives the same dissolve, another seed another one. |
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
