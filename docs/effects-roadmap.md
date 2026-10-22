# Effects roadmap

What photo-fx can do today and what could come next. **✓ marks what exists now** (`photo-fx effects` lists it with every
parameter); everything else is a proposal. Examples use the Python form `fx.<effect>(...)`; in a recipe file the same call
is `effect = "<effect>"` plus the same parameters.

Many looks (sepia, VHS, Polaroid, pop art) are combinations of simpler effects and will ship as presets rather than as
separate code.

## Colour and tone

| Effect | What it does | Example |
| --- | --- | --- |
| `grade` ✓ | Exposure, contrast, saturation. | `fx.grade(exposure=0.3)` |
| `gradient_map` ✓ | Recolour by brightness with any gradient. | `fx.gradient_map(stops=["#000","#f0f"])` |
| `split_tone` ✓ | Tint shadows and highlights. | `fx.split_tone(shadow="#3a1bff")` |
| `hue_shift` ✓ | Rotate hues, keep brightness. | `fx.hue_shift(degrees=150)` |
| `levels` | Black point, white point, gamma. | `fx.levels(black=0.05, white=0.95, gamma=1.1)` |
| `curves` | Tone curve through control points. | `fx.curves(points=[[0,0],[0.25,0.18],[1,1]])` |
| `auto_levels` | Stretch to the full tonal range. | `fx.auto_levels(clip=0.5)` |
| `equalize` | Adaptive histogram equalisation (CLAHE-style). | `fx.equalize(tiles=8, limit=2.0)` |
| `white_balance` | Fix colour cast by temperature/tint or a grey point. | `fx.white_balance(temperature=-0.2)` |
| `vibrance` | Boost muted colours more than vivid ones. | `fx.vibrance(amount=0.5)` |
| `selective_color` | Shift hue/saturation/lightness of one colour range. | `fx.selective_color(hue="orange", sat=-0.4)` |
| `color_balance` | Separate RGB shifts for shadows, mids, highlights. | `fx.color_balance(highlights=[0.05,0,-0.05])` |
| `channel_mixer` | 3x3 matrix between channels. | `fx.channel_mixer(matrix=[[0,1,0],[1,0,0],[0,0,1]])` |
| `invert` | Negative. | `fx.invert()` |
| `posterize` | Few tone levels per channel. | `fx.posterize(levels=4)` |
| `solarize` | Invert above a threshold. | `fx.solarize(threshold=0.5)` |
| `threshold` | Two-colour bitmap. | `fx.threshold(level=0.5, dark="#000", light="#fff")` |
| `fade` | Lift the blacks for a matte film look. | `fx.fade(amount=0.12)` |
| `palette_quantize` | Reduce to N colours (k-means) or a named palette. | `fx.palette_quantize(palette="gameboy")` |
| `color_transfer` | Match the colour mood of a reference photo. | `fx.color_transfer(reference="ref.jpg")` |
| `lut` | Apply a `.cube` 3D colour lookup table. | `fx.lut(file="film.cube", amount=0.8)` |
| `black_and_white` | Mono conversion with filter weights. | `fx.black_and_white(red=0.6, green=0.3, blue=0.1)` |
| `sepia` | Warm monochrome. | `fx.sepia()` |
| `cyanotype` | Prussian-blue print. | `fx.cyanotype()` |
| `cross_process` | Shifted curves per channel. | `fx.cross_process()` |
| `bleach_bypass` | Desaturated, high-contrast film look. | `fx.bleach_bypass(amount=0.6)` |
| `teal_orange` | The blockbuster grade. | `fx.teal_orange(amount=0.5)` |
| `thermal` | Heat-camera false colour. | `fx.thermal()` |
| `infrared` | False-colour infrared (swapped channels, bright foliage). | `fx.infrared()` |
| `xray` | Inverted, blue, edge-lit. | `fx.xray()` |
| `night_vision` | Green phosphor, noise, vignette. | `fx.night_vision()` |
| `film_stock` | Approximations of named stocks (portra, tri-x, velvia). | `fx.film_stock("portra")` |

## Light, focus and detail

| Effect | What it does | Example |
| --- | --- | --- |
| `clarity` ✓ | Local contrast. | `fx.clarity(radius=40)` |
| `glow` ✓ | Soft bloom around highlights. | `fx.glow(radius=28)` |
| `sharpen` | Unsharp mask. | `fx.sharpen(radius=2, amount=0.8)` |
| `highpass` | Keep only fine detail (for blending). | `fx.highpass(radius=8)` |
| `denoise` | Median, bilateral or non-local-means. | `fx.denoise(method="bilateral", amount=0.4)` |
| `shadows_highlights` | Recover dark and bright areas. | `fx.shadows_highlights(shadows=0.4)` |
| `dehaze` | Remove atmospheric haze. | `fx.dehaze(amount=0.5)` |
| `tone_map` | Local tone mapping, the "HDR look". | `fx.tone_map(amount=0.7)` |
| `vignette` | Darken (or colour) the corners. | `fx.vignette(amount=0.5, roundness=0.8)` |
| `gaussian_blur` | Blur. | `fx.gaussian_blur(radius=6)` |
| `motion_blur` | Directional blur. | `fx.motion_blur(angle=30, length=40)` |
| `radial_blur` | Spin or zoom blur from a point. | `fx.radial_blur(mode="zoom", amount=0.1)` |
| `lens_blur` | Disc-shaped bokeh blur. | `fx.lens_blur(radius=12, highlights=0.6)` |
| `bloom` | Multi-scale glow (upgrade of `glow`). | `fx.bloom(radius=40, threshold=0.7)` |
| `anamorphic_streaks` | Horizontal flare streaks off highlights. | `fx.anamorphic_streaks(length=0.3)` |
| `starburst` | Diffraction spikes on bright points. | `fx.starburst(rays=6, length=60)` |
| `lens_flare` | Flare with ghost reflections. | `fx.lens_flare(x=0.8, y=0.2)` |
| `light_rays` | God rays from a point or the highlights. | `fx.light_rays(x=0.7, y=0.1, length=0.5)` |
| `chromatic_aberration` | Radial red/blue fringing. | `fx.chromatic_aberration(amount=4)` |
| `soft_focus` | Dreamy glow over a sharp image (Orton). | `fx.soft_focus(amount=0.5)` |
| `halation` | Red glow around highlights, as on film. | `fx.halation(radius=20)` |
| `tilt_shift` | Miniature effect: blur away from a band, boost colour. | `fx.tilt_shift(focus=0.5, width=0.15)` |
| `light_leak` | Warm coloured light washes . | `fx.light_leak(color="#ff7a2b")` |
| `depth_of_field` | Blur by distance from a depth map. | `fx.depth_of_field(focus=0.4)` |

## Texture and finish

| Effect | What it does | Example |
| --- | --- | --- |
| `grain` ✓ | Film grain. | `fx.grain(amount=0.07)` |
| `film_grain` | Grain that depends on brightness, per channel. | `fx.film_grain(shadows=0.1, highlights=0.03)` |
| `noise` | Gaussian/uniform, colour or mono noise. | `fx.noise(amount=0.1, color=False)` |
| `scanlines` | Horizontal line pattern. | `fx.scanlines(spacing=3, darkness=0.3)` |
| `pixel_grid` | LCD sub-pixel grid. | `fx.pixel_grid(cell=4)` |
| `emboss` | Relief from luminance. | `fx.emboss(angle=135, depth=1)` |
| `bump_light` | Light a height map from a direction. | `fx.bump_light(angle=45, height=0.6)` |
| `dust_scratches` | Specks, hairs and scratches. | `fx.dust_scratches(density=0.3, seed=1)` |
| `paper_texture` | Procedural paper, canvas or linen surface. | `fx.paper_texture(kind="canvas", amount=0.3)` |
| `border` | Solid or rounded border. | `fx.border(width=24, color="#fff", radius=12)` |
| `crt` | Curvature, shadow mask, bloom, scanlines. | `fx.crt(curve=0.1, mask=0.4)` |
| `vhs` | Tracking bands, chroma bleed, tape noise, wobble. | `fx.vhs(tracking=0.4, bleed=3)` |
| `ntsc` | Chroma smear and dot crawl. | `fx.ntsc(bleed=4)` |
| `polaroid_frame` | Instant-film frame with caption space. | `fx.polaroid_frame(caption="2026")` |
| `film_strip_frame` | 35 mm frame with sprocket holes. | `fx.film_strip_frame()` |
| `torn_edge` | Ragged paper edge. | `fx.torn_edge(roughness=0.6, seed=2)` |

## Stylised looks

| Effect | What it does | Example |
| --- | --- | --- |
| `halftone` ✓ | Dot screen. | `fx.halftone(cell=10, angle=45)` |
| `evaporate` ✓ | A still of pieces peeling off the top or outline. | `fx.evaporate(start=0.4, edge=0.1)` |
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
| `low_poly` | Delaunay triangles, one colour each. | `fx.low_poly(points=1500, seed=1)` |
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
| `swirl` | Twist around a point. | `fx.swirl(angle=2.0, radius=0.5)` |
| `pinch_bulge` | Push in or out. | `fx.pinch_bulge(amount=0.5)` |
| `ripple` | Travelling wave displacement. | `fx.ripple(wavelength=40, amplitude=6, phase=0)` |
| `displace` | Move pixels by noise or a map. | `fx.displace(map="noise", amount=12)` |
| `lens_distortion` | Barrel or pincushion. | `fx.lens_distortion(k=0.2)` |
| `fisheye` | Wide-angle bulge. | `fx.fisheye(amount=0.6)` |
| `polar` | To or from polar coordinates (tiny planet). | `fx.polar(mode="to")` |
| `mirror` | Symmetry on an axis, or four-way. | `fx.mirror(axis="vertical")` |
| `kaleidoscope` | N mirrored segments. | `fx.kaleidoscope(segments=8, angle=15)` |
| `tile` | Repeat the picture in a grid. | `fx.tile(columns=3, rows=3)` |
| `shear` | Skew with edge fill. | `fx.shear(x=0.2)` |
| `perspective` | Corner-pin to a quad. | `fx.perspective(corners=[...])` |
| `pixel_sort` | Sort runs of pixels by brightness or hue. | `fx.pixel_sort(by="brightness", angle=90, low=0.2, high=0.8)` |
| `melt` | Brightness-driven drips running down. | `fx.melt(amount=0.4, seed=1)` |
| `flow_warp` | Smeared by a noise flow field. | `fx.flow_warp(scale=80, amount=20, seed=1)` |
| `stretch` | Smear one pixel row or column outward. | `fx.stretch(axis="x", at=0.5)` |
| `scatter` | Random pixel displacement. | `fx.scatter(radius=6, seed=1)` |
| `shatter` | Cracked-glass pieces, slightly offset and rotated. | `fx.shatter(pieces=60, gap=2, seed=1)` |
| `droste` | Recursive picture-in-picture spiral. | `fx.droste(scale=0.4)` |
| `slice_shift` | Horizontal slices slide (a subset of `glitch`). | `fx.slice_shift(slices=30, amount=0.05)` |

**Changing the picture's size**

| Effect | What it does | Example |
| --- | --- | --- |
| `crop` | Crop to a box or aspect ratio. | `fx.crop(aspect="1:1", anchor="subject")` |
| `smart_crop` | Crop around the subject or the salient region. | `fx.smart_crop(aspect="4:5", margin=0.1)` |
| `resize` | Scale, with a fit mode. | `fx.resize(width=1080, fit="cover")` |
| `pad` | Extend the canvas with a colour. | `fx.pad(top=80, color="#000")` |
| `rotate` | Rotate, with fill or crop to fit. | `fx.rotate(degrees=3, fit="crop")` |
| `flip` | Mirror the whole picture. | `fx.flip(axis="x")` |
| `circle_crop` | Circular avatar with transparency. | `fx.circle_crop()` |
| `rounded_corners` | Rounded rectangle with transparency. | `fx.rounded_corners(radius=40)` |

## Glitch and signal damage

| Effect | What it does | Example |
| --- | --- | --- |
| `glitch` ✓ | Channel split, sliding bands, text strips. | `fx.glitch(split=9, bands=8)` |
| `rgb_split` | Channel offset on its own, any direction. | `fx.rgb_split(angle=30, amount=8)` |
| `jpeg_crush` | Re-encode at very low quality. | `fx.jpeg_crush(quality=4)` |
| `scan_roll` | Vertical or horizontal sync roll. | `fx.scan_roll(offset=0.2)` |
| `interlace` | Even/odd line offset and combing. | `fx.interlace(offset=6)` |
| `signal_noise` | Static bands and dropouts. | `fx.signal_noise(bands=5, seed=1)` |
| `echo` | Offset translucent copies. | `fx.echo(copies=4, dx=14, fade=0.5)` |
| `prism` | Rainbow dispersion. | `fx.prism(amount=10)` |
| `anaglyph` | Red/cyan 3D look. | `fx.anaglyph(shift=10)` |
| `dead_pixels` | Stuck and dead LCD pixels. | `fx.dead_pixels(count=40, seed=1)` |
| `byte_corrupt` | Corrupt the encoded file's bytes, then decode. | `fx.byte_corrupt(hits=5, seed=2)` |
| `hologram` | Cyan scanlines, flicker, edge glow. | `fx.hologram()` |

## Atmosphere

| Effect | What it does | Example |
| --- | --- | --- |
| `fog` | Layered haze, gradient or noise-based. | `fx.fog(density=0.4, color="#cfd8e3")` |
| `rain` | Falling streaks (and optional splashes). | `fx.rain(density=0.5, angle=12, seed=1)` |
| `snow` | Flakes in depth layers. | `fx.snow(density=0.4, seed=1)` |
| `smoke` | Noise-driven smoke or steam. | `fx.smoke(amount=0.4, seed=1)` |
| `sparkle` | Twinkling points on highlights. | `fx.sparkle(count=60, seed=1)` |
| `floating_dust` | Slow bokeh particles. | `fx.floating_dust(count=80, seed=1)` |
| `lens_drops` | Water drops on the lens. | `fx.lens_drops(count=14, seed=1)` |
| `embers` | Rising sparks. | `fx.embers(count=120, seed=1)` |

## Subject-aware effects

Use the subject cut-out, so they target the subject or the background.

| Effect | What it does | Example |
| --- | --- | --- |
| `fill` ✓ | Flat colour; masked = background removal. | `fx.fill("#000").where("background")` |
| `outline` | Coloured stroke around the subject, optional glow. | `fx.outline(width=6, color="#fff", glow=12)` |
| `drop_shadow` | Soft shadow behind the subject. | `fx.drop_shadow(dx=12, dy=18, blur=24)` |
| `long_shadow` | Flat-design shadow at an angle. | `fx.long_shadow(angle=45, length=300)` |
| `rim_light` | Light along the subject's edge from a side. | `fx.rim_light(angle=40, color="#ff9a4d")` |
| `reflection` | Floor mirror with fade. | `fx.reflection(height=0.4, fade=0.7)` |
| `background` | Replace the background: colour, gradient, blur or image. | `fx.background(gradient=["#111","#2a0f4a"])` |
| `background_blur` | Portrait-mode blur. | `fx.background_blur(radius=24)` |
| `sticker` | White border and shadow: a die-cut sticker. | `fx.sticker(border=14)` |
| `silhouette` | Flat colour subject. | `fx.silhouette(color="#000", background="#ffd400")` |
| `color_pop` | Subject in colour, background in grey. | `fx.color_pop()` |
| `spotlight` | Light the subject, darken the rest. | `fx.spotlight(falloff=0.6)` |
| `aura` | Soft coloured halo around the subject. | `fx.aura(color="#7a2cff", size=40)` |
| `double_exposure` | Another picture blended inside the silhouette. | `fx.double_exposure(image="forest.jpg")` |
| `layered_cutouts` | Offset copies of the silhouette for depth. | `fx.layered_cutouts(layers=4)` |
| `subject` selection | Pick `largest`, `all` or the n-th subject when there are several. | `subject="largest"` on any masked step |

## Compositing

Combine the photo with other images, text and blend modes.

| Effect | What it does | Example |
| --- | --- | --- |
| `blend` | Combine with another layer in any blend mode. | `fx.blend(image="tex.png", mode="overlay", opacity=0.4)` |
| `overlay` | Place an image, logo or watermark. | `fx.overlay(image="logo.png", at="bottom-right", scale=0.15)` |
| `gradient_overlay` | Linear, radial or conic colour wash. | `fx.gradient_overlay(["#f0f","#0ff"], kind="linear", mode="screen")` |
| `text` | Render text with font, outline and shadow. | `fx.text("PHOTO-FX", size=0.08, at="bottom")` |
| `split_screen` | Before/after or side by side. | `fx.split_screen(vertical=True)` |
| `collage` | Several recipes' results in one layout. | `fx.collage(columns=3)` |
| `transition` | Between two images: dissolve, wipe, glitch, pixel. | `fx.transition(to="b.jpg", kind="pixel", progress=ramp(0,1))` |

## Generated backgrounds

Backgrounds made from nothing, for use behind a cut-out or under any effect.

| Effect | What it does | Example |
| --- | --- | --- |
| `gradient` | Linear, radial, conic or mesh gradient. | `fx.gradient(["#12002b","#ff2bd6"], kind="radial")` |
| `noise_field` | Perlin, simplex or Worley noise as colour. | `fx.noise_field(kind="simplex", scale=200, seed=1)` |
| `pattern` | Stripes, checks, dots, grid. | `fx.pattern(kind="grid", cell=40)` |
| `plasma` | Classic plasma colours. | `fx.plasma(scale=120, seed=1)` |
| `starfield` | Stars with depth. | `fx.starfield(count=600, seed=1)` |
| `voronoi_cells` | Cell pattern. | `fx.voronoi_cells(cells=200, seed=1)` |
| `topographic` | Contour lines of luminance or noise. | `fx.topographic(levels=14)` |
| `flow_lines` | Streamlines through a noise flow field. | `fx.flow_lines(density=0.6, seed=1)` |
| `aurora` | Soft vertical light curtains. | `fx.aurora(colors=["#2bff9a","#7a2cff"])` |

## Utilities

| Effect | What it does | Example |
| --- | --- | --- |
| `auto_enhance` | Sensible levels, white balance and vibrance in one step. | `fx.auto_enhance()` |
| `avatar` | Square crop on the subject, optional circle. | `fx.avatar(size=512, shape="circle")` |
| `upscale` | Lanczos, or an optional AI model. | `fx.upscale(factor=2)` |
| `palette_from_photo` | Extract dominant colours to feed other steps. | `palette = photo.palette(5)` |
| `sweep` (CLI) | Render one parameter across a range, side by side. | `photo-fx sweep r.toml glitch.split 0..20` |
| `compare` (CLI) | Before/after slider page. | `photo-fx compare a.jpg b.jpg` |
| `avatar pack` (export) | Every size a set of platforms asks for. | `[export] preset = "avatars"` |

## Animation

Time-based effects, plus every effect above that has a numeric parameter can be animated with `ramp()`.

| Effect | What it does | Example |
| --- | --- | --- |
| `disperse` ✓ | Pieces detach, drift on the wind and fade. | `fx.disperse(progress=ramp(0,0.9), wind=0.3)` |
| `assemble` | The reverse: pieces fly in and settle. | `fx.disperse(progress=ramp(0.9,0))` |
| `dissolve` | Pixels vanish in noise order, with a burning edge. | `fx.dissolve(progress=ramp(0,1), edge="#ff7a2b")` |
| `pixel_dissolve` | Dissolve into blocks. | `fx.pixel_dissolve(progress=ramp(0,1), cell=16)` |
| `scan_reveal` | A bright line sweeps and reveals the effect. | `fx.scan_reveal(progress=ramp(0,1), angle=90)` |
| `wipe` | Linear, iris or clock reveal. | `fx.wipe(kind="iris", progress=ramp(0,1))` |
| `type_on` | Text appears character by character. | `fx.type_on(progress=ramp(0,1))` |
| `explode` | Pieces burst radially from a point. | `fx.explode(progress=ramp(0,1), center=[0.5,0.4])` |
| `shatter_fall` | Pieces fall with gravity and spin. | `fx.shatter_fall(progress=ramp(0,1))` |
| `melt_down` | Drips run down and pool. | `fx.melt_down(progress=ramp(0,1))` |
| `paper_burn` | An ember front eats the picture. | `fx.paper_burn(progress=ramp(0,1))` |
| `flicker` | Random brightness pulses. | `fx.flicker(rate=8, depth=0.2, seed=1)` |
| `breathe` | Slow scale or brightness pulse. | `fx.breathe(period=3, amount=0.02)` |
| `camera_shake` | Smoothed-noise offsets. | `fx.camera_shake(amount=6, seed=1)` |
| `ken_burns` | Slow pan and zoom. | `fx.ken_burns(from_box=[...], to_box=[...])` |
| `parallax` | 2.5D depth shift from a depth map or layers. | `fx.parallax(amount=0.03)` |
| `trails` | Motion trails from earlier frames. | `fx.trails(length=8, fade=0.7)` |
| `slit_scan` | Time smeared across space. | `fx.slit_scan()` |
| `morph` | Blend two photos with optical flow. | `fx.morph(to="b.jpg", progress=ramp(0,1))` |

## Signature looks

What combining effects can give, once the pieces exist:

- **Cyberpunk**: split tone, chromatic aberration, scanlines, bloom, rain
- **Risograph poster**: levels, risograph inks, paper texture, border
- **Terminal**: green ASCII, CRT, scanlines, vignette
- **Vaporwave**: gradient background, grid pattern, gradient map, glitch
- **Newspaper**: CMYK halftone, paper texture, faded sepia, torn edge
- **Double exposure**: black and white, a second photo inside the silhouette, vignette
- **Sticker**: cut-out with a white border and drop shadow
- **Turn to dust** (animation): ASCII, disperse, embers, camera shake
