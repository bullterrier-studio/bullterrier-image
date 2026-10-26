"""Running a `Recipe` on a photo: `render` (one still), `render_all` (several recipes on one photo) and `trace` (every
intermediate picture, as `Stage`s); `Finder`, how the runner is told to find a map in a photo.

Promises: the input photo is never modified; the same photo, recipe and maps always give the same pixels; `trace`'s
last stage is exactly what `render` returns, because both run the same code; an effect that changes the image shape is
an error; `render` keeps alive only the image the next step needs, so a render's memory peak is its busiest step's
own (that step's input, scratch and output) plus the scaled maps, whatever the number of steps, while `trace` keeps
every stage. The runner is given everything it uses: the photo, the subject either as a `matte` or as a `find_subject`
`Finder`, and the depth either as a `depth` map or as a `find_depth` `Finder`. It asks each finder at most once per
render (once per photo in `render_all`) and only if the recipe needs its map: the subject for a step on it, an effect
that takes it, or a region in use that is made from it (regions declared but unused need nothing); the depth for an
effect that takes it. A region that finds the subject piece by piece (`Subject` with `per`) asks `find_subject` about
the crop of the working picture of each piece, once per piece per render, even when a `matte` was given (a matte of the
whole photo does not answer for a part). It never loads a file or looks for a map on its own;
`bullterrier_image.recipe` binds the default subject finder for users. The maps are found on the photo as given, before
the recipe's `Framing` is applied, so a crop can follow the subject and the maps move with the photo; the framed photo
and maps are then scaled to the recipe's size. The recipe's regions are made once per render, for that scaled picture,
and only as steps ask for them.

Dependency direction: imports `steps`, `regions`, the effect registry and `ops` (to scale the photo and blend a step);
never the recipe-file reader `loader`, so recipes built in Python run without it.
"""

from collections import deque
from collections.abc import Callable, Iterator, Sequence
from dataclasses import dataclass

import numpy as np

from bullterrier_image.errors import DepthNotFoundError, RecipeError, SubjectNotFoundError
from bullterrier_image.ops.composite import composite
from bullterrier_image.ops.pieces import number_pieces
from bullterrier_image.ops.resample import resize

from . import registry
from .regions import BoxFinder, Partition, Regions, Selection, look_weights, resolve, used
from .steps import Each, Recipe, Step


@dataclass(frozen=True)
class Finder:
    """A function that finds a map in a photo, and the name of its answers: `identity` changes exactly when they could.

    `find` takes a photo, float32 (h, w, 3) in 0..1, and returns its map, float32 (h, w) in 0..1 (for the subject: its
    alpha, white = subject; for depth: 1 = nearest), or raises SubjectNotFoundError (DepthNotFoundError) when there is
    nothing to find. `identity` is non-blank text
    naming the function and whatever shapes its answers (model, version, options): two finders with one identity are
    taken to answer alike, so a cache keyed by it (`pipeline.run`) reuses their answers. RecipeError, naming the field,
    for a blank `identity` or a `find` that cannot be called.
    """

    identity: str
    find: Callable[[np.ndarray], np.ndarray]

    def __post_init__(self) -> None:
        if not isinstance(self.identity, str) or not self.identity.strip():
            raise RecipeError(f"Finder identity must be non-blank text naming its answers, got {self.identity!r}")
        if not callable(self.find):
            raise RecipeError(f"Finder find must be a function photo -> map, got {self.find!r}")


def render(
    rgb: np.ndarray,
    recipe: Recipe,
    *,
    matte: np.ndarray | None = None,
    depth: np.ndarray | None = None,
    find_subject: Finder | None = None,
    find_depth: Finder | None = None,
) -> np.ndarray:
    """Run `recipe` on the photo `rgb` and return the result as a new image.

    The photo is first framed (`recipe.frame`: flip, turn, crop, borders) and scaled to `recipe.size`; the subject
    matte and the depth map are framed and scaled with it. Steps run in order, each on the previous step's output, which
    the step's result is laid over: combined by its `blend_mode`, mixed in by its `opacity` and, for a masked step,
    through the selection its `mask` names (the subject, the background or a region of the recipe), so only that
    changes. An `Each` step renders each of its looks once over the whole picture and shows it through the pieces of its
    partition that have it, blending the looks across the borders.
    Effects that declare a `matte` parameter (see `evaporate`) get the subject's silhouette automatically, and those
    that declare `depth` the depth map. After the last step a recipe whose framing is `transparent` is cut out: the
    result then has four channels, RGB and straight alpha (h, w, 4), instead of three.

    rgb: the photo, float32 (h, w, 3) in 0..1. recipe: what to run. matte: the subject's alpha, float32 (h, w) with the
    photo's height and width, 0..1 (white = subject), or None. depth: how near each pixel is, float32 (h, w) likewise,
    0..1 (1 = nearest), or None. find_subject: its `find` is called with the photo as given (before it is framed or
    scaled) when the recipe needs the subject and no `matte` was given, and with the crop of the working picture of each
    piece for a `Subject` region with `per` (see `Finder`). find_depth: its `find` is called once with the photo as
    given when the recipe needs depth (`recipe.needs_depth`) and no `depth` was given. Raises SubjectNotFoundError when
    the recipe needs the subject and neither was given, the finder returns None, the matte is not (h, w), a `per`
    region has no finder or the finder's matte does not fit its crop, or the framing crops around a subject that is not
    there; DepthNotFoundError, naming the step, when the recipe needs depth and neither was given, the finder returns
    None, or the depth map is not (h, w).
    """
    image, inputs, regions = _prepare(rgb, recipe, matte, depth, find_subject, find_depth)
    stages = _stages(image, recipe.steps, inputs, regions)
    del image  # the stages own the scaled photo now, and let it go once the first step has run
    result = _last(stages)
    return recipe.frame.cut(result, inputs.get("matte")) if recipe.frame.has_alpha else result


@dataclass(frozen=True)
class Stage:
    """One picture from inside a render: what the photo looked like at some point on its way through the recipe."""

    label: str
    image: np.ndarray


def trace(
    rgb: np.ndarray,
    recipe: Recipe,
    *,
    matte: np.ndarray | None = None,
    depth: np.ndarray | None = None,
    find_subject: Finder | None = None,
    find_depth: Finder | None = None,
) -> list[Stage]:
    """Every stage of a render, so you can see what each step did and where something went wrong.

    The stages are the photo as the recipe receives it (framed and scaled), the subject matte if the recipe uses one
    (white = subject), the depth map if it uses one (white = nearest), each region of the recipe that its steps use, in
    the order declared (a selection in white where it chooses; a partition as the dimmed photo with its borders and the
    number of each piece, which is what `Pick` and `Each` count by), the image after each step, in order, and, when the
    recipe's framing cuts the picture out, that cut (an RGBA picture, four channels). The last stage is exactly what
    `render` returns, because both run the same code. Every stage stays in memory: one picture per step, plus the input
    and the maps.

    rgb, recipe, matte, depth, find_subject, find_depth: as for `render`.
    """
    rgb, inputs, regions = _prepare(rgb, recipe, matte, depth, find_subject, find_depth)
    stages = [Stage("input", rgb)]
    if recipe.needs_matte:
        stages.append(Stage("subject matte (white = subject)", np.repeat(inputs["matte"][..., None], 3, axis=2)))
    if recipe.needs_depth:
        stages.append(Stage("depth (white = nearest)", np.repeat(inputs["depth"][..., None], 3, axis=2)))
    wanted = used(recipe.regions, (name for step in recipe.steps for _, name, _ in step.references()))
    stages += [_region_stage(name, regions[name], rgb) for name in recipe.regions if name in wanted]
    for number, (step, image) in enumerate(_stages(rgb, recipe.steps, inputs, regions), start=1):
        stages.append(Stage(f"{number}. {step.describe()}", image))
    if recipe.frame.has_alpha:
        label = f"{len(recipe.steps) + 1}. transparent({recipe.frame.transparent!r})"
        stages.append(Stage(label, recipe.frame.cut(stages[-1].image, inputs.get("matte"))))
    return stages


def _region_stage(name: str, region: Partition | Selection, rgb: np.ndarray) -> Stage:
    """A region as a picture: a partition over the photo with its pieces numbered, a selection white where chosen."""
    if isinstance(region, Partition):
        return Stage(
            f"region {name!r} ({region.count} pieces, numbered)", number_pieces(rgb, region.labels, region.count)
        )
    return Stage(f"region {name!r} (white = chosen)", np.repeat(region.alpha[..., None], 3, axis=2))


def render_all(
    rgb: np.ndarray,
    recipes: list[Recipe],
    *,
    matte: np.ndarray | None = None,
    depth: np.ndarray | None = None,
    find_subject: Finder | None = None,
    find_depth: Finder | None = None,
) -> list[np.ndarray]:
    """Run several recipes on one photo; the subject and the depth are each found at most once, however many recipes
    need them.

    rgb: the photo (see `render`). recipes: the recipes to run; the results come back in the same order, one still each.
    matte, depth: the subject's alpha and the depth map, used by every recipe (see `render`), or None. find_subject,
    find_depth: each `find` is called once with the photo as given when any recipe needs its map and the map was not
    given (SubjectNotFoundError or DepthNotFoundError if it is needed and missing); `find_subject` is also asked about
    the pieces of a `Subject` with `per` as for `render`.
    """
    maps = _maps(rgb, recipes, matte, depth, find_subject, find_depth)
    matte, depth = maps.get("matte"), maps.get("depth")
    return [
        render(rgb, recipe, matte=matte, depth=depth, find_subject=find_subject, find_depth=find_depth)
        for recipe in recipes
    ]


# What the runner tells about each map of the photo, by input name: the error when it is missing or unusable, what it
# is, and the keyword of its finder.
_MAPS = {
    "matte": (SubjectNotFoundError, "subject's alpha", "find_subject"),
    "depth": (DepthNotFoundError, "depth map (1 = nearest)", "find_depth"),
}


def _maps(
    rgb: np.ndarray,
    recipes: Sequence[Recipe],
    matte: np.ndarray | None,
    depth: np.ndarray | None,
    find_subject: Finder | None,
    find_depth: Finder | None,
) -> dict[str, np.ndarray]:
    """The maps of the photo `rgb` that `recipes` need or that were given, by input name: each the one given, else
    found once by its finder when a recipe needs it (an error naming that recipe when there is no finder either), and
    checked to be the photo's (h, w). The one place a map is found and checked, the subject and the depth alike."""
    wanted = {
        "matte": (matte, find_subject, [recipe for recipe in recipes if recipe.needs_matte]),
        "depth": (depth, find_depth, [recipe for recipe in recipes if recipe.needs_depth]),
    }
    maps = {}
    for name, (layer, finder, needing) in wanted.items():
        if layer is None and needing:
            if finder is None:
                raise _missing(name, needing[0])
            layer = find_map(name, finder, rgb)
        if layer is None:
            continue
        if layer.shape != rgb.shape[:2]:
            raise _MAPS[name][0](f"{name} is {layer.shape} but the photo is {rgb.shape[:2]}")
        maps[name] = layer
    return maps


def _missing(name: str, recipe: Recipe) -> SubjectNotFoundError | DepthNotFoundError:
    """The error for a map `recipe` needs that neither it nor a finder was given for."""
    if name == "matte":
        return SubjectNotFoundError(
            f"recipe {recipe.name!r} uses the subject (a mask, a region made from it, or an effect that takes it), so "
            "it needs the subject: pass `matte` (the subject's alpha) "
            "or `find_subject` (a Finder, e.g. bullterrier_image.finding.subject.subject_finder())"
        )
    steps = ", ".join(f"step {number} ({effect})" for number, effect in recipe.taking("depth"))
    return DepthNotFoundError(
        f"recipe {recipe.name!r} {steps} takes the depth map, so it needs depth: pass `depth` (float32 (h, w), 0..1, "
        "1 = nearest, e.g. from bullterrier_image.imageio.load_depth) or `find_depth` (a Finder)"
    )


def find_map(name: str, finder: Finder, rgb: np.ndarray) -> np.ndarray:
    """Ask `finder` for the map `name` ("matte" or "depth") of the photo `rgb` and return it. The one place a finder of
    a whole photo is called and its answer checked (the runner and the pipeline both use it): SubjectNotFoundError
    (for the matte) or DepthNotFoundError (for depth) if it returned None, since a finder must answer or raise, and
    carrying on would drop the map without a word."""
    error, what, keyword = _MAPS[name]
    answer = finder.find(rgb)
    if answer is None:
        raise error(
            f"`{keyword}` returned None: it must return the {what} (float32, the photo's height and width, 0..1) or "
            "raise when it finds none"
        )
    return answer


def _prepare(
    rgb: np.ndarray,
    recipe: Recipe,
    matte: np.ndarray | None,
    depth: np.ndarray | None,
    find_subject: Finder | None,
    find_depth: Finder | None,
) -> tuple[np.ndarray, dict[str, np.ndarray], Regions]:
    """Find the maps the recipe needs, then frame the photo and its maps together, scale them all to the recipe's size
    and set up the recipe's regions for that picture. The one place the photo is framed. Returns the working picture,
    its maps by input name (those needed or given) and its regions."""
    maps = _maps(rgb, [recipe], matte, depth, find_subject, find_depth)
    rgb, matte, depth = recipe.frame.apply(rgb, maps.get("matte"), maps.get("depth"))
    inputs = {name: layer for name, layer in (("matte", matte), ("depth", depth)) if layer is not None}
    if recipe.size:
        rgb = resize(rgb, recipe.size)
        inputs = {name: resize(layer, recipe.size) for name, layer in inputs.items()}
    subject = None if matte is None else (lambda: inputs["matte"])  # the subject of the working picture, for a region
    return rgb, inputs, resolve(recipe.regions, rgb.shape[:2], subject=subject, find=_finder_in(rgb, find_subject))


def _finder_in(rgb: np.ndarray, find_subject: Finder | None) -> BoxFinder | None:
    """The finder for the pieces of the working picture `rgb` (see `regions.BoxFinder`): `find_subject` asked about
    the crop of the box. None when no finder was given."""
    if find_subject is None:
        return None
    return lambda box: find_subject.find(rgb[box[1] : box[3], box[0] : box[2]])


def _last(stages: Iterator[tuple[Step | Each, np.ndarray]]) -> np.ndarray:
    """The image after the final stage (there must be one). Only the newest image is kept while the stages run: each
    step's input is freed once the next step has its output, unless something else still holds it."""
    return deque(stages, maxlen=1)[0][1]


def _stages(
    out: np.ndarray, steps: Sequence[Step | Each], inputs: dict[str, np.ndarray], regions: Regions
) -> Iterator[tuple[Step | Each, np.ndarray]]:
    """Run the steps in order, yielding each step with the image after it. The one place a step is applied."""
    for step in steps:
        out = _apply(out, step, inputs, regions)
        yield step, out


def _apply(before: np.ndarray, step: Step | Each, inputs: dict[str, np.ndarray], regions: Regions) -> np.ndarray:
    """`before` with the step's result laid over it. A function of its own, like `_lay`, so that what a step needs on
    the way (the pieces' weights, a third of a picture or more) is freed before the next step runs."""
    result = _each(before, step, inputs, regions) if isinstance(step, Each) else _effect(before, step, inputs, regions)
    return _lay(before, result, step, regions)


def _effect(before: np.ndarray, step: Step, inputs: dict[str, np.ndarray], regions: Regions) -> np.ndarray:
    """The step's effect run on `before`, given the inputs it declares: the maps `_prepare` made for every step that
    takes one, and the pieces of the step's partition."""
    extra = {name: inputs[name] for name in registry.inputs(step.effect) if name != "partition"}
    if step.partition:
        extra["partition"] = regions.pieces(step.partition).labels
    result = registry.apply(step.effect, before, **step.params, **extra)
    if result.shape != before.shape:
        raise RuntimeError(f"effect {step.effect!r} changed the image shape {before.shape} -> {result.shape}")
    return result


def _each(before: np.ndarray, step: Each, inputs: dict[str, np.ndarray], regions: Regions) -> np.ndarray:
    """`before` with each piece of the step's partition replaced by its look: every look that has pieces is rendered
    once over the whole picture and shown by the weights of its pieces, which add up to 1 with the weights of the
    pieces left as they were ("none"). The pixels of a look's own pieces are changed by that look alone."""
    partition = regions.partition(step.each)
    assignment = step.assignment(partition.count)
    weights = look_weights(partition, assignment, len(step.looks), feather=step.feather * max(before.shape[:2]))
    out = before.copy()
    for look, weight in zip(step.looks, weights, strict=True):
        if isinstance(look, Step) and weight.any():  # a look no piece has is not rendered
            out += weight[..., None] * (_apply(before, look, inputs, regions) - before)
    return np.clip(out, 0.0, 1.0, out=out)


def _lay(before: np.ndarray, after: np.ndarray, step: Step | Each, regions: Regions) -> np.ndarray:
    """`after` laid over `before` by the step's blend mode, then mixed in by its opacity and mask. The one place these
    three are applied, for a step and for a look alike."""
    if not (step.mask or step.opacity < 1.0 or step.blend_mode != "normal"):
        return after
    alpha: np.floating | np.ndarray = np.float32(step.opacity)
    if step.mask:
        alpha = alpha * regions.selection(step.mask).alpha[..., None]
    return composite(before, after, step.blend_mode, alpha)
