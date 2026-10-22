"""Recipes: the one public place to import everything about them from.

A recipe is an ordered list of effects (written in Python or in a small TOML file) that is run on a photo and can be
animated. The work is split by what changes together, and this module only re-exports the public names:

    timing   `Animated`, `ramp`, `Animation`: values changing over a clip, easing
    steps    `Step`, `Recipe`: the data model of a look and its validation
    loader   `load_recipe`: the TOML format
    runner   `render`, `render_frames`, `render_all`, `trace`, `Stage`: running a recipe on a photo

Dependency direction (acyclic): timing <- steps <- loader, runner; this module imports all four, and only the
package's `__init__` imports it (so that `photo_fx.recipe` exists after a plain `import photo_fx`).
`from photo_fx.recipe import ...` keeps working; code in the package imports from the module that owns the name.
"""

from .loader import load_recipe
from .runner import Stage, render, render_all, render_frames, trace
from .steps import Recipe, Step
from .timing import Animated, Animation, ramp

__all__ = [
    "Animated", "Animation", "Recipe", "Stage", "Step", "load_recipe", "ramp", "render", "render_all", "render_frames",
    "trace",
]  # fmt: skip
