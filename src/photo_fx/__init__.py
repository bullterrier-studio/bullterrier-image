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

Importing photo_fx is cheap: each public name loads its module the first time it is used, and the effects load on the
registry's first query.
"""

from importlib import import_module
from typing import TYPE_CHECKING

if TYPE_CHECKING:  # what type checkers and editors see; at run time `__getattr__` imports each name on first use
    from . import fx, imageio, recipe
    from .errors import (
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

    __version__: str

# Each public name by the module it lives in ("." for the submodules themselves). Importing these eagerly would load
# numpy, Pillow, pydantic and scipy for a bare `import photo_fx`, about 0.25 s, before anything is used (PEP 562).
_HOMES = {
    ".": ("fx", "imageio", "recipe"),
    ".errors": (
        "BackendUnavailableError", "ImageReadError", "PhotoFxError", "ProjectError", "RecipeError",
        "SubjectNotFoundError", "UnknownEffectError",
    ),
    ".matte": ("subject_matte",),
    ".pipeline": ("Project", "load_project", "run"),
    ".recipe": (
        "Animation", "Recipe", "Stage", "Step", "load_recipe", "ramp", "render", "render_all", "render_frames", "trace",
    ),
}  # fmt: skip
_HOME = {name: module for module, names in _HOMES.items() for name in names}


def __getattr__(name: str) -> object:
    """A public name (or `__version__`), imported from its module the first time it is asked for, then kept."""
    if name == "__version__":
        value: object = _installed_version()
    elif (home := _HOME.get(name)) is None:
        raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
    else:
        value = import_module(f".{name}", __name__) if home == "." else getattr(import_module(home, __name__), name)
    globals()[name] = value
    return value


def __dir__() -> list[str]:
    return sorted({*globals(), *__all__})


def _installed_version() -> str:
    """The installed distribution's version; pyproject.toml is the one place it is written. Looked up on first use
    because importlib.metadata alone costs more than the rest of `import photo_fx`."""
    from importlib import metadata

    try:
        return metadata.version("photo-fx")
    except metadata.PackageNotFoundError:  # run from a source tree that was never installed
        return "0+unknown"


__all__ = ["__version__", *_HOME]  # noqa: PLE0604  (the names of the table above: one list, not a copy of it)
