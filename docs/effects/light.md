# Light

Bloom and local contrast: they add or shape the light in the picture.

[All effects](README.md)

| Example | Effect | What it does | Parameters | Recipe |
| --- | --- | --- | --- | --- |
| <img src="../gallery/ambient_occlusion.jpg" width="160" alt="ambient_occlusion"> | `ambient_occlusion` | Darken the places the depth says are sheltered: creases, and the ground or wall right beside something nearer. | `field='depth'` `radius=10.0` `gain=4.0` +2 advanced | [ambient_occlusion.toml](../../examples/effects/ambient_occlusion.toml) |
| <img src="../gallery/anamorphic_streaks.jpg" width="160" alt="anamorphic_streaks"> | `anamorphic_streaks` | Horizontal streaks of light: what is brighter than `threshold` is smeared along its row, tinted `color` and screened over the picture by `amount`. | `length=168.0` `threshold=0.7` `amount=6.0` `color='#8fb8ff'` | [anamorphic_streaks.toml](../../examples/effects/anamorphic_streaks.toml) |
| <img src="../gallery/bloom.jpg" width="160" alt="bloom"> | `bloom` | Bloom, glow, soft focus or halation: light bleeding from what is brighter than `threshold`, blurred at `octaves` scales and screened over the picture. | `radius=32.0` `threshold=0.6` `amount=0.7` `color=None` `octaves=4` `persistence=0.5` +3 advanced | [bloom.toml](../../examples/effects/bloom.toml) |
| <img src="../gallery/chromatic_aberration.jpg" width="160" alt="chromatic_aberration"> | `chromatic_aberration` | Lens colour fringes or a prism rainbow: add up `samples` copies of the picture, scaled about the centre `x`, `y` and coloured as a spectrum. | `amount=4.0` `samples=3` `x=0.5` `y=0.5` +2 advanced | [chromatic_aberration.toml](../../examples/effects/chromatic_aberration.toml) |
| <img src="../gallery/dehaze.jpg" width="160" alt="dehaze"> | `dehaze` | Remove haze and fog by inverting the atmospheric scattering model I = J t + A (1 - t) (He, Sun, Tang 2009). | `amount=0.9` `patch=15` `radius=30.0` `airlight_fraction=0.001` +3 advanced | [dehaze.toml](../../examples/effects/dehaze.toml) |
| <img src="../gallery/focus_blur.jpg" width="160" alt="focus_blur"> | `focus_blur` | A shallow depth of field, or a tilt-shift miniature: each pixel is blurred by w x `radius`, w = clip((\|field - focus\| - width / 2) / feather, 0, 1), so the pixels whose field lies within `width` / 2 of `focus` are returned exactly as they are. | `field='depth'` `focus=0.8` `width=0.15` `feather=0.4` `radius=10.0` +3 advanced | [focus_blur.toml](../../examples/effects/focus_blur.toml) |
| <img src="../gallery/gaussian_blur.jpg" width="160" alt="gaussian_blur"> | `gaussian_blur` | Soften the picture with a Gaussian blur of standard deviation `radius` px. | `radius=4.0` +3 advanced | [gaussian_blur.toml](../../examples/effects/gaussian_blur.toml) |
| <img src="../gallery/highpass.jpg" width="160" alt="highpass"> | `highpass` | Keep only the detail finer than `radius` px: the picture minus its Gaussian blur, centred on mid-grey. | `radius=4.0` +2 advanced | [highpass.toml](../../examples/effects/highpass.toml) |
| <img src="../gallery/lens_blur.jpg" width="160" alt="lens_blur"> | `lens_blur` | Blur the picture over a disc of `radius` px, as an out-of-focus lens does, favouring bright pixels. | `radius=8.0` `highlights=0.0` +1 advanced | [lens_blur.toml](../../examples/effects/lens_blur.toml) |
| <img src="../gallery/lens_flare.jpg" width="160" alt="lens_flare"> | `lens_flare` | A lens flare: a halo and a ring round the light at (`x`, `y`) and `ghosts` discs along the line through the picture's centre, tinted `color` and screened over the picture by `amount`. | `x=0.75` `y=0.25` `ghosts=5` `size=30.0` `halo=120.0` `ring=72.0` `amount=0.8` `color='#ffe2b8'` +2 advanced | [lens_flare.toml](../../examples/effects/lens_flare.toml) |
| <img src="../gallery/light_leak.jpg" width="160" alt="light_leak"> | `light_leak` | Warm washes of light coming in from the side `angle` points to: `blobs` seeded patches, tinted `color` and screened over the picture by `amount`. | `color='#ff8a3d'` `amount=0.8` `size=150.0` `angle=0.0` `blobs=3` `seed=0` | [light_leak.toml](../../examples/effects/light_leak.toml) |
| <img src="../gallery/light_rays.jpg" width="160" alt="light_rays"> | `light_rays` | God rays: what is brighter than `threshold` is streaked away from (`x`, `y`) and screened over the picture by `amount`. | `x=0.5` `y=0.3` `length=0.5` `threshold=0.7` `amount=2.5` `decay=0.9` +3 advanced | [light_rays.toml](../../examples/effects/light_rays.toml) |
| <img src="../gallery/motion_blur.jpg" width="160" alt="motion_blur"> | `motion_blur` | Smear the picture along a straight line of `length` px at `angle` degrees, as a moving camera would. | `angle=0.0` `length=24.0` +1 advanced | [motion_blur.toml](../../examples/effects/motion_blur.toml) |
| <img src="../gallery/radial_blur.jpg" width="160" alt="radial_blur"> | `radial_blur` | Smear the picture around a centre (`mode` 'spin') or away from it ('zoom'), by `amount`. | `mode='zoom'` `amount=0.1` `x=0.5` `y=0.5` +3 advanced | [radial_blur.toml](../../examples/effects/radial_blur.toml) |
| <img src="../gallery/relight.jpg" width="160" alt="relight"> | `relight` | Light the picture from the shape of a field, read as a relief `height` px tall, from `angle` at `elevation`: bump lighting, emboss, relighting by depth. | `field='depth'` `angle=225.0` `elevation=45.0` `height=200.0` `blur=2.0` `saturation=1.0` +2 advanced | [relight.toml](../../examples/effects/relight.toml) |
| <img src="../gallery/sharpen.jpg" width="160" alt="sharpen"> | `sharpen` | Sharpen, or add clarity, with an unsharp mask: add `amount` times the detail that a `radius` px Gaussian blur removes. | `radius=2.0` `amount=0.8` `threshold=0.0` +3 advanced | [sharpen.toml](../../examples/effects/sharpen.toml) |
| <img src="../gallery/starburst.jpg" width="160" alt="starburst"> | `starburst` | Diffraction spikes, sparkle or star glints: what is brighter than `threshold` gets `rays` thin lines of light, tinted `color` and screened over the picture by `amount`. | `rays=4` `length=60.0` `threshold=0.95` `amount=0.8` `angle=0.0` `spacing=0.0` `color='#ffffff'` +4 advanced | [starburst.toml](../../examples/effects/starburst.toml) |
| <img src="../gallery/tone_map.jpg" width="160" alt="tone_map"> | `tone_map` | HDR tone mapping, shadows and highlights recovery, lifted shadows. | `contrast=0.5` `amount=0.5` `pivot=0.0` `radius=30.0` +1 advanced | [tone_map.toml](../../examples/effects/tone_map.toml) |
| <img src="../gallery/vignette.jpg" width="160" alt="vignette"> | `vignette` | Fade the picture toward `color` from `radius` out to its corners, the centre untouched. | `amount=0.5` `radius=0.5` `feather=0.5` `roundness=1.0` `color='#000000'` | [vignette.toml](../../examples/effects/vignette.toml) |

### `ambient_occlusion`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `field` | field name |  |  | `'depth'` | The field read as how near each pixel is, 1 = nearest: `depth` (the scene's depth), `light` (bright parts stand nearer, so dark creases between them darken), `subject` (a contact shadow around the subject), or a region of the recipe. |
| `radius` | number | px | 1 to 200 | `10.0` | The size of the neighbourhood that counts as shelter, the Gaussian sigma in px at the recipe's size over which the surrounding depth is averaged. Raising it darkens wider areas around nearer things; lowering it keeps the darkening to fine creases. |
| `gain` | number | ratio | 1 to 100 | `4.0` | How strongly a depth difference darkens: a pixel lying d farther than its surroundings (depth 0..1) is darkened by gain x d, at most to black. Raising it darkens shallow creases more and turns deep ones black sooner. |

<details><summary>Advanced parameters (2)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `truncate` | number | ratio | 1 to 10 | `4.0` | Where the averaging Gaussian is cut off, in sigmas: it reaches truncate x radius px. Larger is more exact and slower. |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What the averaging sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated) or 'wrap' (the opposite edge, so a tileable picture darkens without a seam). |

</details>

A step of `ambient_occlusion` has `opacity = 0.6` unless it sets its own.

### `anamorphic_streaks`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `length` | number | px | 0.5 to 480 | `168.0` | How far the streaks reach sideways, in px tuned for a picture about 480 px high: the distance over which a streak fades to 37% of its brightness (the faint tail runs on, to the picture's edge). Raising it makes longer, fainter streaks. |
| `threshold` | number | fraction | 0 to 0.99 | `0.7` | The brightness (0..1, per channel) where a streak starts: only values above it light the streaks, rising to full at 1. Raising it limits the streaks to the brightest highlights. |
| `amount` | number | ratio | 0 to 10 | `6.0` | How strongly the streaks are screened over the picture: 0 = off; the streak is spread thin along its length, so a long one needs a high amount (above 1 it saturates to white sooner). |
| `color` | colour |  |  | `'#8fb8ff'` | The tint of the streaks: '#rrggbb', '#rgb' or [r, g, b] 0..1. The default is the cool blue of coated anamorphic lenses; white keeps the colour of the light that makes them. |

### `bloom`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `radius` | number (drivable) | px | 0 to 500 | `32.0` | Gaussian sigma of the widest halo in px at the recipe's size: larger spreads the light further; 0 keeps the light on the highlights themselves. Each further octave is half as wide. |
| `threshold` | number | fraction | 0 to 0.99 | `0.6` | The brightness (0..1) where the bloom starts, judged on each channel on its own (on the brightest of the three when `color` is set): only values above it give light, rising to full at 1. Raising it limits the bloom to the brightest highlights. |
| `amount` | number (drivable) | ratio | 0 to 10 | `0.7` | How strongly the halo is screened over the picture: 0 = off, 1 = the halo at full brightness; above 1 it saturates to white sooner. |
| `color` | colour or none |  |  | `None` | The colour of the light: '#rrggbb', '#rgb' or [r, g, b] 0..1; none (the default) lets each highlight keep its own colour. A colour makes every highlight bloom in it, whatever its own colour is, with the strength of its brightest channel: '#ff4a14' is the red-orange of film halation, white a neutral glow. |
| `octaves` | whole number | count | 1 to 8 | `4` | How many scales the halo is built from: octave 1 has the sigma `radius`, each next one half of the one before (radius / 2, radius / 4...). 1 = a single Gaussian; more adds a tighter, brighter core around the highlights. |
| `persistence` | number | ratio | 0 to 1 | `0.5` | How much weight each octave gets relative to the wider one before it (the weights are persistence^0, persistence^1... scaled to add up to 1): 0 = the widest octave alone, 1 = every octave equally. Lower values give a softer, more diffuse bloom; higher values a tighter, more concentrated one. |

<details><summary>Advanced parameters (3)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `truncate` | number | ratio | 1 to 10 | `4.0` | Where each octave's Gaussian is cut off, in sigmas: it reaches truncate x its sigma px. Larger is more exact and slower. |
| `edge` | one of 'reflect', 'constant', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What the halo's blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge) or 'constant' (constant_value, so the bloom fades toward the edge when it is 0). |
| `constant_value` | number | fraction | 0 to 1 | `0.0` | The highlight (0..1) assumed beyond the edge when edge is 'constant'; ignored otherwise. |

</details>

Presets (`preset="name"` in a recipe step or `fx`):

| Preset | Description |
| --- | --- |
| `soft_focus` | Orton soft focus: a blurred copy of the whole picture, half-strength, screened over the sharp one. |
| `halation` | Film halation: a red-orange glow around the brightest highlights, from light bouncing back through the film base. |
| `glow` | A glow: the bright parts blurred into one soft halo and screened over the sharp picture. |

### `chromatic_aberration`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `amount` | number (drivable) | px | 0 to 100 | `4.0` | How far red and blue are pulled apart, in pixels at the farthest corner of the picture: there the red copy is shifted this far outward from the centre and the blue copy this far inward, while the middle one stays where it is. The shift grows in proportion to the distance from the centre, so it is half as much halfway out and nothing at the centre. A larger amount is wider fringes; 0 leaves the picture unchanged. A shift larger than the distance to the farthest corner is limited to it. |
| `samples` | whole number | count | 3 to 32 | `3` | How many copies make the spectrum, from red to blue in equal steps of scale: 3 gives a red, a green and a blue copy, as a lens's colour fringe does; more copies fill in yellow and cyan between them and turn the fringe into a smooth rainbow, but cost more time. Fewer copies than the amount needs show as separate ghosts. |
| `x` | number | fraction | 0 to 1 | `0.5` | The centre's horizontal position as a fraction of the way across the picture: 0 = the first pixel column, 0.5 = the middle, 1 = the last. |
| `y` | number | fraction | 0 to 1 | `0.5` | The centre's vertical position as a fraction of the way down the picture: 0 = the first pixel row, 0.5 = the middle, 1 = the last. |

<details><summary>Advanced parameters (2)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `interpolation` | one of 'nearest', 'bilinear', 'bicubic' |  |  | `'bilinear'` | How a point between pixels is read from a copy: 'nearest' takes the closest pixel (fastest, but the fringes step), 'bilinear' blends the four pixels around it (smooth, slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest, a little slower, and may ring beside hard edges). |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What the copies see beyond the picture's edge (the bluer ones read from farther out than they show): 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge). |

</details>

Presets (`preset="name"` in a recipe step or `fx`):

| Preset | Description |
| --- | --- |
| `prism` | Rainbow dispersion: seven copies of the picture fan out into a smooth spectrum fringe on every edge. |

### `dehaze`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `amount` | number | fraction | 0 to 1 | `0.9` | How much of the haze is taken out (He, Sun and Tang's omega): 0 = off, 1 = all of it that the dark channel finds, which also flattens real distance cues; a little below 1 keeps the picture natural. |
| `patch` | whole number | px | 1 to 201 | `15` | Side of the square patch in which the darkest value is taken to find the haze, in px at the recipe's size (an even size is centred half a pixel off). Larger patches estimate the haze more reliably but blur it over bigger areas and miss thin haze; 1 judges every pixel by its own darkest channel. |
| `radius` | number | px | 0 to 500 | `30.0` | Radius of the guided filter that refines the haze estimate to follow the picture's edges, in px at the recipe's size (rounded to whole px). Larger gives a smoother haze with fewer halos far from edges; 0 keeps the blocky patch estimate. |
| `airlight_fraction` | number | fraction | 0.0001 to 0.1 | `0.001` | The share of the picture's pixels, those with the highest dark channel (the haziest), among which the brightest one is taken as the colour of the haze (He, Sun and Tang use 0.1 percent = 0.001; at least one pixel is always used). Raising it makes the estimate less sensitive to a single odd pixel but can pick a bright object instead of the sky. |

<details><summary>Advanced parameters (3)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `edge_threshold` | number | fraction | 0.001 to 1 | `0.03` | The brightness step (0..1, Rec. 709 luma) that counts as an edge the refined haze estimate must not blur across: steps above it keep the haze of their own side, steps below it are smoothed. Raising it gives a smoother estimate and halos around edges; lowering it follows ever smaller edges. |
| `transmission_floor` | number | fraction | 0.01 to 1 | `0.1` | The lowest transmission (He, Sun and Tang's t0), the share of a pixel's light that reaches the camera: colours are stretched by at most 1 / transmission_floor, so a low floor clears thick haze but amplifies noise and clips; 1 allows no stretch at all. |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'nearest'` | What the darkest-value search sees beyond the picture's edge, which sets the haze estimate along the border (dehaze filters by local darkest values rather than warping, so the default is 'nearest', not 'reflect'): 'nearest' (the edge pixel repeated) and 'reflect' (mirrored, edge pixel repeated) look only at pixels of the patch inside the picture and give the same result; 'mirror' (mirrored about the edge pixel) also looks at pixels up to half a patch from the border; 'wrap' (the opposite edge) lets the far side's darkest values set the haze at the border. |

</details>

### `focus_blur`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `field` | field name |  |  | `'depth'` | The selection the focus is measured on: `depth` (1 = nearest: a lens focused in the scene), `light`, `subject`, or a region of the recipe such as a linear gradient (a band of focus across the picture, the miniature look) or a radial one. Its value at a pixel is compared with `focus`. |
| `focus` | number | fraction | 0 to 1 | `0.8` | The value of the field that stays sharp, as a fraction of its range: for `depth` 1 = the nearest and 0 = the farthest. Raising it moves the focus towards the field's high values; driving it moves the focus through the scene. |
| `width` | number | fraction | 0 to 1 | `0.15` | How wide a band of the field's values around the focus stays exactly as it is, as a fraction of its range. Raising it keeps more of the picture sharp. |
| `feather` | number | fraction | 0.01 to 1 | `0.4` | How far in the field's values beyond the band's edge the blur takes to reach its full radius, as a fraction of its range: the blur grows evenly across it. Raising it makes a gentler fall-off, lowering it an abrupt one. |
| `radius` | number | px | 0 to 100 | `10.0` | The Gaussian sigma in px at the recipe's size that the blur reaches at the full distance from the focus (a further `feather` from the band's edge): larger blurs the out-of-focus picture more; 0 = no blur. |

<details><summary>Advanced parameters (3)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `levels` | whole number | count | 1 to 16 | `6` | How many blurred copies of the picture the blur is built from: copy k has the sigma radius x k / levels, and each pixel mixes the two copies (or the sharp picture and the first) nearest its distance from the focus. More is smoother between levels and slower; 1 mixes the sharp picture with one blur. |
| `truncate` | number | ratio | 1 to 10 | `4.0` | Where each blurred copy's Gaussian is cut off, in sigmas: it reaches truncate x its sigma px. Larger is more exact and slower. |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What the blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated) or 'wrap' (the opposite edge). |

</details>

### `gaussian_blur`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `radius` | number (drivable) | px | 0 to 500 | `4.0` | The Gaussian's standard deviation (sigma) in px at the recipe's size: larger is softer and spreads detail further; 0 leaves the picture unchanged. |

<details><summary>Advanced parameters (3)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `truncate` | number | ratio | 1 to 10 | `4.0` | Where the Gaussian is cut off, in sigmas: the blur reaches truncate x radius px. Larger is more exact and slower; below 3 the blur turns visibly boxy. |
| `edge` | one of 'reflect', 'constant', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What the blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge), 'constant' (constant_value, so the edge fades toward it). |
| `constant_value` | number | fraction | 0 to 1 | `0.0` | The brightness (0..1, every channel) assumed beyond the edge when edge is 'constant': 0 darkens the edge, 1 lightens it; ignored otherwise. |

</details>

### `highpass`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `radius` | number | px | 0 to 100 | `4.0` | Gaussian sigma of the blurred copy in px at the recipe's size: detail finer than this is kept, coarser tones are removed. Small radii keep only the finest texture, large ones keep edges and local contrast; 0 keeps nothing but mid-grey. |

<details><summary>Advanced parameters (2)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `truncate` | number | ratio | 1 to 10 | `4.0` | Where the blur's Gaussian is cut off, in sigmas: the blur reaches truncate x radius px. Larger is more exact and slower. |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What the blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge). |

</details>

### `lens_blur`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `radius` | number | px | 0 to 100 | `8.0` | The radius of the disc in px at the recipe's size: larger is further out of focus and spreads each point into a bigger circle; 0 leaves the picture unchanged. |
| `highlights` | number | ratio | 0 to 50 | `0.0` | How much brighter pixels count in the average: a pixel of brightness b (0..1) counts 1 + highlights x b times as much as a black one, so with 0 every pixel counts the same (a plain average), and raising it turns bright lights into distinct, luminous discs (bokeh) while dark areas hardly bleed into them. |

<details><summary>Advanced parameters (1)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What the blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge). |

</details>

### `lens_flare`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `x` | number | fraction | 0 to 1 | `0.75` | The light's horizontal position as a fraction of the way across the picture: 0 = the first pixel column, 0.5 = the middle, 1 = the last. |
| `y` | number | fraction | 0 to 1 | `0.25` | The light's vertical position as a fraction of the way down the picture: 0 = the first pixel row, 0.5 = the middle, 1 = the last. |
| `ghosts` | whole number | count | 0 to 12 | `5` | How many ghosts (soft discs, the lens's reflections of the light) lie along the line from the light through the picture's centre; each is dimmer than the one before, the last 1/ghosts as bright as the first. 0 = none. |
| `size` | number | px | 1 to 300 | `30.0` | The radius in px of each ghost, tuned for a picture about 480 px high: the disc is brightest at its middle and falls smoothly to nothing at this distance. Raising it makes bigger, softer ghosts. |
| `halo` | number | px | 0 to 600 | `120.0` | The radius in px of the soft glow round the light, tuned for a picture about 480 px high: brightest at the light and falling smoothly to nothing at this distance. 0 = no halo. |
| `ring` | number | px | 0 to 600 | `72.0` | The radius in px of the thin ring round the light, tuned for a picture about 480 px high: the distance from the light at which the ring is brightest. 0 = no ring. |
| `amount` | number | ratio | 0 to 10 | `0.8` | How strongly the flare is screened over the picture: 0 = off, 1 = the light at full brightness where it is brightest (the light itself and the first ghost); above 1 it saturates to white sooner. |
| `color` | colour |  |  | `'#ffe2b8'` | The tint of the flare: '#rrggbb', '#rgb' or [r, g, b] 0..1. The default is a warm white; white keeps the flare colourless. |

<details><summary>Advanced parameters (2)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `spacing` | number | ratio | 0 to 3 | `0.45` | Where the ghosts lie on that line, as multiples of the distance from the light to the picture's centre: ghost number i sits i x spacing of the way from the light toward the centre. 1 puts the first on the centre, 2 mirrors the light through it, more goes beyond; 0 piles them on the light. A light at the centre gives them no line, so they sit on it. |
| `ring_width` | number | px | 1 to 300 | `12.0` | How far each side of its radius the ring reaches, in px tuned for a picture about 480 px high: it falls smoothly from full brightness on the radius to nothing this far off. Raising it makes a wider, softer ring. |

</details>

### `light_leak`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `color` | colour |  |  | `'#ff8a3d'` | The tint of the light: '#rrggbb', '#rgb' or [r, g, b] 0..1. The default is the orange of a leaking film back; red or yellow give other leaks. |
| `amount` | number | ratio | 0 to 2 | `0.8` | How strongly the light is screened over the picture: 0 = off, 1 = the brightest patch at full brightness in `color`; above 1 it saturates to white sooner. |
| `size` | number | px | 6 to 600 | `150.0` | How big the patches are and how deep they reach into the picture, in px tuned for a picture about 480 px high: each patch is a soft bump whose width is between half of this and all of it, centred between the edge and this far in. Raising it floods more of the picture. |
| `angle` | number (cyclic) | degrees | 0 to 360 | `0.0` | The side the light comes in from, as the direction from the picture's centre, in degrees clockwise from pointing right, as seen on screen: 0 = the right edge, 90 = the bottom, 180 = the left, 270 = the top; in between, the corner. |
| `blobs` | whole number | count | 1 to 16 | `3` | How many patches of light there are, at random places along that side (each brighter or fainter and stretched along the edge by chance). Overlapping patches merge into one wash; raising it gives a more even, wider leak. |
| `seed` | whole number | count | 0 to 4294967295 | `0` | Random seed: the same seed always gives the same leak (the same patches in the same places), another seed a different one. |

### `light_rays`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `x` | number | fraction | 0 to 1 | `0.5` | The light's horizontal position as a fraction of the way across the picture: 0 = the first pixel column, 0.5 = the middle, 1 = the last. The rays point away from it. |
| `y` | number | fraction | 0 to 1 | `0.3` | The light's vertical position as a fraction of the way down the picture: 0 = the first pixel row, 0.5 = the middle, 1 = the last. The rays point away from it. |
| `length` | number | fraction | 0 to 1 | `0.5` | How far the rays reach back toward the light, as a fraction of each pixel's distance from it: a pixel gathers light from the line between itself and the light, as far as this share of the way (0.5 = half way, 1 = all the way to the light). A highlight streaks outward from itself up to 1 / (1 - length) times its distance from the light; 0 gives no rays. |
| `threshold` | number | fraction | 0 to 0.99 | `0.7` | The brightness (0..1, per channel) where light starts: only values above it make rays, rising to full at 1. Raising it limits the rays to the brightest highlights. |
| `amount` | number | ratio | 0 to 10 | `2.5` | How strongly the rays are screened over the picture: 0 = off, 1 = the rays at full brightness; above 1 they saturate to white sooner. The rays are an average along the pixel's line to the light, so a highlight far from the pixel gives them much less than the highlight itself: a few times 1 is usual. |
| `decay` | number | ratio | 0 to 1 | `0.9` | How much less each reading counts than the one nearer the pixel (the weights are decay^0, decay^1... scaled to add up to 1): 1 = all alike, rays of even brightness to their tips; lower values make them fade away from the highlight; 0 = no rays at all. |

<details><summary>Advanced parameters (3)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `samples` | whole number | count | 1 to 256 | `8` | How many readings of the picture along each ray are added up, spread evenly over its length: more is smoother and slower (the time grows with it); few show as separate ghost copies of a highlight. With 1 the highlights only brighten themselves. |
| `interpolation` | one of 'nearest', 'bilinear', 'bicubic' |  |  | `'bilinear'` | How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, but steps and ghosts show), 'bilinear' blends the four pixels around it (smooth, slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest, a little slower). |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What a reading sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge). The readings lie between a pixel and the light, so they leave the picture only when the light itself is outside it; this rule then matters at the border for 'bicubic' only, whose spline fit reads it. |

</details>

### `motion_blur`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `angle` | number (cyclic) | degrees | 0 to 360 | `0.0` | The direction of the motion, in degrees clockwise from pointing right on the screen, 0 to 360: 0 = horizontal, 90 = vertical, 45 = falling to the lower right. Opposite directions (0 and 180) give the same blur. |
| `length` | number (drivable) | px | 0 to 500 | `24.0` | How far the smear reaches along the line, end to end, in px, tuned for a picture about 480 px high: larger is a faster motion; 0 leaves the picture unchanged. |

<details><summary>Advanced parameters (1)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What the blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge). |

</details>

### `radial_blur`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `mode` | one of 'spin', 'zoom' |  |  | `'zoom'` | The direction of the smear: 'spin' averages along the circle around the centre, as a rotating camera or wheel would blur; 'zoom' averages along the ray through the centre, as zooming the lens during the exposure would. The centre itself stays sharp in both. |
| `amount` | number | fraction | 0 to 1 | `0.1` | How far each pixel is smeared. 'spin': the arc, as a fraction of a full turn (0.1 = 36 degrees, 1 = a whole circle), centred on the pixel. 'zoom': the range of scales, as a fraction of the pixel's distance from the centre (0.1 = from 5% closer to 5% farther, 1 = from half to one and a half times the distance). Larger is a longer smear and stronger at the edges; 0 leaves the picture unchanged. |
| `x` | number | fraction | 0 to 1 | `0.5` | The centre's horizontal position as a fraction of the way across the picture: 0 = the first pixel column, 0.5 = the middle, 1 = the last. |
| `y` | number | fraction | 0 to 1 | `0.5` | The centre's vertical position as a fraction of the way down the picture: 0 = the first pixel row, 0.5 = the middle, 1 = the last. |

<details><summary>Advanced parameters (3)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `samples` | whole number | count | 1 to 256 | `8` | How many points along each arc or ray are averaged, spread evenly over its length and read from the picture by the chosen interpolation: more is smoother and slower (the time grows with it), and a smear of more than a few pixels per sample shows as separate ghost copies. With 1 the picture is unchanged. |
| `interpolation` | one of 'nearest', 'bilinear', 'bicubic' |  |  | `'bilinear'` | How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps every value exactly as it is but steps and ghosts show in smooth areas), 'bilinear' blends the four pixels around it (smooth, slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest and most exact on smooth gradients, a little slower, and may ring beside hard edges). |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What the smear sees beyond the picture's edge (arcs and rays near the corners leave it): 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge). |

</details>

### `relight`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `field` | field name |  |  | `'depth'` | The field read as the relief, the higher the value the higher the ground: `depth` (the nearest stands highest; the subject rises from its background), `light` (the picture's own brightness: bright parts stand proud), `subject`, or a region the recipe declares. |
| `angle` | number (cyclic) | degrees | 0 to 360 | `225.0` | Where the light comes from, in degrees clockwise from pointing right, on screen: 0 = the right, 90 = below, 180 = the left, 225 = the upper left, 270 = above. Sides that face it are brightened, those that face away darkened. |
| `elevation` | number | degrees | 1 to 90 | `45.0` | How high the light stands above the picture's plane, in degrees: 90 = straight above (every slope only dims a little, whatever `angle`), 45 = the usual raking light, 1 = grazing, so the shapes are dramatic and the lit side is clipped to white. |
| `height` | number | px | 0 to 1000 | `200.0` | How tall the whole range of the field stands as a relief, in px at the recipe's size: a field value of 1 stands this many px above 0, so the slope between neighbours is their difference times this. 0 = flat (the relief is nothing: the picture at `saturation` 1, mid grey at 0), larger = steeper shapes that catch more of the light. A field of the picture's own brightness wants far less (about 8) than a depth (about 200). |
| `blur` | number | px | 0 to 50 | `2.0` | Gaussian sigma in px with which the field is smoothed before its slopes are taken: 0 = every ripple of the field is lit (its noise shows), larger = softer lighting that follows only the broad shapes. |
| `saturation` | number | fraction | 0 to 1 | `1.0` | How much of the picture's colour is lit: 1 = the picture itself, lit; 0 = the relief alone, a grey surface lit (a flat field is mid grey); between, the picture mixed into mid grey. |

<details><summary>Advanced parameters (2)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `truncate` | number | ratio | 1 to 10 | `4.0` | Where the smoothing Gaussian is cut off, in sigmas (only matters when `blur` > 0). Larger is more exact and slower. |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What the smoothing and the slope see beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge, so a tileable picture lights without a seam). |

</details>

Presets (`preset="name"` in a recipe step or `fx`):

| Preset | Description |
| --- | --- |
| `bump_light` | The picture's own brightness read as a fine relief (8 px tall) and lit from the upper left. |
| `emboss` | An embossed print: the picture's brightness as a 6 px relief in grey, lit from the upper left at 30 degrees. |

### `sharpen`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `radius` | number | px | 0 to 100 | `2.0` | Gaussian sigma of the blurred copy in px at the recipe's size: small radii sharpen fine texture, large ones add local contrast around coarse edges; 0 leaves the picture unchanged. |
| `amount` | number (drivable) | ratio | 0 to 5 | `0.8` | How much of the detail is added back: 0 = off, 1 = the full difference from the blurred copy; above 1 edges overshoot and halos show. |
| `threshold` | number | fraction | 0 to 1 | `0.0` | The smallest difference (0..1, per channel) from the blurred copy that is sharpened: smaller differences are left alone, which keeps grain and smooth skin from being sharpened; 0 sharpens everything. |

<details><summary>Advanced parameters (3)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `truncate` | number | ratio | 1 to 10 | `4.0` | Where the blur's Gaussian is cut off, in sigmas: the blur reaches truncate x radius px. Larger is more exact and slower. |
| `edge` | one of 'reflect', 'constant', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What the blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge), 'constant' (constant_value, so the border stands out from it). |
| `constant_value` | number | fraction | 0 to 1 | `0.0` | The brightness (0..1) assumed beyond the edge when edge is 'constant'; ignored otherwise. |

</details>

Presets (`preset="name"` in a recipe step or `fx`):

| Preset | Description |
| --- | --- |
| `clarity` | Local contrast: features stand out against a wide (30 px) blur of their surroundings. |

### `starburst`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `rays` | whole number | count | 1 to 32 | `4` | How many spikes leave each bright point, evenly spread round it: 4 = a cross, 6 = a six-pointed star, 1 = a single comet tail. Raising it makes a denser star. |
| `length` | number | px | 0.5 to 600 | `60.0` | How far each spike reaches, in px tuned for a picture about 480 px high: it fades linearly from the point to nothing at this distance. Raising it makes longer, fainter-tipped spikes. |
| `threshold` | number | fraction | 0 to 0.99 | `0.95` | The brightness (0..1, per channel) where a spike starts: only values above it make light, rising to full at 1. Raising it limits the spikes to the brightest points. Every pixel above it sends out spikes, so at a low threshold a large bright area (a window, a sky, white fur) floods the picture with light: raise the threshold, or cap the number of points with `limit`. |
| `amount` | number | ratio | 0 to 10 | `0.8` | How strongly the spikes are screened over the picture: 0 = off, 1 = a spike as bright as the point that makes it, at the point; above 1 it saturates to white sooner. |
| `angle` | number (cyclic) | degrees | 0 to 360 | `0.0` | The direction of the first spike, in degrees clockwise from pointing right, as seen on screen: 0 = to the right, 90 = down. The other spikes follow at equal angles, so with 4 spikes 45 turns the cross into an X. |
| `spacing` | number | px | 0 to 200 | `0.0` | The average distance in px, tuned for a picture about 480 px high, between the points that send out spikes inside a bright area: each bright pixel sends them with the chance 1 / spacing^2 (the same ones for the same `seed`; raising it only drops points), so the glints keep their number per area of the picture at any size. 0, or anything below 1, = every bright point, each with its share of a design pixel's light, so a bright area gives the same total light at any size. Raise it to turn a bright area into separate glints; 13 gives a scatter of glitter. |
| `color` | colour |  |  | `'#ffffff'` | The tint of the spikes: '#rrggbb', '#rgb' or [r, g, b] 0..1. White keeps the colour of the light that makes them. |

<details><summary>Advanced parameters (4)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `width` | number | px | 1 to 20 | `1.0` | The thickness in px of each spike, tuned for a picture about 480 px high, with a soft edge one px wide: 1 = a hairline. Raising it makes bolder spikes. |
| `limit` | number | fraction | 0 to 1 | `1.0` | The most pixels that send out spikes, as a share of all the picture's pixels, the brightest first: the threshold is raised just enough to keep to it (never above 0.99, so a picture with more than this share of near-white pixels still lights those). 1 = no cap, 0.001 = at most one pixel in a thousand, 0 = none. Lower it to keep a low `threshold` from flooding large bright areas. |
| `variation` | number | fraction | 0 to 1 | `0.0` | How much the points differ in brightness, and so in how far their spikes show: each one's light is scaled by a random number from 1 - `variation` to 1. 0 = all alike, 1 = from full down to nothing. |
| `seed` | whole number | count | 0 to 4294967295 | `0` | Random seed for `spacing` and `variation`: the same seed picks the same points of the same picture, another seed others. |

</details>

Presets (`preset="name"` in a recipe step or `fx`):

| Preset | Description |
| --- | --- |
| `sparkle` | Scattered glitter: small white four-pointed glints on a random few of the bright points, of varied brightness. |

### `tone_map`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `contrast` | number | ratio | 0 to 1 | `0.5` | How much of the broad brightness differences between areas is kept: 1 = all of them (no compression), 0.5 = halved in stops (a picture with dark areas three stops below its bright ones gets them one and a half apart), 0 = every area brought up to white and only the detail left. Lowering it lightens dark areas the most; white stays white. |
| `amount` | number | ratio | 0 to 5 | `0.5` | How strongly the fine detail (what is left of the brightness once the broad areas are taken out) is exaggerated: 0 = kept as it is, 1 = doubled in stops. Raising it brings out texture; it can clip highlights. |
| `pivot` | number | stops | 0 to 8 | `0.0` | The brightness, in stops below white, that compression leaves where it is: 0 = white (everything is lightened, nothing darkened); 2 = a quarter of white's brightness (about 0.25), where areas brighter than it are darkened and darker ones lightened, recovering highlights as well as shadows. Raising it darkens bright areas more and lightens dark ones less. |
| `radius` | number | px | 0 to 500 | `30.0` | Size of what counts as a broad area, in px at the recipe's size (rounded to whole px): the base layer is an average over a square of 2 x radius + 1 px. Larger treats bigger structures as base and keeps only finer ones as detail; 0 makes every pixel its own base, a plain global curve (a grey v becomes v^contrast). |

<details><summary>Advanced parameters (1)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `edge_threshold` | number | stops | 0.01 to 8 | `0.5` | The brightness step, in stops, that counts as an edge the base layer must not blur across: steps above it keep their own side of the base, which avoids halos around them; steps below it are averaged away and count as detail. Raising it makes the base smoother and more like a plain blur. |

</details>

Presets (`preset="name"` in a recipe step or `fx`):

| Preset | Description |
| --- | --- |
| `shadows_highlights` | Shadows and highlights recovery: dark areas lifted and bright areas tamed by their local surroundings, texture kept. |

### `vignette`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `amount` | number (drivable) | fraction | 0 to 1 | `0.5` | How far the edges are mixed toward the colour where the vignette is fullest: 0 = off, 0.5 = halfway, 1 = the colour itself at the corners. |
| `radius` | number | fraction | 0 to 1 | `0.5` | Where the vignette starts, as a fraction of the distance from the centre to a corner: everything inside is untouched. Raising it clears more of the picture; 1 leaves even the corners alone. |
| `feather` | number | fraction | 0 to 1 | `0.5` | How far beyond the radius the vignette takes to reach full strength, as a fraction of the distance from the centre to a corner (a smooth S-curve). 0 = a hard edge; raising it softens the transition. |
| `roundness` | number | fraction | 0 to 1 | `1.0` | The shape of the vignette's contours: 1 = an ellipse that follows the picture's proportions, 0 = a rectangle, which darkens the middle of each edge as much as a corner; in between, a rounded rectangle. |
| `color` | colour |  |  | `'#000000'` | The colour the edges fade to: '#rrggbb', '#rgb' or [r, g, b] 0..1. Black darkens, white lightens. |
