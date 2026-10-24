"""bullterrier-image: style photos with composable effects, optionally on a cut-out subject.

    from bullterrier_image import Recipe, fx, render, imageio

    photo = imageio.load("dog.jpg")
    look = Recipe("neon", size=1024, steps=[
        fx.gradient_map(stops=["#12002b", "#ff2bd6", "#00f0ff"]),
        fx.fill("#000000").where("background"),
        fx.glow(radius=26),
    ])
    imageio.save(render(photo, look), "dog-neon.jpg")

    from bullterrier_image import trace                       # what did each step do?
    for stage in trace(photo, look):
        print(stage.label)

Layers, each importing only from the ones below it (import-linter checks this in the test suite):
    app        cli, pipeline (many photos x many recipes), gallery (the effect catalogue), sheet (comparison images),
               checks (automatic quality checks), fx (effects as step builders), recipe (the public namespace)
    adapters   imageio (files and the one in-memory image type), matte (finding the subject), loader (recipe files)
    effects    the effect library: pure functions image -> image, found automatically
    core       the registry and the recipe engine (steps, runner)
    ops        shared numeric primitives, named by concept

Importing bullterrier_image is cheap: each public name loads its module the first time it is used, and the effects load
on the registry's first query.
"""

from importlib import import_module
from typing import TYPE_CHECKING

if TYPE_CHECKING:  # what type checkers and editors see; at run time `__getattr__` imports each name on first use
    from . import fx, imageio, recipe
    from .core.params import Picture
    from .errors import (
        BackendUnavailableError,
        BullterrierImageError,
        ImageReadError,
        NonFiniteImageError,
        ProjectError,
        RecipeError,
        SubjectNotFoundError,
        UnknownEffectError,
    )
    from .matte import subject_matte
    from .pipeline import Project, load_project, run
    from .recipe import Recipe, Stage, Step, load_recipe, render, render_all, trace

    __version__: str

# Each public name by the module it lives in ("." for the submodules themselves). Importing these eagerly would load
# numpy, Pillow, pydantic and scipy for a bare `import bullterrier_image`, about 0.25 s, before anything is used (PEP
# 562).
_HOMES = {
    ".": ("fx", "imageio", "recipe"),
    ".errors": (
        "BackendUnavailableError", "ImageReadError", "NonFiniteImageError",
        "BullterrierImageError", "ProjectError", "RecipeError", "SubjectNotFoundError", "UnknownEffectError",
    ),
    ".core.params": ("Picture",),
    ".matte": ("subject_matte",),
    ".pipeline": ("Project", "load_project", "run"),
    ".recipe": ("Recipe", "Stage", "Step", "load_recipe", "render", "render_all", "trace"),
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
    because importlib.metadata alone costs more than the rest of `import bullterrier_image`."""
    from importlib import metadata

    try:
        return metadata.version("bullterrier-image")
    except metadata.PackageNotFoundError:  # run from a source tree that was never installed
        return "0+unknown"


__all__ = [
    "BackendUnavailableError",
    "BullterrierImageError",
    "ImageReadError",
    "NonFiniteImageError",
    "Picture",
    "Project",
    "ProjectError",
    "Recipe",
    "RecipeError",
    "Stage",
    "Step",
    "SubjectNotFoundError",
    "UnknownEffectError",
    "__version__",
    "fx",
    "imageio",
    "load_project",
    "load_recipe",
    "recipe",
    "render",
    "render_all",
    "run",
    "subject_matte",
    "trace",
]  # fmt: skip
