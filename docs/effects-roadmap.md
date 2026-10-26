# Effects roadmap

What bullterrier-image can do today and what could come next. **✓ marks what exists now** (`bt-image effects` lists it with every
parameter); everything else is a proposal. Examples use the Python form `fx.<effect>(...)`; in a recipe file the same call
is `effect = "<effect>"` plus the same parameters.

Looks that are one effect with a set of values ship as presets of that effect (`fx.gradient_map(preset="sepia")`, data in
`src/bullterrier_image/presets/`) rather than as separate code; looks that combine several effects (VHS, Polaroid, pop art) are
recipes.

## Colour and tone

| Effect | What it does | Example |
| --- | --- | --- |
| `grade` ✓ | Exposure, contrast, saturation. | `fx.grade(exposure=0.3)` |
| `gradient_map` ✓ | Recolour by brightness with any gradient. | `fx.gradient_map(stops=["#000","#f0f"])` |
| `split_tone` ✓ | Tint shadows and highlights. | `fx.split_tone(shadow="#3a1bff")` |
| `hue_shift` ✓ | Rotate hues, keep brightness. | `fx.hue_shift(hue=150)` |
| `levels` ✓ | Black point, white point, gamma. | `fx.levels(black=0.05, white=0.95, gamma=1.1)` |
| `curves` ✓ | Tone curve through control points. | `fx.curves(shadows=0.18, highlights=0.82)` |
| `auto_levels` ✓ | Stretch to the full tonal range. | `fx.auto_levels(clip=0.005)` |
| `equalize` ✓ | Adaptive histogram equalisation (CLAHE-style). | `fx.equalize(tiles=8, limit=2.0)` |
| `white_balance` ✓ | Fix colour cast by temperature/tint or a grey point. | `fx.white_balance(temperature=-0.2)` |
| `vibrance` ✓ | Boost muted colours more than vivid ones. | `fx.vibrance(amount=0.5)` |
| `selective_color` ✓ | Shift hue/saturation/lightness of one colour range. | `fx.selective_color(hue=30, saturation=0.6)` |
| `color_balance` ✓ | Separate RGB shifts for shadows, mids, highlights. | `fx.color_balance(highlights_red=0.05, highlights_blue=-0.05)` |
| `channel_mixer` ✓ | 3x3 matrix between channels. | `fx.channel_mixer(red_from_red=0, red_from_green=1, green_from_red=1, green_from_green=0)` |
| `invert` ✓ | Negative. | `fx.invert()` |
| `posterize` ✓ | Few tone levels per channel. | `fx.posterize(levels=4)` |
| `solarize` ✓ | Invert above a threshold. | `fx.solarize(threshold=0.5)` |
| `threshold` ✓ | Two-colour bitmap. | `fx.threshold(level=0.5, dark="#000", light="#fff")` |
| `fade` ✓ | Lift the blacks for a matte film look. | `fx.fade(amount=0.12)` |
| `palette_quantize` ✓ | Reduce to N colours (k-means) or a named palette. | `fx.palette_quantize(palette="gameboy")` |
| `color_transfer` | Match the colour mood of a reference photo. | `fx.color_transfer(reference="ref.jpg")` |
| `lut` | Apply a `.cube` 3D colour lookup table. | `fx.lut(file="film.cube", amount=0.8)` |
| `black_and_white` ✓ | Mono conversion with filter weights. | `fx.black_and_white(red=0.6, green=0.3, blue=0.1)` |
| `sepia` ✓ (a preset of `gradient_map`) | Warm monochrome. | `fx.gradient_map(preset="sepia")` |
| `cyanotype` ✓ (a preset of `gradient_map`) | Prussian-blue print. | `fx.gradient_map(preset="cyanotype")` |
| `cross_process` ✓ (a preset of `curves`) | Shifted curves per channel. | `fx.curves(preset="cross_process")` |
| `bleach_bypass` | Desaturated, high-contrast film look. | `fx.bleach_bypass(amount=0.6)` |
| `teal_orange` ✓ (a preset of `split_tone`) | The blockbuster grade. | `fx.split_tone(preset="teal_orange", amount=0.4)` |
| `thermal` ✓ (a preset of `gradient_map`) | Heat-camera false colour. | `fx.gradient_map(preset="thermal")` |
| `infrared` ✓ (a preset of `channel_mixer`) | False-colour infrared (swapped channels, bright foliage). | `fx.channel_mixer(preset="infrared")` |
| `xray` ✓ (a preset of `gradient_map`) | Inverted and blue (not edge-lit). | `fx.gradient_map(preset="xray")` |
| `night_vision` ✓ (a preset of `gradient_map`) | Green phosphor (no noise or vignette: add `film_grain` and `vignette`). | `fx.gradient_map(preset="night_vision")` |
| `film_stock` | Approximations of named stocks (portra, tri-x, velvia). | `fx.film_stock("portra")` |

## Light, focus and detail

| Effect | What it does | Example |
| --- | --- | --- |
| `clarity` ✓ | Local contrast. | `fx.clarity(radius=40)` |
| `glow` ✓ | Soft bloom around highlights. | `fx.glow(radius=28)` |
| `sharpen` ✓ | Unsharp mask. | `fx.sharpen(radius=2, amount=0.8)` |
| `highpass` ✓ | Keep only fine detail (for blending). | `fx.highpass(radius=8)` |
| `denoise` | Median, bilateral or non-local-means (the default method fits the render budget; a slower one may be chosen). | `fx.denoise(method="bilateral", amount=0.4)` |
| `shadows_highlights` ✓ | Recover dark and bright areas. | `fx.shadows_highlights(shadows=0.4)` |
| `dehaze` ✓ | Remove atmospheric haze. | `fx.dehaze(amount=0.5)` |
| `tone_map` ✓ | Local tone mapping, the "HDR look". | `fx.tone_map(amount=0.7)` |
| `vignette` ✓ | Darken (or colour) the corners. | `fx.vignette(amount=0.5, roundness=0.8)` |
| `gaussian_blur` ✓ | Blur. | `fx.gaussian_blur(radius=6)` |
| `motion_blur` ✓ | Directional blur. | `fx.motion_blur(angle=30, length=40)` |
| `radial_blur` ✓ | Spin or zoom blur from a point. | `fx.radial_blur(mode="zoom", amount=0.1)` |
| `lens_blur` ✓ | Disc-shaped bokeh blur. | `fx.lens_blur(radius=12, highlights=0.6)` |
| `bloom` ✓ | Multi-scale glow (upgrade of `glow`). | `fx.bloom(radius=40, threshold=0.7)` |
| `anamorphic_streaks` ✓ | Horizontal flare streaks off highlights. | `fx.anamorphic_streaks(length=0.3)` |
| `starburst` ✓ | Diffraction spikes on bright points. | `fx.starburst(rays=6, length=0.15)` |
| `lens_flare` ✓ | Flare with ghost reflections. | `fx.lens_flare(x=0.8, y=0.2)` |
| `light_rays` ✓ | God rays from a point or the highlights. | `fx.light_rays(x=0.7, y=0.1, length=0.5)` |
| `chromatic_aberration` ✓ | Radial red/blue fringing. | `fx.chromatic_aberration(amount=4)` |
| `soft_focus` ✓ | Dreamy glow over a sharp image (Orton). | `fx.soft_focus(opacity=0.5)` |
| `halation` ✓ | Red glow around highlights, as on film. | `fx.halation(radius=20)` |
| `tilt_shift` ✓ | Miniature effect: blur away from a band, boost colour. | `fx.tilt_shift(focus=0.5, width=0.15)` |
| `light_leak` ✓ | Warm coloured light washes . | `fx.light_leak(color="#ff7a2b")` |
| `depth_of_field` ✓ | Blur by distance, from the injected depth (the model, or a map you supply); a band around `focus` stays sharp. | `fx.depth_of_field(focus=0.58, radius=8)` |

## Texture and finish

| Effect | What it does | Example |
| --- | --- | --- |
| `grain` ✓ | Film grain. | `fx.grain(amount=0.07)` |
| `film_grain` ✓ | Grain that depends on brightness, per channel. | `fx.film_grain(shadows=0.1, highlights=0.03)` |
| `noise` ✓ | Gaussian/uniform, colour or mono noise. | `fx.noise(amount=0.1, color=False)` |
| `scanlines` ✓ | Horizontal line pattern. | `fx.scanlines(spacing=3, darkness=0.3)` |
| `pixel_grid` ✓ | LCD sub-pixel grid. | `fx.pixel_grid(cell=4)` |
| `emboss` ✓ | Relief from luminance. | `fx.emboss(angle=135, height=6)` |
| `bump_light` ✓ | Light a height map from a direction. | `fx.bump_light(angle=45, height=8)` |
| `dust_scratches` ✓ | Specks, hairs and scratches. | `fx.dust_scratches(density=0.3, seed=1)` |
| `paper_texture` ✓ | Procedural paper, canvas or linen surface. | `fx.paper_texture(kind="canvas", amount=0.3)` |
| `border` | Solid or rounded border, drawn inside the picture's edge. | `fx.border(width=24, color="#fff", radius=12)` |
| `crt` ✓ | Curvature, shadow mask, bloom, scanlines. | `fx.crt(curve=0.15, shadow_mask=0.35)` |
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
| `dither` | Floyd-Steinberg, Atkinson, Bayer, blue-noise, to any palette. | `fx.dither(method="atkinson", palette=["#000","#fff"])` |
| `pixelate` | Mosaic of flat cells. | `fx.pixelate(cell=12)` |
| `edges` | Outlines by Sobel, Canny or difference of Gaussians. | `fx.edges(method="dog", width=2)` |
| `oil_paint` | Kuwahara filter: flat painterly regions. | `fx.oil_paint(radius=6)` |
| `crosshatch` | Layers of hatch lines by tone. | `fx.crosshatch(layers=4, angle=45)` |
| `line_halftone` | Lines whose width carries tone. | `fx.line_halftone(spacing=8, angle=30)` |
| `cmyk_halftone` | Four-ink halftone at screen angles. | `fx.cmyk_halftone(cell=9)` |
| `engraving` | Banknote-style flowing lines. | `fx.engraving(spacing=5)` |
| `stipple` | Dots whose density carries tone (weighted Voronoi). | `fx.stipple(dots=20000, seed=1)` |
| `voronoi_mosaic` | Stained-glass cells with leaded edges. | `fx.voronoi_mosaic(cells=800, edge="#000")` |
| `low_poly` | Delaunay triangles, one colour each, or just their wireframe. | `fx.low_poly(points=1500, seed=1)` |
| `painterly` | Oriented brush strokes. | `fx.painterly(stroke=14, seed=1)` |
| `pointillism` | Dots of varying size and colour. | `fx.pointillism(size=7, jitter=0.4)` |
| `led_wall` | Round LEDs in a grid, brightness by tone. | `fx.led_wall(cell=10)` |
| `brick_mosaic` | Toy-brick tiles with studs. | `fx.brick_mosaic(cell=14, palette="lego")` |
| `sketch` | Pencil drawing (dodge-blended inverted blur). | `fx.sketch(radius=14)` |
| `line_art` | Clean ink lines on flat paper. | `fx.line_art(threshold=0.45)` |
| `pixel_art` | Downsample, limited palette, optional outline. | `fx.pixel_art(cell=8, colors=16)` |
| `toon` | Flat colour regions with ink edges. | `fx.toon(levels=5, edge=2)` |
| `comic` | Toon plus halftone shading. | `fx.comic()` |
| `watercolor` | Edge darkening, pigment pooling, paper. | `fx.watercolor(wetness=0.6)` |
| `woodcut` | Few tones carved as line texture. | `fx.woodcut(levels=3)` |
| `risograph` | Spot-colour layers, misregistration, grain. | `fx.risograph(inks=["#ff4d6d","#2b59ff"])` |
| `screenprint` | N flat colour layers, slightly offset. | `fx.screenprint(layers=3)` |
| `pop_art` | Quad grid of palette-swapped copies. | `fx.pop_art(palettes=4)` |
| `blueprint` | White edges on blue with a grid. | `fx.blueprint()` |
| `neon_edges` | Glowing outlines on black. | `fx.neon_edges(color="#00f0ff")` |
| `paper_cutout` | Posterised layers with drop shadows. | `fx.paper_cutout(layers=5)` |
| `photo_mosaic` | The photo built from other photos. | `fx.photo_mosaic(tiles="tiles/", cell=24)` |

## Computational and experimental

Looks made by running an algorithm on the picture rather than filtering it. The iterative ones (`reaction_diffusion`,
`cellular_automata`) take an explicit step count so they stay within the render budget.

| Effect | What it does | Example |
| --- | --- | --- |
| `liquid_metal` ✓ | Chrome: the picture's brightness as a relief, lit and reflecting a fake environment, in a metal gradient. | `fx.liquid_metal(relief=35, tint="#c9d6ff")` |
| `distance_field` | Signed-distance-field shapes from the picture's silhouette or brightness: soft outlines, glow, contour bands, metaballs. | `fx.distance_field(source="luminance", level=0.5, bands=8)` |
| `reaction_diffusion` ✓ | Gray-Scott growth seeded by the picture: spots, stripes, coral. | `fx.reaction_diffusion(feed=0.037, kill=0.06, steps=300)` |
| `cellular_automata` | Life-like rules run on the thresholded picture. | `fx.cellular_automata(rule="life", steps=12, threshold=0.5)` |
| `lenticular` ✓ | Lenticular-print look: thin interlaced strips that shift with the viewing angle. | `fx.lenticular(strip=6, shift=5)` |

## Text and symbols

| Effect | What it does | Example |
| --- | --- | --- |
| `ascii_edges` | Characters follow edge direction (`/ \ \| _`). | `fx.ascii_edges(cell=6)` |
| `braille` | Unicode braille dots, 2x4 per character. | `fx.braille(cell=4)` |
| `half_blocks` | Terminal-style `▀▄` pixel art. | `fx.half_blocks(cell=6)` |
| `digital_rain` | Falling glyph columns over the picture. | `fx.digital_rain(density=0.3, seed=1)` |
| `text_mosaic` | The photo built from the words of a supplied text. | `fx.text_mosaic(text=open("poem.txt").read())` |
| `typographic_portrait` | Lines of text, weight carries tone. | `fx.typographic_portrait(text="...")` |
| `barcode` | Vertical bars whose width carries tone. | `fx.barcode(bars=120)` |
| `cross_stitch` | Grid of X stitches in the photo's colours. | `fx.cross_stitch(cell=8)` |
| `segments` | Seven-segment digits. | `fx.segments(cell=10)` |
| `binary` | ASCII with `0`/`1` only. | `fx.binary(cell=6)` |

## Distortion and geometry

**Shape-preserving warps**

| Effect | What it does | Example |
| --- | --- | --- |
| `swirl` ✓ | Twist around a point. | `fx.swirl(angle=120, radius=0.5)` |
| `pinch_bulge` ✓ | Push in or out. | `fx.pinch_bulge(amount=0.5)` |
| `ripple` ✓ | Travelling wave displacement. | `fx.ripple(wavelength=40, amplitude=6, phase=0)` |
| `displace` ✓ | Move pixels by noise or a map. | `fx.displace(amount=12, scale=80, seed=1)` |
| `lens_distortion` ✓ | Barrel or pincushion. | `fx.lens_distortion(distortion=0.2)` |
| `fisheye` ✓ | Wide-angle bulge. | `fx.fisheye(amount=0.6)` |
| `polar` ✓ | To or from polar coordinates (tiny planet). | `fx.polar(mode="to")` |
| `mirror` ✓ | Symmetry on an axis, or four-way. | `fx.mirror(direction="left_to_right")` |
| `kaleidoscope` ✓ | N mirrored segments. | `fx.kaleidoscope(segments=8, angle=15)` |
| `tile` ✓ | Repeat the picture in a grid. | `fx.tile(columns=3, rows=3)` |
| `shear` ✓ | Skew with edge fill. | `fx.shear(x=0.2)` |
| `perspective` ✓ | Corner-pin to a quad. | `fx.perspective(vertical=0.3, rotate=-3, scale=0.2)` |
| `pixel_sort` ✓ | Sort runs of pixels by brightness or hue. | `fx.pixel_sort(by="brightness", low=0.25, high=0.75)` |
| `melt` ✓ | Brightness-driven drips running down. | `fx.melt(amount=40, seed=1)` |
| `flow_warp` ✓ | Smeared by a noise flow field. | `fx.flow_warp(scale=80, amount=20, seed=1)` |
| `stretch` ✓ | Smear one pixel row or column outward. | `fx.stretch(axis="vertical", at=0.5)` |
| `scatter` ✓ | Random pixel displacement. | `fx.scatter(radius=4, seed=1)` |
| `shatter` ✓ | Cracked-glass pieces, slightly offset and rotated. | `fx.shatter(pieces=24, gap=1.5, seed=1)` |
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
| `rgb_split` ✓ | Channel offset on its own, any direction. | `fx.rgb_split(angle=30, amount=8)` |
| `jpeg_crush` ✓ | Re-encode at very low quality (the JPEG algorithm as maths: blocks, chroma bleed). | `fx.jpeg_crush(quality=6)` |
| `scan_roll` ✓ | Vertical or horizontal sync roll. | `fx.scan_roll(offset=0.2)` |
| `interlace` ✓ | Even/odd line offset and combing. | `fx.interlace(offset=6)` |
| `signal_noise` ✓ | Static bands and dropouts. | `fx.signal_noise(bands=3, seed=1)` |
| `echo` ✓ | Offset translucent copies. | `fx.echo(copies=4, dx=14, fade=0.5)` |
| `prism` ✓ | Rainbow dispersion. | `fx.prism(amount=18)` |
| `anaglyph` ✓ | Red/cyan 3D look. | `fx.anaglyph(shift=6)` |
| `dead_pixels` ✓ | Stuck and dead LCD pixels. | `fx.dead_pixels(count=120, seed=1)` |
| `byte_corrupt` ✓ | Damage a JPEG's stored coefficients, then decode: smears and colour blocks. | `fx.byte_corrupt(hits=40, seed=2)` |
| `hologram` ✓ | Cyan scanlines, flicker, edge glow. | `fx.hologram(color="#30e0ff", phase=0.3)` |

## Atmosphere

| Effect | What it does | Example |
| --- | --- | --- |
| `fog` ✓ | Layered haze, gradient or noise-based. | `fx.fog(density=0.4, color="#cfd8e3")` |
| `rain` ✓ | Falling streaks (and optional splashes). | `fx.rain(density=0.5, angle=12, seed=1)` |
| `snow` ✓ | Flakes in depth layers. | `fx.snow(density=0.4, seed=1)` |
| `smoke` ✓ | Noise-driven smoke or steam. | `fx.smoke(amount=0.4, seed=1)` |
| `sparkle` ✓ | Twinkling points on highlights. | `fx.sparkle(count=60, seed=1)` |
| `floating_dust` ✓ | Slow bokeh particles. | `fx.floating_dust(count=80, seed=1)` |
| `lens_drops` ✓ | Water drops on the lens. | `fx.lens_drops(count=14, highlight=0.5, seed=1)` |
| `embers` ✓ | Rising sparks. | `fx.embers(count=120, seed=1)` |

## Depth and light from geometry

Effects and recipes that read the injected depth (`docs/design-decisions.md`, "Depth is injected like the subject") and,
from it, the surface's shape, so light and atmosphere follow the scene's geometry instead of the subject's silhouette. A
model only estimates the map; every effect is plain, deterministic maths on the photo's own pixels, never a generative
model that repaints them. Why: grading the background apart from the subject makes it look pasted in (a sticker); depth
treats the ground under the subject's feet like the subject, and falloff follows the distance.

| Effect | What it does | Example |
| --- | --- | --- |
| `depth_haze` ✓ (a recipe, [depth-haze.toml](../examples/recipes/depth-haze.toml)) | Aerial perspective: `fill` toward a pale haze and a flatter `grade`, masked to a `depth` region of the far part; `clarity` on the near. | `fill` with `mask` = a `kind = "depth"` region |
| `depth_grade` ✓ (a recipe, [depth-grade.toml](../examples/recipes/depth-grade.toml)) | A grade per distance: `white_balance`, `split_tone` and `grade` looks over `depth_bands` pieces (`each` + `looks`). | `each` over a `kind = "depth_bands"` region |
| `relight` ✓ | Light the depth, read as a relief, from a direction (Lambert on normals of the smoothed depth); flat areas are unchanged. | `fx.relight(angle=270, elevation=35, height=250)` |
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
| `color_pop` ✓ | Subject in colour, background in grey. A recipe: `black_and_white` with `mask = "background"` (examples/recipes/portrait-pop.toml). | `black_and_white mask = "background"` |
| `spotlight` ✓ | Light the subject, darken the rest. | `fx.spotlight(falloff=0.6)` |
| `aura` ✓ | Soft coloured halo around the subject. `outline` with a large `glow`. | `fx.outline(width=0, color="#7a2cff", glow=40)` |
| `double_exposure` | Another picture blended inside the silhouette. | `fx.double_exposure(image="forest.jpg")` |
| `layered_cutouts` ✓ | Offset copies of the silhouette for depth. | `fx.layered_cutouts(layers=4)` |
| `subject` selection | Pick `largest`, `all` or the n-th subject when there are several. | `subject="largest"` on any masked step |
| decisions layer (future) | Optional, bring-your-own-key: a vision model you configure decides which subjects to select (the person, not the dog), off by default and never needed. | `subject="the person on the left"` with your own API key |

## Compositing

Combine the photo with an asset (a picture that is part of the look, given as a `Picture` parameter) and blend modes.

| Effect | What it does | Example |
| --- | --- | --- |
| `blend` ✓ | Combine with another layer in any blend mode (masked to the background, it replaces the background with a picture). | `fx.blend(image="tex.png", mode="soft_light")` |
| `overlay` ✓ | Place an image, logo or watermark. | `fx.overlay(image="logo.png", at="bottom_right", scale=0.2)` |
| `gradient_overlay` ✓ (a recipe step) | Linear, radial or conic colour wash: `gradient` with a step `blend_mode`, not an effect. | `fx.gradient(colors=["#f0f","#0ff"], kind="linear", blend_mode="screen")` |

Combining several photos as equals (a collage, a transition or morph between two shots) needs several inputs or a
canvas of its own, so it belongs to video or a layout package; before/after side by side is the `compare` command.

Text is bullterrier-vector's (typesetting); a caption here is an `svg` region + `fill`/`outline`.

## Generated backgrounds

Backgrounds made from nothing, for use behind a cut-out or under any effect.

| Effect | What it does | Example |
| --- | --- | --- |
| `gradient` ✓ | Linear, radial, conic or mesh gradient. | `fx.gradient(colors=["#12002b","#ff2bd6"], kind="radial")` |
| `noise_field` ✓ | Perlin, simplex or Worley noise as colour. | `fx.noise_field(kind="perlin", scale=200, seed=1)` |
| `pattern` ✓ | Stripes, checks, dots, grid. | `fx.pattern(kind="grid", cell=40)` |
| `plasma` ✓ | Classic plasma colours. | `fx.plasma(scale=120, seed=1)` |
| `starfield` ✓ | Stars with depth. | `fx.starfield(count=600, seed=1)` |
| `voronoi_cells` ✓ | Cell pattern. | `fx.voronoi_cells(cells=200, seed=1)` |
| `topographic` ✓ | Contour lines of luminance or noise. | `fx.topographic(levels=14)` |
| `flow_lines` ✓ | Streamlines through a noise flow field. | `fx.flow_lines(density=0.004, seed=1)` |
| `aurora` ✓ | Soft vertical light curtains. | `fx.aurora(colors=["#2bff9a","#7a2cff"])` |
| `fractal` ✓ | Mandelbrot or Julia sets with smooth colouring. | `fx.fractal(kind="julia", zoom=1.5, iterations=64)` |

## Utilities

| Effect | What it does | Example |
| --- | --- | --- |
| `auto_enhance` | Sensible levels, white balance and vibrance in one step. | `fx.auto_enhance()` |
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
| `assemble` | The reverse: pieces fly in and settle. | `fx.assemble(progress=0.5)` |
| `dissolve` | Pixels vanish in noise order, with a burning edge. | `fx.dissolve(progress=0.5, edge="#ff7a2b")` |
| `pixel_dissolve` | Dissolve into blocks. | `fx.pixel_dissolve(progress=0.5, cell=16)` |
| `scan_reveal` | A bright line sweeps and reveals the effect. | `fx.scan_reveal(progress=0.5, angle=90)` |
| `wipe` | Linear, iris or clock reveal. | `fx.wipe(kind="iris", progress=0.5)` |
| `explode` | Pieces burst radially from a point. | `fx.explode(progress=0.5, center=[0.5,0.4])` |
| `shatter_fall` | Pieces fall with gravity and spin. | `fx.shatter_fall(progress=0.5)` |
| `melt_down` | Drips run down and pool. | `fx.melt_down(progress=0.5)` |
| `paper_burn` | An ember front eats the picture. | `fx.paper_burn(progress=0.5)` |
| `parallax` ✓ | 2.5D depth shift, from the injected depth: near layers slide over far ones, one shifted view. | `fx.parallax(amount=0.03, focus=0.5)` |

Text appearing character by character is bullterrier-vector's (typesetting) animated by video; a caption here is an `svg` region + `fill`/`outline`.

Effects that need time, several frames or two shots (`flicker`, `breathe`, `camera_shake`, `ken_burns`, `trails`,
`slit_scan`, `morph`) and animated values (ramps, easing, keyframes) do not belong here: they are for the video and
control packages.

## Signature looks

What combining effects can give, once the pieces exist:

- **Cyberpunk** ✓ ([recipe](../examples/recipes/cyberpunk.toml)): split tone, chromatic aberration, scanlines, bloom, rain, CRT glass
- **Risograph poster**: levels, risograph inks, paper texture, border
- **Terminal** ✓ ([recipe](../examples/recipes/terminal.toml)): green ASCII on a curved CRT screen with shadow mask, scanlines and glow
- **Vaporwave** ✓ ([recipe](../examples/recipes/vaporwave.toml)): gradient background, grid pattern, gradient map, glitch
- **Newspaper** ✓ ([recipe](../examples/recipes/newsprint.toml), black and white until `cmyk_halftone` exists; the sepia is `gradient_map`'s preset): CMYK halftone, paper texture, faded sepia, torn edge
- **Double exposure** ✓ ([recipe](../examples/recipes/double-exposure.toml)): black and white, a night sky multiplied into the silhouette, vignette
- **Sticker** ✓ ([recipe](../examples/recipes/sticker.toml)): cut-out with a white border and drop shadow
- **Portrait pop** ✓ ([recipe](../examples/recipes/portrait-pop.toml)): grey, blurred background, subject in colour
- **Turn to dust**: ASCII, disperse, embers (one moment of it; a video tool animates it)
- **Northern lights** ✓ ([recipe](../examples/recipes/northern-lights.toml)): aurora and stars behind a cold-tinted subject with a rim light

A look is added to `examples/recipes/` by the batch that builds its last missing effect (Risograph poster needs `risograph` and `border`; Double exposure needed the first `Picture` effect, `blend`, which exists; Turn to dust needs a recipe file only: `ascii`, `disperse` and `embers` exist).
