"""photo-fx: style photos with composable effects, optionally on a cut-out subject, and animate them.

    from photo_fx import Recipe, fx, render, imageio

    photo = imageio.load("dog.jpg")
    look = Recipe("neon", size=1024, steps=[
        fx.gradient_map(stops=["#12002b", "#ff2bd6", "#00f0ff"]),
        fx.fill("#000000").where("background"),
        fx.glow(radius=26),
    ])
    imageio.save(render(photo, look), "dog-neon.jpg")

    from photo_fx import trace                       # what did each step do?
    for stage in trace(photo, look):
        print(stage.label)

Modules, each owning one decision:
    imageio    file formats and the one in-memory image representation
    matte      finding the subject: person, pet or object (which model is hidden here)
    effects    the effect library: pure functions image -> image
    fx         the same effects as validated steps, for building recipes in Python
    recipe     effect lists (Python or TOML), running them, tracing them, animating them
    pipeline   many photos x many recipes: caching, automatic checks, export, report
    checks     automatic quality checks
    sheet      comparison images
    cli        the `photo-fx` command
"""

from . import fx, imageio
from .matte import subject_matte
from .pipeline import Project, load_project, run
from .recipe import Animation, Recipe, Stage, Step, load_recipe, ramp, render, render_all, render_frames, trace

__all__ = [
    "Animation", "Project", "Recipe", "Stage", "Step", "fx", "imageio", "load_project", "load_recipe",
    "ramp", "render", "render_all", "render_frames", "run", "subject_matte", "trace",
]  # fmt: skip
