# Python API

Same recipes as the TOML files, written with `fx`. Basics are in the [README](../README.md); this page is the rest.

Every effect = function with real signature and docstring: editors complete names and
parameters.

```python
from bullterrier_image import Recipe, fx, imageio, render

photo = imageio.load("dog.jpg")

neon_glitch = Recipe(
    "neon-glitch",
    size=1024,
    steps=[
        fx.gradient_map(stops=("#12002b", "#ff2bd6", "#00f0ff")),
        fx.gradient(colors=["#000000"], mask="background"),  # only where the photo's background is
        fx.slice_shift(preset="glitch", bands=8, seed=4),
        fx.rgb_split(angle=0, amount=12, edge="wrap"),
        fx.bloom(radius=26, amount=0.65, opacity=0.8, blend_mode="screen"),  # screened over it at 80%
    ],
)

imageio.save(render(photo, neon_glitch), "dog-neon-glitch.jpg")
```

Step keys are keywords on every `fx` function: `fx.bloom(radius=26, opacity=0.8, blend_mode="screen")` = step `effect = "bloom"`,
`radius = 26`, `opacity = 0.8`, `blend_mode = "screen"`. `fx.rgb_split(amnt=8)` raises at once, lists valid parameters, same as
TOML typo. Code-built recipes go straight to pipeline: `run(Project(photos=(...), recipes=(neon_glitch,), output=...))`.
Runnable: [`examples/python_api.py`](../examples/python_api.py).

Every effect declares parameters once, as frozen Pydantic model: description, unit, range, default. Validation, `fx`
signatures, `help(fx.<effect>)`, `bt-image effects`, [docs/effects/](effects/README.md) all come from it, so
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

Pixel values (numbers in `px`: radius, cell, shift) are written for recipe's `size`, or photo's own
longest side without one. `render(photo, recipe, size=480)` (also `trace`, `render_all`) makes the picture 480 px longest side
and multiplies every pixel value (ends of a driven number too) by 480 over that size: same look at draft and full size,
differing in detail. Whole number rounded, value stops at its minimum when shrinking, growing has no ceiling.

## Cyclic angles

Angle in degrees whose value and value 360 further give the same picture (direction, rotation, hue) is
`(cyclic)` in the docs: range = one turn, any real accepted, same direction inside it (270 on -180..180 is -90). Other angles
(elevation, lean) keep range and refuse outside.

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

**Plugins.** Plugin that fails to load stops first effect lookup with error naming it; `BT_IMAGE_PLUGINS=builtin` loads only
own effects. Effect names unique across all installed packages. Import is quick: effects load on first ask, once, under a
lock, so threads render at same time; register effects at import time only.
