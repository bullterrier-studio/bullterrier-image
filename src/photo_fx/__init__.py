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

Layers, each importing only from the ones below it (import-linter checks this in the test suite):
    app        cli, pipeline (many photos x many recipes), gallery (the effect catalogue), sheet (comparison images),
               checks (automatic quality checks), fx (effects as step builders), recipe (the public namespace)
    adapters   imageio (files and the one in-memory image type), matte (finding the subject)
    effects    the effect library: pure functions image -> image, found automatically
    core       the registry and the recipe engine (timing, steps, loader, runner)
    ops        shared numeric primitives, named by concept
"""

from importlib import metadata

from . import fx, imageio, recipe  # binds `photo_fx.recipe`, which callers use as a namespace
from .core.errors import (
    BackendUnavailableError,
    ImageReadError,
    PhotoFxError,
    ProjectError,
    RecipeError,
    SubjectNotFoundError,
    UnknownEffectError,
)
from .matte import subject_matte
from .pipeline import Project, load_project, run
from .recipe import Animation, Recipe, Stage, Step, load_recipe, ramp, render, render_all, render_frames, trace

try:
    __version__ = metadata.version("photo-fx")  # pyproject.toml is the one place the version is written
except metadata.PackageNotFoundError:  # run from a source tree that was never installed
    __version__ = "0+unknown"

__all__ = [
    "Animation", "BackendUnavailableError", "ImageReadError", "PhotoFxError", "Project", "ProjectError", "Recipe",
    "RecipeError", "Stage", "Step", "SubjectNotFoundError", "UnknownEffectError", "__version__", "fx", "imageio",
    "load_project", "load_recipe", "ramp", "recipe", "render", "render_all", "render_frames", "run", "subject_matte",
    "trace",
]  # fmt: skip
