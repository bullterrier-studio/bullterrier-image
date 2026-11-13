"""Recipes: the one public place to import everything about them from.

A recipe is an ordered list of effects (written in Python or in a small TOML file) that is run on a photo, and the
regions (`Grid`, `Shards`, `Cuts`, `Path`, `Svg`, `Pick`, `Subject`, `Band`, `Bands`, `Components`, `Combine`,
`ColorRange`, `LinearGradient`, `RadialGradient`, `Given`) its steps name: `mask` limits a step to one, `Each` gives
each piece of one its own look. The machinery lives in `bullterrier_image.core` (`steps`, and `bridge`, which runs a
recipe in the Rust core) and `bullterrier_image.loader`; this module re-exports its public names and binds what the core
leaves open:
how the subject and the depth are found. When a recipe will ask for the subject (it needs the subject and no `subject`
map is given, or it finds the subject piece by piece) and `finders` has no `subject`, `render`, `trace` and
`render_all` find it with `bullterrier_image.finding.subject.subject_finder()`; when a recipe needs depth and neither a
`depth` map nor a `depth` finder is given, with `bullterrier_image.finding.depth.depth_finder()`. Each is built only
then, so a recipe that needs neither never depends on the model settings. Any other map (a `Given` region's) is handed
in by name in `maps`."""

from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from typing import cast

import numpy as np

from .core import bridge, native
from .core import looks as shipped_looks
from .core.bridge import maps_needed, needs_finder
from .core.finder import Finder
from .core.framing import Framing
from .core.pieces import Drive
from .core.regions import (
    FOUND_MAPS,
    Band,
    Bands,
    ColorRange,
    Combine,
    Components,
    Curve,
    Cuts,
    Edge,
    Given,
    Grid,
    Line,
    LinearGradient,
    Path,
    Pick,
    RadialGradient,
    Shards,
    Subject,
    Svg,
)
from .core.steps import Each, Parameter, Recipe, Step
from .errors import RecipeError
from .finding import subject
from .finding.depth import depth_finder
from .loader import load_recipe

__all__ = [
    "Band",
    "Bands",
    "ColorRange",
    "Combine",
    "Components",
    "Curve",
    "Cuts",
    "Drive",
    "Each",
    "Edge",
    "Finder",
    "Framing",
    "Given",
    "Grid",
    "Line",
    "LinearGradient",
    "Parameter",
    "Path",
    "Pick",
    "RadialGradient",
    "Recipe",
    "Shards",
    "Stage",
    "Step",
    "Subject",
    "Svg",
    "load_recipe",
    "looks",
    "render",
    "render_all",
    "trace",
]


def looks() -> dict[str, str]:
    """The looks that ship with bullterrier-image, by name, each with what it looks like in a sentence; load one with
    `load_recipe(name)`."""
    return {name: look.description for name, look in shipped_looks.read().items()}


@dataclass(frozen=True)
class Stage:
    """One picture from inside a render: what the photo looked like at some point on its way through the recipe."""

    label: str
    image: np.ndarray


def render(
    rgb: np.ndarray,
    recipe: Recipe | Step,
    *,
    maps: Mapping[str, np.ndarray] | None = None,
    finders: Mapping[str, Finder] | None = None,
    size: int | None = None,
    threads: int = 1,
    band: int | None = None,
) -> np.ndarray:
    """Run `recipe` on the photo `rgb` and return the result as a new image.

    The photo is first framed (`recipe.frame`: flip, turn, crop, borders) and scaled to `size`, else `recipe.size`,
    else kept as it is; its maps are framed and scaled with it. Steps run in order, each on the previous step's output,
    which the step's result is laid over: combined by its `blend_mode`, mixed in by its `opacity` and, for a masked
    step, through the selection its `mask` names (the subject, the background or a region of the recipe), so only
    that changes. An `Each` step renders each of its looks once over the whole picture and shows it through the pieces
    of its partition that have it, blending the looks across the borders. An effect with a field parameter gets the
    map it names (`field = "subject"`: the subject's silhouette; `"depth"`: the depth map). After the last step a
    recipe whose framing is `transparent` is cut out: the result then has four channels, RGB and straight alpha
    (h, w, 4), instead of three. A `Step` given as the recipe is the recipe of that one step: `render(photo, step)` is
    `step.apply(photo)`.

    rgb: the photo, a numpy array (h, w, 3): floats in 0..1, or unsigned integer levels (uint8 over 255, uint16 over
    65535); anything else is an InvalidImageError naming what was found and the fix (NaN or an infinity a
    NonFiniteImageError). It is read in place, not copied, and never modified: do not modify it from another thread
    while it renders. recipe: what to run. maps: the photo's maps by name, each float32 (h, w) with the photo's height
    and width, 0..1: `subject` (white = subject), `depth` (1 = nearest), and one for each `Given` region the recipe
    declares, under that region's name; None for none. A map given or found must be finite and within 0..1. finders: a
    `Finder` by map name, called with the photo as given (before it is framed or scaled) once, when the recipe needs
    that map and it was not handed in; the `subject` finder is also called with the crop of the working picture of each
    piece for a `Subject` region with `per` (see `Finder`). Without a `subject` finder, the subject is found by
    `finding.subject.subject_finder()` if needed; without a `depth` map or finder, the depth by
    `finding.depth.depth_finder()`. Raises RecipeError for a name in `maps` or `finders` that is no map of the recipe
    (naming the ones it has); the map's error (SubjectNotFoundError for the subject, DepthNotFoundError for the depth,
    else MapNotFoundError), naming the map, when the recipe needs it and neither the map nor a finder was given, the
    finder returns None, the map is not (h, w) or not within 0..1; a `per` region has no `subject` finder or the
    finder's map does not fit its crop or is not within 0..1; SubjectNotFoundError also when the framing crops around a
    subject that is not there.

    size: the longest side of the result in px, 1 or more, within the core's pixel limit (the recipe's own size
        when None;
    RecipeError for anything
    else). The recipe's pixel values are written for 480 and multiplied by the result's longest side over 480, so one
    recipe gives the same look at any size; a value stops at its minimum when shrinking.

    threads: how many threads the render may use for the rows of its work (laying a step over its input, framing and
    resizing the photo, an effect that works pixel by pixel), a whole number from 0 to 256 (RecipeError for anything
    else). 1, the default, starts none; 0 is one for each core. The result is the same
    bytes at any number, so ask for more when you render one picture and keep 1 when you render many at once from your
    own threads (each render using every core would only compete with the others). The GIL is released while the
    core works, so renders from several Python threads run side by side.

    band: how many rows of the working picture are made at a time, a whole number of 1 or more (RecipeError for anything
    else); None, the default, makes the whole picture at once. The result equals the whole render within float rounding
    (1e-4; most effects byte for byte), and a run of steps that each read only a bounded neighbourhood of a pixel (a
    blur, a grade, a vignette) needs memory for a band, not for several pictures. A step that may read any pixel runs on
    the whole picture. `trace` keeps every step's
    picture and takes no band.
    """
    recipe = _one_recipe(recipe)
    return bridge.render(
        rgb, recipe, maps=maps, finders=_finders([recipe], maps, finders), size=size, threads=threads, band=band
    )


def trace(
    rgb: np.ndarray,
    recipe: Recipe | Step,
    *,
    maps: Mapping[str, np.ndarray] | None = None,
    finders: Mapping[str, Finder] | None = None,
    size: int | None = None,
    threads: int = 1,
) -> list[Stage]:
    """Every stage of a render, so you can see what each step did and where something went wrong.

    The stages are the photo as the recipe receives it (framed and scaled), the subject matte if the recipe uses one
    (white = subject), the depth map if it uses one (white = nearest), each region of the recipe that its steps use, in
    the order declared (a selection in white where it chooses, which shows a `Given` map too; a partition as the dimmed
    photo with its borders and the number of each piece, which is what `Pick` and `Each` count by), the image after
    each step, in order, and, when the recipe's framing cuts the picture out, that cut (an RGBA picture, four
    channels). The last stage is exactly what `render` returns. Every stage stays in memory: one picture per step, plus
    the input and the maps.

    rgb, recipe, maps, finders, size, threads: as for `render`.
    """
    recipe = _one_recipe(recipe)
    stages = bridge.trace(rgb, recipe, maps=maps, finders=_finders([recipe], maps, finders), size=size, threads=threads)
    photo = stages[0][2]
    return [_stage(recipe, photo, *stage) for stage in stages]


def render_all(
    rgb: np.ndarray,
    recipes: Sequence[Recipe],
    *,
    maps: Mapping[str, np.ndarray] | None = None,
    finders: Mapping[str, Finder] | None = None,
    size: int | None = None,
    threads: int = 1,
    band: int | None = None,
) -> list[np.ndarray]:
    """Run several recipes on one photo; each map is found at most once, however many recipes need it.

    rgb: the photo (see `render`). recipes: the recipes to run; the results come back in the same order, one still each.
    maps: the photo's maps by name, used by every recipe that needs them (see `render`), or None. finders: each `find`
    is called once with the photo as given when any recipe needs its map and the map was not handed in (the map's error
    if it is needed and missing); the `subject` finder is also asked about the pieces of a `Subject` with `per` as for
    `render`. A name that is no map of any recipe is a RecipeError. size, threads, band: as for `render`, for every
    recipe.
    """
    recipes = _recipes(recipes)
    return bridge.render_all(
        rgb, recipes, maps=maps, finders=_finders(recipes, maps, finders), size=size, threads=threads, band=band
    )


def _stage(recipe: Recipe, photo: np.ndarray, kind: str, place: int | str | None, picture: np.ndarray) -> Stage:
    """The core's stage as a labelled picture: a map as grey, a partition as the dimmed photo with its borders and the
    number of each of its pieces."""
    match kind:
        case "subject":
            return Stage("subject matte (white = subject)", _grey(picture))
        case "depth":
            return Stage("depth (white = nearest)", _grey(picture))
        case "selection":
            return Stage(f"region {place!r} (white = chosen)", _grey(picture))
        case "partition":
            pieces = int(picture.max()) + 1
            return Stage(f"region {place!r} ({pieces} pieces, numbered)", native.number_pieces(photo, picture))
        case "step":
            return Stage(f"{place}. {recipe.steps[cast(int, place) - 1].describe()}", picture)
        case "cut_out":
            return Stage(f"{len(recipe.steps) + 1}. transparent({recipe.frame.transparent!r})", picture)
        case _:
            return Stage(kind, picture)


def _grey(layer: np.ndarray) -> np.ndarray:
    """The map `layer` (h, w) as a grey picture (h, w, 3)."""
    return np.repeat(layer[..., None], 3, axis=2)


def _one_recipe(recipe: object) -> Recipe:
    """`recipe` as the `Recipe` to run: itself, or a `Step` as the recipe of that one step. RecipeError, naming the fix,
    for anything else: the one check of what `render` and `trace` are given as the recipe."""
    if isinstance(recipe, Recipe):
        return recipe
    if isinstance(recipe, Step):
        return Recipe([recipe])
    fix = {
        np.ndarray: "the photo comes first, then the recipe: render(photo, recipe)",
        str: 'a look is loaded first: load_recipe("glitch"), or a file with load_recipe(path)',
    }.get(type(recipe), "wrap steps in a recipe: Recipe([step, ...])")
    raise RecipeError(f"the recipe is a Recipe or a Step, got {type(recipe).__name__}: {fix}")


def _recipes(recipes: object) -> list[Recipe]:
    """`recipes` as the list of `Recipe` to run; RecipeError, naming the fix, unless it is a list or tuple of them:
    the one check of what `render_all` is given."""
    if not isinstance(recipes, list | tuple) or not all(isinstance(recipe, Recipe) for recipe in recipes):
        found = type(recipes).__name__ + (
            f" of {', '.join(sorted({type(item).__name__ for item in recipes}))}"
            if isinstance(recipes, list | tuple)
            else ""
        )
        raise RecipeError(
            f"the recipes are a sequence of Recipe (a list or tuple), got {found}: one recipe is "
            "render(photo, recipe); steps are wrapped in Recipe([step, ...])"
        )
    return list(recipes)


def _finders(
    recipes: list[Recipe], maps: Mapping[str, np.ndarray] | None, finders: Mapping[str, Finder] | None
) -> dict[str, Finder]:
    """`finders` with the default finder of the subject and of the depth added when none is named and one of `recipes`
    will ask for it: the subject when a recipe needs it and no `subject` map is given, or finds it piece by piece; the
    depth when a recipe needs it and no `depth` map is given. Each default is built only then, once per call."""
    maps, bound = maps or {}, dict(finders or {})
    for name in FOUND_MAPS:
        asks = name not in maps and any(name in maps_needed(recipe) for recipe in recipes)
        asks = asks or (name == "subject" and any(needs_finder(recipe) for recipe in recipes))
        if name not in bound and asks:
            bound[name] = default_finder(name)
    return bound


def default_finder(name: str) -> Finder:
    """The default finder of the built-in map `name` (`subject` or `depth`): `finding.subject.subject_finder()`, or
    `finding.depth.depth_finder()`. Built when called, so a setting nobody uses cannot stop a run."""
    return {"subject": subject.subject_finder, "depth": depth_finder}[name]()
