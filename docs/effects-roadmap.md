# Effects roadmap

What bullterrier-image can do today and what could come next. **✓ marks what exists now** (`bt-image effects` lists it with every
parameter); everything else is a proposal. Examples use the Python form `fx.<effect>(...)`; in a recipe file the same call
is `effect = "<effect>"` plus the same parameters.

Only looks that read as made, not as a one-click photo filter, are proposed. Cut: oil paint, sketch, toon, comic,
watercolor, woodcut, pop art, film emulation, pixel and brick novelties, novelty text looks and reveal wipes.

Looks that are one effect with a set of values ship as presets of that effect (`fx.gradient_map(preset="sepia")`, data in
`src/bullterrier_image/presets/`) rather than as separate code; looks that combine several effects (VHS, Polaroid, pop art) are
recipes. A new look first extends an existing effect when every one of its parameters still matters for every look;
it becomes a new effect only when it is a new idea. Planned merges of today's effects: "Planned consolidation", at the end.

**Next:** `lut`, then `edges`. After them, effects are added only when a video look needs one.

## Colour and tone

| Effect | What it does | Example |
| --- | --- | --- |
| `grade` ✓ | Exposure, contrast, saturation, vibrance (muted colours rise more than vivid ones). | `fx.grade(exposure=0.3, vibrance=0.5)` |
| `gradient_map` ✓ | Recolour by brightness with any gradient. | `fx.gradient_map(stops=["#000","#f0f"])` |
| `split_tone` ✓ | Tint shadows and highlights. | `fx.split_tone(shadow="#3a1bff")` |
| `hue_shift` ✓ | Rotate hues, keep brightness. | `fx.hue_shift(hue=150)` |
| `levels` ✓ | Black point, white point, gamma, output range (`output_black` lifts the blacks for a matte film look). | `fx.levels(black=0.05, white=0.95, gamma=1.1)` |
| `curves` ✓ | Tone curve through control points. | `fx.curves(shadows=0.18, highlights=0.82)` |
| `auto_levels` ✓ | Stretch to the full tonal range. | `fx.auto_levels(clip=0.005)` |
| `equalize` ✓ | Adaptive histogram equalisation (CLAHE-style). | `fx.equalize(tiles=8, limit=2.0)` |
| `white_balance` ✓ | Fix colour cast by temperature/tint or a grey point. | `fx.white_balance(temperature=-0.2)` |
| `selective_color` ✓ | Shift hue/saturation/lightness of one colour range. | `fx.selective_color(hue=30, saturation=0.6)` |
| `color_balance` ✓ | Separate RGB shifts for shadows, mids, highlights. | `fx.color_balance(highlights_red=0.05, highlights_blue=-0.05)` |
| `channel_mixer` ✓ | 3x3 matrix between channels. | `fx.channel_mixer(red_from_red=0, red_from_green=1, green_from_red=1, green_from_green=0)` |
| `invert` ✓ (a preset of `levels`) | Negative. | `fx.levels(preset="invert")` |
| `posterize` ✓ | Few tone levels per channel. | `fx.posterize(levels=4)` |
| `solarize` ✓ | Invert above a threshold. | `fx.solarize(threshold=0.5)` |
| `threshold` ✓ | Two-colour bitmap. | `fx.threshold(level=0.5, dark="#000", light="#fff")` |
| `color_transfer` | Match the colour mood of a reference photo. | `fx.color_transfer(reference="ref.jpg")` |
| `lut` (next) | Apply a `.cube` 3D colour lookup table: how grades are handed over. | `fx.lut(file="film.cube", amount=0.8)` |
| `black_and_white` ✓ (a preset of `channel_mixer`) | Mono conversion; set the rows for a colour filter. | `fx.channel_mixer(preset="monochrome")` |
| `sepia` ✓ (a preset of `gradient_map`) | Warm monochrome. | `fx.gradient_map(preset="sepia")` |
| `cyanotype` ✓ (a preset of `gradient_map`) | Prussian-blue print. | `fx.gradient_map(preset="cyanotype")` |
| `cross_process` ✓ (a preset of `curves`) | Shifted curves per channel. | `fx.curves(preset="cross_process")` |
| `teal_orange` ✓ (a preset of `split_tone`) | The blockbuster grade. | `fx.split_tone(preset="teal_orange", amount=0.4)` |
| `thermal` ✓ (a preset of `gradient_map`) | Heat-camera false colour. | `fx.gradient_map(preset="thermal")` |
| `infrared` ✓ (a preset of `channel_mixer`) | False-colour infrared (swapped channels, bright foliage). | `fx.channel_mixer(preset="infrared")` |
| `xray` ✓ (a preset of `gradient_map`) | Inverted and blue (not edge-lit). | `fx.gradient_map(preset="xray")` |
| `night_vision` ✓ (a preset of `gradient_map`) | Green phosphor (no noise or vignette: add `grain` and `vignette`). | `fx.gradient_map(preset="night_vision")` |

## Light, focus and detail

| Effect | What it does | Example |
| --- | --- | --- |
| `sharpen` ✓ | Unsharp mask; preset `clarity` is local contrast. | `fx.sharpen(radius=2, amount=0.8)` |
| `highpass` ✓ | Keep only fine detail (for blending). | `fx.highpass(radius=8)` |
| `denoise` (deferred) | Median, bilateral or non-local-means (the default method fits the render budget; a slower one may be chosen). | `fx.denoise(method="bilateral", amount=0.4)` |
| `lens_blur` aperture (planned) | `blades` on `lens_blur`: hexagonal or cat's-eye bokeh; 0 = a disc. | `fx.lens_blur(radius=12, highlights=4, blades=6)` |
| `shadows_highlights` ✓ | Recover dark and bright areas. | `fx.shadows_highlights(shadows=0.4)` |
| `dehaze` ✓ | Remove atmospheric haze. | `fx.dehaze(amount=0.5)` |
| `tone_map` ✓ | Local tone mapping, the "HDR look". | `fx.tone_map(amount=0.7)` |
| `vignette` ✓ | Darken (or colour) the corners. | `fx.vignette(amount=0.5, roundness=0.8)` |
| `gaussian_blur` ✓ | Blur. | `fx.gaussian_blur(radius=6)` |
| `motion_blur` ✓ | Directional blur. | `fx.motion_blur(angle=30, length=40)` |
| `radial_blur` ✓ | Spin or zoom blur from a point. | `fx.radial_blur(mode="zoom", amount=0.1)` |
| `lens_blur` ✓ | Disc-shaped bokeh blur. | `fx.lens_blur(radius=12, highlights=0.6)` |
| `bloom` ✓ | Multi-scale glow; `octaves=1` is a single soft halo; presets `soft_focus` (Orton glow of the whole picture) and `halation` (film's red-orange glow). | `fx.bloom(radius=40, threshold=0.7)` |
| `anamorphic_streaks` ✓ | Horizontal flare streaks off highlights. | `fx.anamorphic_streaks(length=0.3)` |
| `starburst` ✓ | Diffraction spikes on bright points. | `fx.starburst(rays=6, length=0.15)` |
| `lens_flare` ✓ | Flare with ghost reflections. | `fx.lens_flare(x=0.8, y=0.2)` |
| `light_rays` ✓ | God rays from a point or the highlights. | `fx.light_rays(x=0.7, y=0.1, length=0.5)` |
| `chromatic_aberration` ✓ | Radial red/blue fringing; `samples` fans it into a rainbow (preset `prism`). | `fx.chromatic_aberration(amount=4)` |
| `light_leak` ✓ | Warm coloured light washes . | `fx.light_leak(color="#ff7a2b")` |
| `focus_blur` ✓ | Blur by how far a `field` (default `depth`: the model, or a map you supply) lies from `focus`; a band around it stays sharp. A linear-gradient `field` and a `grade` driven by the band is the miniature look ([focus_blur.toml](../examples/effects/focus_blur.toml)). | `fx.focus_blur(focus=0.58, radius=8)` |

## Texture and finish

| Effect | What it does | Example |
| --- | --- | --- |
| `grain` ✓ | Film grain or sensor noise: Gaussian or uniform, heavier in the shadows or highlights, grey or per channel. | `fx.grain(amount=0.065, balance=-0.54, color=True)` |
| `scanlines` ✓ | Horizontal line pattern, hard or soft (`softness` 1 is a CRT's sin^2). | `fx.scanlines(spacing=3, darkness=0.3)` |
| `pixel_grid` ✓ | LCD sub-pixel grid; `keep_brightness` for a shadow mask that does not dim. | `fx.pixel_grid(cell=4)` |
| `dust_scratches` ✓ | Specks, hairs and scratches. | `fx.dust_scratches(density=0.3, seed=1)` |
| `paper_texture` ✓ | Procedural paper, canvas or linen surface. | `fx.paper_texture(kind="canvas", amount=0.3)` |
| `crt` ✓ (a recipe, [crt.toml](../examples/recipes/crt.toml)) | Bulged glass in a bezel, bloom, soft scanlines, shadow mask, vignette. | `lens_distortion` (`fit` centre, `border`), `bloom`, `scanlines` (`softness` 1), `pixel_grid` (`gap` 0, `keep_brightness`), `vignette` |
| `vhs` ✓ | Tracking bands, chroma bleed, tape noise, wobble. | `fx.vhs(tracking=0.06, bleed=5)` |
| `ntsc` ✓ | Chroma smear and dot crawl. | `fx.ntsc(sigma=4)` |
| `torn_edge` ✓ | Ragged paper edge. | `fx.torn_edge(roughness=0.6, seed=2)` |

Frames that need a canvas larger than the photo (a polaroid with caption space, a 35 mm strip with sprocket holes)
belong to a layout package, not here.

## Stylised looks

| Effect | What it does | Example |
| --- | --- | --- |
| `halftone` ✓ | Dot screen. | `fx.halftone(cell=10, angle=45)` |
| `seams` ✓ | Solid line or band along every border between a partition's pieces. | `fx.seams(partition="glass", width=0.0016)` |
| `evaporate` ✓ | A still of pieces peeling off the top or outline. | `fx.evaporate(start=0.4, outline=0.1)` |
| `ascii` ✓ | Text rendering. | `fx.ascii(cell=6)` |
| `quadtree` ✓ | Cells split only where the picture has detail: fine blocks on faces and edges, large flat ones elsewhere. Optional `gap` and rounded corners; fine detail only on the subject is two steps, one masked. | `fx.quadtree(threshold=0.09, max_cell=0.2, min_cell=0.012)` |
| `dither` ✓ | Floyd-Steinberg, Atkinson, Bayer or blue-noise dots to a palette found in the photo, a named one (`gameboy`, `cga`, `ega`, `pico8`) or your own, on a grid of any size; `pattern=0` is plain palette reduction. | `fx.dither(method="blue_noise", palette=["#0b1026", "#ff5a36", "#f7e7c6"], cell=0.008)` |
| `edges` (next) | Outlines by Sobel, Canny or difference of Gaussians; on the existing `ops.edges`. | `fx.edges(method="dog", width=2)` |
| `crosshatch` | Layers of hatch lines by tone. | `fx.crosshatch(layers=4, angle=45)` |
| line and CMYK screens (planned, on `halftone`) | `shape` = dot or line (lines whose width carries tone); `separation` = none or cmyk (four inks at the screen angles). Replaces `line_halftone` and `cmyk_halftone`. | `fx.halftone(shape="line", cell=8, angle=30)`, `fx.halftone(separation="cmyk", cell=9)` |
| `engraving` | Banknote-style flowing lines. | `fx.engraving(spacing=5)` |
| `stipple` | Dots whose density carries tone (weighted Voronoi). | `fx.stipple(dots=20000, seed=1)` |
| `voronoi_mosaic` ✓ (`shatter` with `fill` and `detail`) | Stained-glass cells that gather along the picture's edges, each one flat colour, leaded. [Recipe](../examples/recipes/stained-glass.toml). | `fx.shatter(pieces=420, offset=0, angle=0, detail=0.85, fill=1, gap=2)` |
| `low_poly` ✓ (`shatter` with `shape="triangles"`) | Delaunay triangles, one colour each, more of them where the picture has edges. [Recipe](../examples/recipes/low-poly.toml). | `fx.shatter(shape="triangles", pieces=480, offset=0, angle=0, detail=0.7, fill=1)` |
| `risograph` (a recipe, planned) | Spot-colour layers, misregistration, grain: `dither` or `halftone` with inks, each offset, then `grain`. Needs inks with a per-ink offset on those two. | `fx.halftone(inks=["#ff4d6d","#2b59ff"], ...)` |

## Computational and experimental

Looks made by running an algorithm on the picture rather than filtering it. The iterative one (`reaction_diffusion`)
takes an explicit step count so it stays within the render budget.

| Effect | What it does | Example |
| --- | --- | --- |
| `liquid_metal` ✓ | Chrome: the picture's brightness as a relief, lit and reflecting a fake environment, in a metal gradient. | `fx.liquid_metal(relief=35, tint="#c9d6ff")` |
| `distance_field` | Signed-distance-field shapes from the picture's silhouette or brightness: soft outlines, glow, contour bands, metaballs. | `fx.distance_field(source="luminance", level=0.5, bands=8)` |
| `glass` (after `distance_field`) | The picture refracted through a region's shape with a lit bevel: fluted or ribbed glass, glass blocks, "liquid glass" over a logo, text or the subject. May absorb `lenticular` (fluted glass). | `fx.glass(kind="fluted", refraction=1.4)` with `partition = "logo"` |
| `reaction_diffusion` ✓ | Gray-Scott growth seeded by the picture: spots, stripes, coral. | `fx.reaction_diffusion(feed=0.037, kill=0.06, steps=300)` |
| `lenticular` ✓ | Lenticular-print look: thin interlaced strips that shift with the viewing angle. | `fx.lenticular(strip=6, shift=5)` |

Dropped: `cellular_automata` (noisy specks, little value).

## Text and symbols

| Effect | What it does | Example |
| --- | --- | --- |
| glyph matching (planned, on `ascii`) | Pick each character by how its shape matches the cell rather than by its ink: edge characters (`/ \ \| _`), braille (2x4 dots) or half blocks (`▀▄`). Replaces `ascii_edges`, `braille` and `half_blocks`. | `fx.ascii(match="shape", chars="braille")` |
| text order (planned, on `ascii`) | The photo built from a supplied text, its characters in order, tone carried by colour. Replaces `text_mosaic`. | `fx.ascii(text=open("poem.txt").read())` |
| `typographic_portrait` (deferred) | Lines of text, weight carries tone (needs variable font weights). | `fx.typographic_portrait(text="...")` |

## Distortion and geometry

**Shape-preserving warps**

| Effect | What it does | Example |
| --- | --- | --- |
| `swirl` ✓ | Twist around a point. | `fx.swirl(angle=120, radius=0.5)` |
| `pinch_bulge` ✓ | Push in or out. | `fx.pinch_bulge(amount=0.5)` |
| `ripple` ✓ | Travelling wave displacement. | `fx.ripple(wavelength=40, amplitude=6, phase=0)` |
| `displace` ✓ | Move pixels by noise or a map; at `scale=1` with `interpolation="nearest"` each pixel moves on its own (frosted glass). | `fx.displace(amount=12, scale=80, seed=1)` |
| `lens_distortion` ✓ | Barrel or pincushion; `model` 'fisheye' is a wide-angle bulge. | `fx.lens_distortion(amount=0.2)` |
| `polar` ✓ | To or from polar coordinates (tiny planet). | `fx.polar(mode="to")` |
| `mirror` ✓ (`kaleidoscope` with 1 segment; 2 for four-way) | Symmetry on an axis. | `fx.kaleidoscope(segments=1, angle=90)` |
| `kaleidoscope` ✓ | N mirrored segments. | `fx.kaleidoscope(segments=8, angle=15)` |
| `tile` ✓ | Repeat the picture in a grid. | `fx.tile(columns=3, rows=3)` |
| `perspective` ✓ | Keystone, skew, turn and zoom. | `fx.perspective(vertical=0.3, shear_x=0.2, rotate=-3, scale=0.2)` |
| `pixel_sort` ✓ | Sort runs of pixels by brightness or hue. | `fx.pixel_sort(by="brightness", low=0.25, high=0.75)` |
| `melt` ✓ | Brightness-driven drips running down. | `fx.melt(amount=40, seed=1)` |
| `flow_warp` ✓ | Smeared by a noise flow field. | `fx.flow_warp(scale=80, amount=20, seed=1)` |
| `stretch` ✓ | Smear one pixel row or column outward. | `fx.stretch(axis="vertical", at=0.5)` |
| `shatter` ✓ | Cracked-glass pieces, slightly offset and rotated; or flat stained glass and low-poly triangles (`fill`, `detail`, `shape`). | `fx.shatter(pieces=24, gap=1.5, seed=1)` |
| `droste` ✓ | Recursive picture-in-picture spiral. | `fx.droste(scale=0.5)` |
| `slice_shift` ✓ | Horizontal slices slide (a subset of `glitch`). | `fx.slice_shift(slices=30, amount=0.05)` |

**Changing the picture's size and shape**

These are settings of the recipe's `[frame]` table, not effects (see "Size changes frame the photo before the steps" in
the [design decisions](design-decisions.md)): framing runs before the first step, on the photo and the subject matte
together, always in the order flip, rotate, crop, pad, size; transparency is cut after the last step. Lengths are shares
of the picture, so a look does not depend on the resolution. `Framing.documentation()` lists every field.

| Setting | What it does | Example |
| --- | --- | --- |
| crop ✓ | Crop to a width-over-height ratio around the middle or a side. | `aspect = 1.0`, `anchor = "top"` |
| smart crop ✓ | Crop around the subject with a margin. | `aspect = 0.8`, `anchor = "subject"`, `margin = 0.1` |
| resize ✓ | Scale to a longest side (the recipe's `size`). | `size = 1080` |
| pad ✓ | Extend the canvas with a colour, per side. | `pad_top = 0.1`, `fill = "#000000"` |
| rotate ✓ | Rotate, expanding the canvas or cropping to fit. | `angle = 3`, `fit = "crop"` |
| flip ✓ | Mirror the whole picture. | `flip = "x"` |
| transparent background ✓ | The subject on transparency. | `transparent = "background"` |
| circle crop ✓ | Circular cut with transparency. | `transparent = "circle"` |
| rounded corners ✓ | Rounded rectangle with transparency. | `transparent = "rounded"`, `corner_radius = 0.1` |

## Glitch and signal damage

| Effect | What it does | Example |
| --- | --- | --- |
| `glitch` ✓ | Channel split and sliding bands. | `fx.glitch(split=9, bands=8)` |
| `rgb_split` ✓ | Channel offset on its own, any direction; `pair='red_cyan'` after `grade(saturation=0)` is the red/cyan 3D look. | `fx.rgb_split(angle=330, amount=8)` |
| `jpeg_crush` ✓ | Re-encode at very low quality (the JPEG algorithm as maths: blocks, chroma bleed). | `fx.jpeg_crush(quality=6)`; with `hits`, damaged coefficients: smears and colour blocks, `fx.jpeg_crush(quality=20, hits=40, seed=2)` |
| `scan_roll` ✓ | Vertical or horizontal sync roll. | `fx.scan_roll(offset=0.2)` |
| `interlace` ✓ | Even/odd line offset and combing. | `fx.interlace(offset=6)` |
| `signal_noise` ✓ | Static bands and dropouts. | `fx.signal_noise(bands=3, seed=1)` |
| `echo` ✓ | Offset translucent copies. | `fx.echo(copies=4, dx=14, fade=0.5)` |
| `dead_pixels` ✓ | Stuck and dead LCD pixels. | `fx.dead_pixels(count=120, seed=1)` |
| `hologram` ✓ | Cyan scanlines, flicker, edge glow. | `fx.hologram(color="#30e0ff", phase=0.3)` |

## Atmosphere

| Effect | What it does | Example |
| --- | --- | --- |
| `fog` ✓ | Layered haze, gradient or noise-based. | `fx.fog(density=0.4, color="#cfd8e3")` |
| `smoke` ✓ | Noise-driven smoke or steam. | `fx.smoke(amount=0.4, seed=1)` |
| `sparkle` ✓ | Twinkling points on highlights. | `fx.sparkle(count=60, seed=1)` |
| `lens_drops` ✓ | Water drops on the lens. | `fx.lens_drops(count=14, highlight=0.5, seed=1)` |
| `particles` ✓ | Dots and streaks of light at several depths; presets `embers`, `rain`, `snow`, `dust`, `stars`. | `fx.particles(density=1.5, size=7, length=8, angle=105)` |

## Driving a number by a field

Any number an effect marks `(drivable)` can vary across the picture by a field (`docs/design-decisions.md`, "A field
varies a parameter"): `radius = { by = "far", start = 0, end = 12 }` on a `gaussian_blur` step. Drivable today: the
radius of `gaussian_blur`, `bloom`; `motion_blur`'s `length`; the amounts of `bloom`,
`grain`, `sharpen`, `chromatic_aberration`, `displace`, `vignette`;
`grade`'s `exposure`, `contrast`, `saturation`, `vibrance`; `levels`' `output_black`; `white_balance`'s `temperature` and `tint`;
`hue_shift`'s `hue`; `fog`'s `density`. More follow when a look needs them; the rule is that the result changes smoothly with the number.

## Depth and light from geometry

Effects and recipes that read the injected depth (`docs/design-decisions.md`, "Depth is injected like the subject") and,
from it, the surface's shape, so light and atmosphere follow the scene's geometry instead of the subject's silhouette. A
model only estimates the map; every effect is plain, deterministic maths on the photo's own pixels, never a generative
model that repaints them. Why: grading the background apart from the subject makes it look pasted in (a sticker); depth
treats the ground under the subject's feet like the subject, and falloff follows the distance.

| Effect | What it does | Example |
| --- | --- | --- |
| `depth_haze` ✓ (a recipe, [depth-haze.toml](../examples/recipes/depth-haze.toml)) | Aerial perspective: `fill` toward a pale haze and a flatter `grade`, masked to a `band` of the far part of the depth; `sharpen` (preset `clarity`) on the near. | `fill` with `mask` = a `band` of `depth` region |
| `depth_grade` ✓ (a recipe, [depth-grade.toml](../examples/recipes/depth-grade.toml)) | A grade per distance: `white_balance`, `split_tone` and `grade` looks over `bands` of `depth` pieces (`each` + `looks`). | `each` over a `bands` region of `depth` |
| `relight` ✓ | Light a field (`depth` by default, the picture's `light`, any region), read as a relief, from a direction (Lambert on normals of the smoothed field); flat areas are unchanged. | `fx.relight(field="depth", angle=180, elevation=35, height=250)` |
| coloured point light (planned, on `relight`) | `color` and a light at `x`, `y` as well as a direction: a neon light moving over the subject as video sets `x`, `y`. | `fx.relight(color="#ff2bd6", x=0.8, y=0.3)` |
| depth contours ✓ (`topographic` with `field`) | Contour lines of any field, such as the depth: a LiDAR or scanner look. | `fx.topographic(field="depth", levels=24)` |
| `ambient_occlusion` ✓ | Soft darkening in creases and beside nearer things, from the depth (screen-space); steps mix it in at opacity 0.6 by default. | `fx.ambient_occlusion()` |
| `light_wrap` | The surroundings' light and colour wrapped onto the subject's edge. Possible today as a recipe: `blend` of a blurred copy, `soft_light`, masked to the subject. | `fx.light_wrap(radius=60, amount=0.4)` |

## Subject-aware effects

Use the subject cut-out, so they target the subject or the background.

| Effect | What it does | Example |
| --- | --- | --- |
| `fill` ✓ | Flat colour; masked = background removal. | `fx.fill("#000", mask="background")` |
| `outline` ✓ | Coloured stroke around the subject, optional glow. | `fx.outline(width=6, color="#fff", glow=12)` |
| `drop_shadow` ✓ | Soft shadow behind the subject. | `fx.drop_shadow(dx=12, dy=18, blur=24)` |
| `long_shadow` ✓ | Flat-design shadow at an angle. | `fx.long_shadow(angle=45, length=300)` |
| `rim_light` ✓ | Light along the subject's edge from a side. | `fx.rim_light(angle=40, color="#ff9a4d")` |
| `reflection` ✓ | Floor mirror with fade. | `fx.reflection(height=0.4, fade=0.7)` |
| `background` ✓ | Replace the background: colour, gradient, blur or image. A recipe step, no effect: `mask = "background"` on `fill`, `gradient` or any other effect. | `fill / gradient with mask = "background"` |
| `background_blur` ✓ | Portrait-mode blur. A recipe step: `gaussian_blur` with `mask = "background"`. | `gaussian_blur(radius=24) with mask = "background"` |
| `sticker` ✓ | White border and shadow: a die-cut sticker. A recipe: `outline` then `drop_shadow` (examples/recipes/sticker.toml). | `outline(width=14) + drop_shadow` |
| `silhouette` ✓ | Flat colour subject. A recipe: `fill` masked to the subject, then to the background. | `fill(color="#000") mask = "subject"` |
| `color_pop` ✓ | Subject in colour, background in grey. A recipe: `channel_mixer(preset="monochrome")` with `mask = "background"` (examples/recipes/portrait-pop.toml). | `channel_mixer preset = "monochrome", mask = "background"` |
| `spotlight` ✓ | Light the subject, darken the rest. | `fx.spotlight(falloff=0.6)` |
| `aura` ✓ | Soft coloured halo around the subject. `outline` with a large `glow`. | `fx.outline(width=0, color="#7a2cff", glow=40)` |
| `layered_cutouts` ✓ | Offset copies of the silhouette for depth. | `fx.layered_cutouts(layers=4)` |
| `subject` selection | Pick `largest`, `all` or the n-th subject when there are several. | `subject="largest"` on any masked step |
| decisions layer (future) | Optional, bring-your-own-key: a vision model you configure decides which subjects to select (the person, not the dog), off by default and never needed. | `subject="the person on the left"` with your own API key |

## Compositing

Combine the photo with an asset (a picture that is part of the look, given as a `Picture` parameter) and blend modes.

| Effect | What it does | Example |
| --- | --- | --- |
| `blend` ✓ | Combine with another layer in any blend mode (masked to the background, it replaces the background with a picture); `fit="width"` places a logo or watermark (preset `logo`). | `fx.blend(image="tex.png", mode="soft_light")` |
| `gradient_overlay` ✓ (a recipe step) | Linear, radial or conic colour wash: `gradient` with a step `blend_mode`, not an effect. | `fx.gradient(colors=["#f0f","#0ff"], kind="linear", blend_mode="screen")` |

Combining several photos as equals (a collage, a transition or morph between two shots) needs several inputs or a
canvas of its own, so it belongs to video or a layout package; before/after side by side is the `compare` command.

Text is bullterrier-vector's (typesetting); a caption here is an `svg` region + `fill`/`outline`.

## Generated backgrounds

Backgrounds made from nothing, for use behind a cut-out or under any effect.

| Effect | What it does | Example |
| --- | --- | --- |
| `gradient` ✓ | Linear, radial, conic or mesh gradient. | `fx.gradient(colors=["#12002b","#ff2bd6"], kind="radial")` |
| `noise_field` ✓ | Value, Perlin, Worley or sine noise as colour, with a domain `warp`; preset `plasma`. | `fx.noise_field(kind="perlin", scale=200, seed=1)` |
| `pattern` ✓ | Stripes, checks, dots, grid. | `fx.pattern(kind="grid", cell=40)` |
| `voronoi_cells` ✓ | Cell pattern. | `fx.voronoi_cells(cells=200, seed=1)` |
| `topographic` ✓ | Contour lines of luminance, depth or any field. | `fx.topographic(levels=14)` |
| `flow_lines` ✓ | Streamlines through a noise flow field. | `fx.flow_lines(density=0.004, seed=1)` |
| `aurora` ✓ | Soft vertical light curtains. | `fx.aurora(colors=["#2bff9a","#7a2cff"])` |
| `fractal` ✓ | Mandelbrot or Julia sets with smooth colouring. | `fx.fractal(kind="julia", zoom=1.5, iterations=64)` |

## Utilities

| Effect | What it does | Example |
| --- | --- | --- |
| `avatar` ✓ (a recipe, [avatar.toml](../examples/transparent/avatar.toml)) | Square crop on the subject, optional circle: framing and transparency settings. | `aspect = 1.0`, `anchor = "subject"`, `size = 512`, `transparent = "circle"` |
| `upscale` ✓ (a setting) | Resampling to a `size` above the photo's own (an optional AI model is not built). | `size = 4096` |
| `palette_from_photo` | Extract dominant colours to feed other steps. | `palette = photo.palette(5)` |
| `sweep` ✓ (CLI) | Render one parameter across a range, side by side. | `bt-image sweep photo.jpg r.toml glitch.split 0..20` |
| `compare` (CLI) | Before/after slider page. | `bt-image compare a.jpg b.jpg` |
| `avatar pack` (export) | Every size a set of platforms asks for. | `[export] preset = "avatars"` |

## One moment of a process

Effects that show one picture of something that unfolds. `progress` (0 to 1) is the moment shown; whoever animates it
(the future `bullterrier-video` package) sets it for each frame, from a ramp, a beat or anything else. This library has
no time of its own.

| Effect | What it does | Example |
| --- | --- | --- |
| `disperse` ✓ | Pieces detach, drift on the wind and fade. | `fx.disperse(progress=0.45, wind=0.3)` |
| `dissolve` | Pixels or blocks of `cell` px vanish in noise order, with a burning edge (covers the old `pixel_dissolve`). | `fx.dissolve(progress=0.5, cell=16, edge="#ff7a2b")` |
| burst (planned, on `disperse`) | Pieces burst outward from a point (`x`, `y`) as well as drifting on the wind. Replaces `explode`. | `fx.disperse(progress=0.5, burst=1, x=0.5, y=0.4)` |
| `parallax` ✓ | 2.5D depth shift, from the injected depth: near layers slide over far ones, one shifted view. | `fx.parallax(amount=0.03, focus=0.5)` |

Dropped: `assemble`. Pieces flying in and settling is `disperse` with `progress` running from 1 to 0, set by video.

Text appearing character by character is bullterrier-vector's (typesetting) animated by video; a caption here is an `svg` region + `fill`/`outline`.

Effects that need time, several frames or two shots (`flicker`, `breathe`, `camera_shake`, `ken_burns`, `trails`,
`slit_scan`, `morph`) and animated values (ramps, easing, keyframes) do not belong here: they are for the video and
control packages.

## Signature looks

What combining effects can give, once the pieces exist:

- **Cyberpunk** ✓ ([recipe](../examples/recipes/cyberpunk.toml)): split tone, chromatic aberration, scanlines, bloom, rain, CRT glass
- **Risograph poster**: levels, risograph inks (`halftone` or `dither` with inks), paper texture
- **Terminal** ✓ ([recipe](../examples/recipes/terminal.toml)): green ASCII on a curved CRT screen with shadow mask, scanlines and glow
- **Vaporwave** ✓ ([recipe](../examples/recipes/vaporwave.toml)): gradient background, grid pattern, gradient map, glitch
- **Newspaper** ✓ ([recipe](../examples/recipes/newsprint.toml), black and white until `halftone` has CMYK separation; the sepia is `gradient_map`'s preset): CMYK halftone, paper texture, faded sepia, torn edge
- **Double exposure** ✓ ([recipe](../examples/recipes/double-exposure.toml)): black and white, a night sky multiplied into the silhouette, vignette
- **Sticker** ✓ ([recipe](../examples/recipes/sticker.toml)): cut-out with a white border and drop shadow
- **Portrait pop** ✓ ([recipe](../examples/recipes/portrait-pop.toml)): grey, blurred background, subject in colour
- **Turn to dust**: ASCII, disperse, `particles` (preset `embers`) (one moment of it; a video tool animates it)
- **Northern lights** ✓ ([recipe](../examples/recipes/northern-lights.toml)): aurora and stars behind a cold-tinted subject with a rim light

A look is added to `examples/recipes/` by the batch that builds its last missing effect (Risograph poster needs inks on `halftone` or `dither`; Double exposure needed the first `Picture` effect, `blend`, which exists; Turn to dust needs a recipe file only: `ascii`, `disperse` and `particles` exist).

## Planned consolidation (before 1.0; breaking, no aliases)

Fewer effects, each fully customisable and distinct. A merged effect has fewer parameters than the effects it replaces,
and each parameter matters for every look. About 128 effects become about 94. The ✓ rows above stay until each merge
lands.

**Core: field parameters.** An effect may name a field (`depth`, `light`, `subject`, a region such as a gradient or a
band), resolved like a `Drive`'s `by`; only named fields are built. This merged `relight`, `bump_light` and `emboss`
(`relight`'s `field`; presets `bump_light` and `emboss`) and `depth_of_field` with `tilt_shift` (`focus_blur`; the tilt
look = a linear-gradient field), and gives `topographic` depth contours.

**Merges that change no picture:**

| Goes | Into |
| --- | --- |
| `fade` | `levels(output_black)` |
| `invert` | a `levels` preset |
| `noise`, `film_grain` | `grain` (`shadows`, `highlights`, `color`, `distribution`) |
| `black_and_white` | a `channel_mixer` preset |
| `vibrance` | `grade` + `vibrance` |
| `mirror` | `kaleidoscope(segments=1)` |
| `plasma` | `noise_field(kind="sine")` + `warp` |

**Family merges:**

| Goes | Into |
| --- | --- |
| `embers`, `floating_dust`, `rain`, `snow`, `starfield` | `particles`, with presets |
| `drop_shadow`, `layered_cutouts`, `long_shadow` | one silhouette-copies effect |
| `glitch` | a recipe: `slice_shift` + `rgb_split` |
| `sparkle` | `starburst` + `count`, `dot` |
| `fog`, `smoke` | one, with presets |
| `hologram` | a recipe (needs `edges`; `scanlines` has `softness`) |
| `split_tone`, `color_balance` | one three-range tool |
| `overlay` | `blend` + `fit="width"` |
| `evaporate` | `disperse`, if its looks are reachable at a fixed `progress` |
| `shadows_highlights` + `tone_map` | one, if one set of curves covers both looks |

**Conventions, applied with the merges:**
- Angles of a direction are measured clockwise from pointing right.
- The size of a mark is in px (tuned for 480); placement and reach are a share of the long side.
- Scattered marks use `density`.
- `edge` defaults to 'reflect' for blurs and warps, and to 'nearest' for shifts and copies.
