"""photo-fx: cut a person out of a photo and style the result with composable effects.

    from photo_fx import imageio, load_recipe, render

    photo = imageio.load("me.jpg")
    imageio.save(render(photo, load_recipe("recipes/ascii-green.toml")), "me-ascii.png")

Modules, each owning one decision:
    imageio   file formats and the one in-memory image representation
    matte     finding the person (which model is hidden here)
    effects   the effect library: pure functions image -> image
    recipe    effect lists in TOML, and running them (with optional person masks)
    sheet     comparison images
    cli       the `photo-fx` command
"""

from . import imageio
from .matte import subject_matte
from .recipe import Recipe, Step, load_recipe, render, render_all

__all__ = ["Recipe", "Step", "imageio", "load_recipe", "render", "render_all", "subject_matte"]
