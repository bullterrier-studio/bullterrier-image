# Python API

The code API is the product; a TOML recipe file is a `Recipe` saved. Basics are in the [README](../README.md); this
page is the rest.

Every effect = function with real signature and docstring: editors complete names and
parameters.

```python
from bullterrier_image import Recipe, fx, imageio, render

photo = imageio.load("dog.jpg")

neon_glitch = Recipe(
    [
        fx.gradient_map(stops=("#12002b", "#ff2bd6", "#00f0ff")),
        fx.gradient(colors=["#000000"], mask="background"),  # only where the photo's background is
        fx.slice_shift(preset="glitch", bands=8, seed=4),
        fx.rgb_split(angle=0, amount=5.6, edge="wrap"),
        fx.bloom(radius=12.2, amount=0.65, opacity=0.8, blend_mode="screen"),  # screened over it at 80%
    ],
    name="neon-glitch",
    size=1024,
)

imageio.save(render(photo, neon_glitch), "dog-neon-glitch.jpg")
```

Step keys are keywords on every `fx` function: `fx.bloom(radius=12.2, opacity=0.8, blend_mode="screen")` = step `effect = "bloom"`,
`radius = 12.2`, `opacity = 0.8`, `blend_mode = "screen"`. `fx.rgb_split(amnt=8)` raises at once, lists valid parameters, same as
TOML typo. Code-built recipes go straight to pipeline: `run(Project(photos=(...), recipes=(neon_glitch,), output=...))`.
Runnable: [`examples/python_api.py`](../examples/python_api.py).

One argument by position, the thing itself: `Recipe(steps)`, `Step(effect)`, `Each(each)`; effect parameters and
everything else by keyword (`fx.bloom(12)` is a TypeError naming `radius=12`).

- **Photos in.** `render`, `trace`, `render_all`, `step.apply` and `imageio.save` take a numpy array (h, w, 3): uint8 or
  uint16 levels (divided by 255 or 65535) or floats 0..1. Greyscale, an alpha channel
  (`save` takes (h, w, 4)), `bool`, signed or wider unsigned integers, values outside 0..1 (not clipped) and a Pillow image itself are an
  `InvalidImageError` naming the fix (`np.dstack([grey] * 3)`, `np.asarray(image)`); NaN is a `NonFiniteImageError`.
- **Palette.** `palette(rgb, *, colors=5, seed=0)` is the dominant colours of a photo array as `'#rrggbb'` strings, largest
  share of the pixels first (equal shares: the lighter first), so `fx.gradient_map(stops=palette(rgb, colors=3))` and
  `fx.gradient(colors=palette(rgb))` take it as it is. `colors` is 1 to 256 and `seed` 0 to 4294967295 (a `RecipeError`
  naming the value); fewer colours come back when `rgb` has fewer distinct ones. The same `rgb`, `colors` and `seed`
  give the same list.
- **Threads.** `render(photo, recipe, threads=0)` (also `trace`, `render_all`) lets one render use every core for the work
  the core shares out by rows; `threads` is a whole number from 0, default 1 (none started), and the picture is the same
  bytes at any number. To render many photos at once keep 1 and use your own threads: the GIL is released while the core
  works. `step.apply` and `fx.<effect>` use 1. `bt-image` (`render`, `sheet`, `sweep`, `steps`, `run`) and the gallery render one picture at a
  time, so they use every core.
- **One effect.** `fx.bloom(radius=12).apply(photo)` is `render(photo, Recipe([step]))`, and takes what `render` takes
  (`maps`, `finders`, `size`); `render(photo, step)` too. The recipe is a `Recipe` or a `Step`, `render_all` takes a list
  or tuple of `Recipe`; anything else (a list of steps, a look's name, swapped arguments) is a `RecipeError` naming the fix.
- **Copies.** `step.replace(radius=20, opacity=0.5)` takes the parameters and step keys `fx` takes, except `effect`
  and `preset` (a step does not know which values its preset gave: build it again with `fx.<effect>(preset=...)`);
  `recipe.replace(name="warmer", size=64)` the recipe's fields. Checked as when made; the original is untouched. A
  picture or lookup table is a `Picture`/`Lut`, a colour or an array, as for `fx`; a path is read only by `fx`.
- **Names.** `Recipe(steps, name="my look!", description="...")`: free text, both optional. Files written for it (the
  command line, a project) are named by its name made safe (`my-look`); an unnamed recipe there is a `RecipeError`.
- **Reading values.** `step.describe(width=56)` is the short label `trace` shows (an `Each` has one too, and
  `drive.describe()`); `drive.values()` lists the values a `Drive` takes at its levels. `Step`, `Each` and `Recipe` are
  frozen dataclasses; `to_dict()` writes one as the plain data a recipe file holds and `Class.from_dict(data)` reads it back
  (a `RecipeError` naming the key). A recipe's dict starts with `format`: the lowest recipe-file format that can write
  it (1, 2 or 3; see [recipes.md](recipes.md)). `RECIPE_FORMATS`, `(1, 2, 3)`, lists the formats this release reads;
  `Recipe.from_dict` refuses another `format` (the one found and those read are named) before it reads the rest, and
  one below what the recipe needs. The regions, `Framing` and `Drive` are frozen dataclasses of keyword fields too;
  the core checks each when it is made; `help(Grid)` lists their fields, with unit and
  range.
- **A region as a map.** `region_map(photo, recipe, name, *, maps=None, finders=None, size=None)` returns the region
  `name` (`subject`, `background`, `depth`, `light`, or a declared selection such as `ColorRange`, `Band`, `Combine`) as a
  float32 `(h, w)` array in 0..1, what a step that masks by it is given, at the size `render` works at (the recipe's
  frame, then `size`, else the recipe's, else the photo's own). A step need not use the region. Only the maps the region is
  made from are needed (found by the default finders only then); no step runs. A partition or an unknown name is a
  `RecipeError` listing the selections and the nearest. Example: [`examples/python_api.py`](../examples/python_api.py).
- **Shapes.** `Path.rect(x, y, w, h)` and `Path.ellipse(cx, cy, rx, ry)` are `Path` regions (`samples=` as for `Path`) for the
  common shapes: `regions={"box": Path.rect(0.1, 0.2, 0.5, 0.3)}`. All numbers are shares of the frame (0 to 1 across and down),
  so an ellipse is a circle only in a square frame (in a 3:2 frame, `Path.ellipse(0.5, 0.5, 0.2, 0.3)` is a circle). A size
  that is not above 0 is a `RecipeError` naming it.
- **ColorRange.** `ColorRange(target="#8a3b2e", tolerance=0.1, feather=0.05)` selects the pixels near a colour (Oklab distance; read
  from the photo before any step): `regions={"brick": ColorRange(...), "glass": Band(of="brick", low=0, high=0.3)}`.
- **What ships.** `bullterrier_image.looks()`: `{name: description}`, each loads with `load_recipe(name)`.
  `fx.<effect>.presets`: `{preset: {parameter: value}}`, used as `fx.<effect>(preset=...)`. A misspelt parameter or
  look name gets the nearest ones.
- **A piece left as it is** in an `Each` is `None`: `Each("panes", looks=[fx.halftone(), None])`.

Every effect declares parameters once (as a plain `EffectParams` class for a plugin, or, for an effect the Rust core runs, in its Rust
struct): description, unit, range, default. Validation, `fx` signatures,
`help(fx.<effect>)`, `bt-image effects`, [docs/effects/](effects/README.md) all come from it, so
`fx.grade(exposure=12)` fails at once naming effect, field, limit, value.

## Addresses

Every number of recipe has address. Program driving a look (video tool mapping beat onto bloom, frame after
frame) reads numbers and sets them without rebuilding recipe:

```python
from bullterrier_image import load_recipe

look = load_recipe("pulse.toml")
for address, number in look.parameters().items():  # pulse.radius 24.0 px 0 500, pulse.exposure.end 0.4 stops -10 10 ...
    print(address, number.value, number.unit, number.minimum, number.maximum, number.whole)

frame_look = look.with_values({"pulse.radius": 12 + 20 * kick, "pulse.exposure.end": 0.4 * level})  # a copy
```

Every number of a recipe (not colour, choice, list, picture) has an address, so a program driving a look (video tool mapping
a beat onto a bloom, frame after frame) can read the numbers and set them without rebuilding the recipe.

- A step's numbers start with its `name` (`name = "pulse"` on a `[[step]]`, `fx.bloom(name="pulse")`: letters, digits,
  underscores, one per step; address survives reordering). No name: the effect, `bloom`, when recipe has one such step, else
  effect and its number from 1, `bloom#2`; an `each` step is `each`.
- After the step: effect's numbers (`pulse.radius`), its `opacity`, `.start`, `.end`, `.levels` of a driven number
  (`pulse.radius.end`); for `each`: its `feather` and numbers of a look (`each.looks[0].radius`, counted from 0). Then
  `region.near.low` for region numbers, `frame.angle` for framing's.
- `parameters()` lists them in recipe order, each a `Parameter`: `value` (default where recipe does not write it), `unit`,
  `minimum`, `maximum`, `doc`, `whole` (must be whole number), `cyclic`.
- `with_values` returns a new recipe (original untouched; unchanged steps, regions, frame shared), checks each value against
  the range as any recipe does (cyclic angle takes any real, stores same direction inside its turn), refuses an address the
  recipe lacks (naming it, listing valid ones and nearest). Whole number may be written `3.0`. `bt-image sweep` takes the same
  addresses.

## One look at any size

Pixel values (numbers in `px`: radius, cell, shift) are design pixels: pixels on a picture whose long side is 480, whatever
the output size. `Recipe(size=None)`, the default, renders at the photo's own size, so on a 4000 px photo every pixel value is
multiplied by 4000/480. `render(photo, recipe, size=480)` (also `trace`, `render_all`) makes the picture 480 px longest side
with the same multiplication (ends of a driven number too): same look at draft and full size, differing in detail. Whole number rounded, value stops at its minimum when shrinking, growing has no ceiling.

## Cyclic angles

Angle in degrees whose value and value 360 further give the same picture (direction, rotation, hue) is
`(cyclic)` in the docs: range = one turn, any real accepted, same direction inside it (270 on -180..180 is -90). Other angles
(elevation, lean) keep range and refuse outside.

## Pointing at a subject

`subject_matte(rgb, segmenter="sam2", points=[(x, y, positive), ...], box=(left, top, right, bottom))` and
`subject_finder("sam2", points=..., box=...)` (a `Finder` for `render(..., finders={"subject": ...})`) find what the clicks
and box point at: shares of the picture, from 0 to 1, `positive=False` for a click on what to leave out. With neither it is a
`ValueError` naming `points`, as is a share outside 0 to 1, more than 64 clicks, a box with no area, or clicks given to
another segmenter. The `Finder`'s identity holds the prompts, so a moved click is a new find. See
[subject and depth](subject-and-depth.md).

## Writing a plugin

Effects can live in own package. Write against `bullterrier_image.plugin`:

```python
import numpy as np
from bullterrier_image.plugin import Color, EffectParams, effect, param


class TintParams(EffectParams):
    """Settings of the tint."""

    colour: Color = param("#ff8800", doc="The colour mixed in.")
    amount: float = param(0.3, unit="fraction", min=0, max=1, doc="How much of it, 0..1. Raising it tints more.")


@effect(category="colour", params=TintParams)
def tint(rgb: np.ndarray, params: TintParams) -> np.ndarray:
    """Mixes a flat colour into the picture."""
    return (rgb * (1 - params.amount) + np.asarray(params.colour, np.float32) * params.amount).astype(np.float32)
```

Declare module as entry point in your `pyproject.toml`, install: `tint` then works by name in TOML recipes, as `fx.tint(...)`,
in `bt-image effects`:

```toml
[project.entry-points."bullterrier_image.effects"]
my_effects = "my_package.effects"
```

**Plugins.** Plugin that fails to load stops first effect lookup with error naming it; `BT_IMAGE_PLUGINS=builtin` loads
no package. Effect names unique across all installed packages. Import is quick: effects load on first ask, once, under a
lock, so threads render at same time; register effects at import time only. A thread must not modify a photo (or a map)
while another thread renders it: rendering reads the array in place without a copy. Give each thread its own frame, or
share one read-only (`array.flags.writeable = False`).
