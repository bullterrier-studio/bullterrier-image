# Recipes

A recipe is an ordered list of effects, in TOML or Python. Basics are in the [README](../README.md); this page is the rest:
step keys, regions, fields, driven numbers, framing, presets. Python equivalents and addresses: [python-api.md](python-api.md).

## The recipe file

Recipe = ordered list of effects. Each `[[step]]` names an effect; other keys are its parameters.

```toml
name = "ember-glitch"
size = 1024                       # optional: longest side in px the recipe is written for; absent = tuned for 480, rendered at the photo's own size

[[step]]
effect = "gradient_map"           # a fire palette
stops = ["#0a0000", "#e0261b", "#ffb347", "#fff3d6"]

[[step]]
effect = "gradient"
colors = ["#000000"]
mask = "background"               # "subject", "background" or a region: apply only there

[[step]]
effect = "slice_shift"
preset = "glitch"
bands = 10
amount = 51.2                     # how far a strip may slide, in px (written for the size above)
seed = 4                          # same seed, same result, every time

[[step]]
effect = "rgb_split"              # red right, blue left, wrapping
angle = 0
amount = 10
edge = "wrap"

[[step]]
effect = "bloom"
blend_mode = "screen"             # lighten what is below, as light does
opacity = 0.6                     # mix this step in at 60%
```

Replace the background = a `gradient` step with preset `fill` (black; set `colors` for another) and `mask = "background"`. Unknown effects, misspelt parameters, bad values fail at load, naming
file and step.

`size`: longest side in px the recipe is written for. Absent (the default for a file), the pixel values are tuned for 480 (the size
effect defaults are tuned for) and the picture renders at the photo's own size, so a 4000 px photo multiplies them by 4000/480.
A Python `Recipe` without `size` (its default, `size=None`) is the same.

**Step keys.** Every step takes same four keys besides `effect` and its parameters. Runner applies them after any effect: each
step is a layer over picture below, as in image editor. Result combined by blend mode, mixed by opacity, only where mask says.

| Key | Values | What it does |
|---|---|---|
| `mask` | `"subject"`, `"background"`, or a selection region's name | Apply only there. No subject (empty map): `"subject"` selects nothing, `"background"` whole picture. |
| `opacity` | 0 to 1, default 1 (or effect's own) | How much shows. Effects meant to mix in partway (shadow, rain) set own default, shown in docs; step's `opacity` replaces it. |
| `blend_mode` | `normal` (default), `multiply`, `screen`, `overlay`, `soft_light`, `difference`, `color` | How result meets picture below. W3C blend modes: `multiply` darkens, `screen` lightens, `overlay`/`soft_light` deepen contrast, `difference` subtracts, `color` takes hue and saturation, keeps brightness below. |
| `preset` | preset's name | Start from effect's named parameter set: `preset = "sepia"` on `gradient_map`. Own parameters win. |

No effect parameter shares these names, so key means same in every step, and in Python: `fx.bloom(opacity=0.6, blend_mode="screen")`.
Gradient screened over photo = ordinary step with `blend_mode`. `blend` effect is for second picture of your own
(`image = "grain.png"`).

## Regions

`[region.NAME]` declares one by `kind`. `format = 2`: `grid` (columns and rows, equal or in shares), `shards` (irregular pieces
like glass or stone; presets `glass`, `torn`, `stone`, `mosaic`), `cuts` (straight and curved lines), `svg` (shapes of an SVG
file), `path` (inside a closed SVG path), `pick` (some pieces of a partition), `subject`, `given` map. `format = 3` adds
`band`, `bands`, `combine`, `linear_gradient`, `radial_gradient`.

- **Partition**: every pixel in one piece, numbered from 0 in reading order (`bt-image steps` draws them numbered): `grid`,
  `shards`, `cuts`, `svg`, `bands`.
- **Selection**: values 0..1: `path`, `pick`, `subject`, `given`, `band`, `combine`, gradients, built-in `subject`,
  `background`, `depth`, `light`.
- `fit = "subject"` lays a kind over the subject instead of whole picture. `edge = { style = "torn" }` tears its borders.
- A step uses regions by name: `mask` limits to a selection; `each` + `looks` gives every piece its own look (each look
  rendered once over whole picture: cost = number of looks, not pieces); `partition` names the partition `seams` draws
  along. Unused region costs nothing. Names and kinds checked when recipe is made.

Cuts, paths and SVG files, in the units every region uses (positions = shares of the frame: the picture, or box of the `fit`
selection):

```toml
format = 2

[region.diagonal]                  # a straight cut: through (x, y) at an angle in degrees, counter-clockwise from horizontal
kind = "cuts"
lines = [{ x = 0.5, y = 0.5, angle = 62 }, { path = "M 0.38 0 C 0.15 0.45, 0.85 0.55, 0.62 1" }]  # and a curve

[region.blob]                      # a selection: inside a closed path (M L H V C S Q T A Z, absolute or relative)
kind = "path"
d = "M 0.08 0.42 C 0.1 0.25, 0.32 0.22, 0.38 0.38 C 0.44 0.52, 0.36 0.74, 0.22 0.72 Z"

[region.shapes]                    # every shape of the file is a piece
kind = "svg"
file = "shapes.svg"                # relative to the recipe file

[region.leaf]
kind = "pick"
of = "shapes"
pieces = ["leaf", 0]               # a shape by its id, or a piece by its number
```

- **Units.** `path`/curve coordinates run 0 to 1 across and down; unit square stretched to the frame (arc `A 0.25 0.25` is a
  circle only in a square frame). Line `angle` measured on picture pixels. Nonzero fill rule decides inside a `path` (inner
  outline drawn the other way = hole). Border soft by share of each pixel covered (`samples` = scanlines measuring it). Curve
  cut should run border to border; an end that does not is carried straight on to nearest border.
- **SVG files** placed by `viewBox` and `preserveAspectRatio` (default: fitted inside, centred; no `viewBox`: `width` and
  `height` are the view box). Each `rect`, `circle`, `ellipse`, `polygon`, `polyline`, `path` = a piece. Colours and strokes
  ignored, `fill-rule` kept (holes are holes), later shapes on top, `use` drawn where used. Area outside every shape = one
  more piece, numbered like any piece by its first pixel. `pick` takes a shape by `id`: `pieces = ["leaf"]`.
- **SVG file = untrusted input.** Nothing in it runs or fetches (scripts, links, external images ignored). Refused with error
  naming limit and value: declares XML entities, larger than `max_kilobytes` (default 1024), more than `max_shapes` shapes
  (default 1000, counting what each `use` repeats), nests deeper than 64 levels.

**Grid first.** `kind = "subject"` with `per` runs subject finder inside each piece of a partition and unites the finds, so
small things a whole-picture search misses show up (a calf behind its mother). Without `per` it is the built-in `subject`:

```toml
[region.cells]
kind = "grid"
columns = 3
rows = 4

[region.found]
kind = "subject"
per = "cells"            # one run of the subject finder per cell; `found` is a selection like any other
context = 0.25           # the finder sees each cell grown by this share of its size on every side (default 0.25)
threshold = 0.9          # keep only pixels the finder is surer of than this: it invents faint subjects in plain cells
```

`bt-image run` remembers what finder found per cell (finder, photo, cell pixels): repeat run asks model nothing. A `subject`
map you hand in does not stand in for the finder here (a cell is a part of the picture).

**Field kinds.** `band` = selection: pixels where field of `of` lies `low` to `high`, fading out linearly over `feather` beyond
both (shares of field's 0..1 scale; `low` above `high` = error). `bands` = partition into `count` equal ranges, numbered
**highest first** (only exception to reading order): piece 0 = nearest of depth, brightest of light; empty range dropped,
rest renumbered. `combine` = product (`multiply`, all), least (`min`), greatest (`max`, any). `linear_gradient` rises across
the picture along `angle` (counter-clockwise from right: 90 rises upward), 0 at one edge, 1 at other. `radial_gradient` rises
from centre (`x`, `y`, shares) to 1 at `radius`, share of long side. Band of gradient = tilt-shift zone or vignette ring; band
of light = shadows or highlights; subject combined with shadows = shadows on subject.

Version 2 of the file format named depth kinds `depth` and `depth_bands`. Now `band` and `bands` with `of = "depth"` (`far` is
`low`, `near` is `high`); old file says so. Finished examples: [`examples/regions/`](../examples/regions/)
([pictures](gallery/regions/README.md)), [`examples/fields/`](../examples/fields/) ([pictures](gallery/fields/README.md)).

### A look per piece

Recipe says *where* via regions: `grid`, `shards` (broken glass, stone), `cuts`, `svg`, `path`, `pick`, `subject`, `given` map,
and (`format = 3`) `band`, `bands`, `combine`, gradients. `mask` limits a step to a selection; `each` + `looks` gives every
piece of a partition its own look:

```toml
format = 2

[region.glass]
kind = "shards"
preset = "glass"
fit = "subject"

[[step]]
each = "glass"
looks = [{ effect = "halftone", cell = 8 }, "none", { effect = "gradient_map", preset = "cyanotype" }]  # "none": as it was
assign = "random"                 # or "cycle" (the default): how the pieces get looks; seed = 4 makes it repeatable
mask = "subject"

[[step]]
effect = "seams"
partition = "glass"
width = 1
```

Python:

```python
from bullterrier_image import Each, Recipe, Shards, fx

glass = Recipe(
    "glass",
    regions={"glass": Shards(preset="glass", fit="subject")},
    steps=[
        Each(
            "glass",
            [fx.halftone(cell=8), "none", fx.gradient_map(preset="cyanotype")],
            assign="random",
            seed=4,
            mask="subject",
        ),
        fx.seams(partition="glass", width=1),
    ],
)
```

## Fields

Selection also has *values*; *field* = selection read for them: `depth` (1 = nearest), `light` (picture's own brightness, no
model needed), `subject`, a `given` map, a gradient. `band` keeps where field lies `low` to `high`, `bands` splits into equal
ranges (highest first), `combine` multiplies/min/max several:

```toml
format = 3

[region.front]
kind = "band"
of = "depth"
low = 0.6
high = 1.0
feather = 0.1

[region.tones]                    # the picture's own brightness in three ranges: brightest first
kind = "bands"
of = "light"
count = 3

[region.shadows]
kind = "band"
of = "light"
low = 0
high = 0.3

[region.dark_subject]
kind = "combine"
of = ["subject", "shadows"]

[[step]]
each = "tones"                    # highlights, midtones, shadows
looks = ["none", { effect = "grade", saturation = 0.7 }, { effect = "grade", saturation = 0.4, exposure = 0.3 }]

[[step]]
effect = "sharpen"
preset = "clarity"
mask = "front"                    # only what is near

[[step]]
effect = "bloom"
mask = "dark_subject"             # only the shadows on the subject
```

## Driving a parameter

Field can also vary a number across the picture. Docs mark such numbers `(drivable)`: result changes
smoothly with value (blur radius, amount, exposure, saturation). Write number as table: `by` (field), value at field 0
(`start`) and 1 (`end`):

```toml
format = 3

[region.far]                      # 1 where the depth is up to 0.5 (the far half), fading to 0 by 0.6
kind = "band"
of = "depth"
low = 0
high = 0.5
feather = 0.1

[[step]]
effect = "gaussian_blur"
radius = { by = "far", start = 0, end = 12 }     # sharp up close, blurred far away

[[step]]
effect = "grade"
exposure = { by = "light", start = 0.4, end = 0, levels = 3 }   # lift the shadows, leave the highlights
saturation = { by = "light", start = 1.2, end = 1, levels = 3 }    # and give them a little more colour
```

Python: `fx.grade(exposure=Drive(by="light", start=0.4, end=0))`. Cost: effect renders `levels` times (2 to 16, default 5),
pixels mix neighbouring renders. Rules follow.

A field = a selection read for its values: built-in `depth` (1 = nearest), `light` (picture's own brightness, made from the
picture as recipe receives it, before first step: no map or model), `subject`, a `given` map, a gradient, any selection.

Effect docs mark numbers a field may drive: `(drivable)` = result changes smoothly with value (blur radius, amount, exposure,
saturation). Write the number as a table: `by` (field), values at field 0 (`start`) and 1 (`end`). Python:
`fx.grade(exposure=Drive(by="light", start=0.4, end=0))` (`from bullterrier_image import Drive`).

- `start`, `end` and every value between must lie in the number's range (a cyclic angle takes any). Bad one fails when step is
  made, naming effect, number, value. Same for a number not drivable (whole number, choice, anything effect does not declare).
- All tables of one step name one `by` and one `levels`. Step still takes `mask`, `opacity`, `blend_mode`. A look of an `Each`
  can be driven.
- **Cost.** Effect renders at `levels` evenly spaced values between `start` and `end` (2 to 16, default 5). Each pixel mixes
  the two renders either side of its field value, linearly: field 0.5 with 5 levels = exactly render at middle value. Driven
  step costs up to `levels` times the effect; value no pixel needs is not rendered (field 0 everywhere = one render). More
  levels = smoother where field changes slowly; 2 levels = mix of effect at `start` and `end`.

## Naming a field

An effect may work from a field: its docs list a parameter of type `field name` (`field`). Write the name of a selection, as
for `mask` and `by`: `depth`, `light`, `subject`, `background` or a region of the recipe. In a step: `field = "light"`; in
Python: `field="light"`. Only the fields some step names are built (no depth model runs for `light`).

Checked when the recipe is made: the name must be a selection. Not a number: no address, never driven. A step may name a
field and drive another number; the field is built once and every level of the drive gets it. A map the field needs and
nobody gave is a `MapNotFoundError` (`DepthNotFoundError`, `SubjectNotFoundError`).

## Framing and cut-outs

`[frame]` table crops, turns, flips, pads photo and subject matte together before first step, then `size` scales it, and can
cut finished picture out after last. Crop follows subject:

```toml
size = 512

[frame]
aspect = 1                        # crop to this width over height...
anchor = "subject"                # ...around the subject (or "center", "top", "bottom", "left", "right")
margin = 0.1                      # room kept around the subject, as a share of its size
transparent = "circle"            # "background" (the subject), "circle" or "rounded": the render has an alpha channel
```

`transparent` renders to PNG or WebP (RGBA), never JPEG.

Order is fixed: `flip`, `angle` (with `fit`, `resample`), crop, `pad_*` borders (with `fill`), then `size`. Crop follows the
subject, so masked step after it still finds the subject. `Recipe(..., frame=Framing(...))` is the same in Python;
`Framing.documentation()` lists every field with unit and range. Recipe with `transparent` renders to PNG or WebP (RGBA),
never JPEG: project exporting it to `jpg` fails before anything renders.

**`bt-image cutout` or `transparent = "background"`?** Both cut the subject out, for different jobs. `bt-image cutout photo.jpg
-o dog.png` makes a cut-out *file*: photo at its own size with subject's alpha, optional edge clean-up (`--shrink` pulls in,
`--feather` softens), no recipe. Make one when cut-out is an input you reuse and check once: `--map subject=` of a later run
(`maps={"subject": ...}`), so subject is not found again, or a picture parameter such as `blend`'s `image`
(`examples/assets/bullterrier-cutout.png` is one). `transparent = "background"` in `[frame]` ends a *look*: finished picture,
recipe's size, after every step, cut out around subject (sticker, avatar).

## Presets

Some built-in effects ship named parameter sets: `preset = "sepia"` on a `gradient_map` step, or
`fx.gradient_map(preset="sepia")`. Parameters written next to it win (`fx.gradient_map(preset="sepia", shadow_clip=0.05)`).
`bt-image effects gradient_map` lists presets. Data in `src/bullterrier_image/presets/<effect>.toml`, checked like any step;
third-party effects have none.

## Looks

A look that needs several steps (`anaglyph`, `crt`, `glitch`, `hologram`, `tilt-shift`, `stained-glass`, `low-poly`, `spotlight`, `voronoi-cells`) is a
built-in recipe, listed under "Looks" by `bt-image effects`. Give its name where a recipe file goes: `load_recipe("crt")`,
`bt-image render photo.jpg crt`, `bt-image sweep photo.jpg glitch rgb_split.amount 0..20`. Use a preset for one effect with a
set of values, a look for several. A name that is a `str` is the look even when a file of that name exists; write `./crt` or
pass a `Path` for the file. Data in `src/bullterrier_image/looks/<name>.toml`: a recipe file with a top-level `description`.
Naming a preset or a look where an effect goes (`effect = "glitch"`, `fx.crt`) fails saying where the name lives: a preset, a look, a parameter of an effect, or the effects whose summary mentions it.
