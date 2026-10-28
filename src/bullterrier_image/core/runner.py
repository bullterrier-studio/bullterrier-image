"""Running a `Recipe` on a photo: `render` (one still), `render_all` (several recipes on one photo) and `trace` (every
intermediate picture, as `Stage`s); `Finder`, how the runner is told to find a map in a photo.

Promises: the input photo is never modified; the same photo, recipe and maps always give the same pixels; `trace`'s
last stage is exactly what `render` returns, because both run the same code; an effect that changes the image shape is
an error; `render` keeps alive only the image the next step needs, so a render's memory peak is its busiest step's
own (that step's input, scratch and output) plus the scaled maps, whatever the number of steps, while `trace` keeps
every stage. The runner is given everything it uses: the photo and its maps by name (`maps`: float32 (h, w), 0..1, one
value per pixel of the photo), each either handed in or found by a `Finder` of that name (`finders`). The names are
`subject` (the subject's alpha), `depth` (how near, 1 = nearest) and the name of every `Given` region a recipe declares.
It asks each finder at most once per render (once per photo in `render_all`) and only if the recipe needs its map
(`steps.maps_needed`): the subject for a step on it, an effect that takes it, or a region in use made from it (regions
declared but unused need nothing); the depth and a given map likewise. A region that finds the subject piece by piece
(`Subject` with `per`) asks the `subject` finder about the crop of the working picture of each piece, once per piece
per render, even when a `subject` map was given (a map of the whole photo does not answer for a part). It never loads
a file or looks for a map on its own; `bullterrier_image.recipe` binds the default finders for users. The maps are
found on the photo as given, before the recipe's `Framing` is applied, so a crop can follow the subject and the maps
move with the photo; the framed photo and maps are then scaled to the `size` a render is asked for, else the
recipe's size, else kept. Every pixel value (the fields with the unit `px`) is written for a picture whose longest side
is 480, whatever the recipe's size; a render multiplies them by the working size over 480 (`params.scaled`), so
the look is the same at any size. The recipe's regions are made once per render, for
that scaled picture, and only as steps ask for them.

Dependency direction: imports `steps`, `regions`, the effect registry and `ops` (to scale the photo and blend a step);
never the recipe-file reader `loader`, so recipes built in Python run without it.
"""

from collections import deque
from collections.abc import Callable, Iterator, Mapping, Sequence
from dataclasses import dataclass
from typing import Any

import numpy as np

from bullterrier_image.errors import DepthNotFoundError, MapNotFoundError, RecipeError, SubjectNotFoundError
from bullterrier_image.ops.composite import composite
from bullterrier_image.ops.image import as_image
from bullterrier_image.ops.levels import level_weight
from bullterrier_image.ops.pieces import number_pieces
from bullterrier_image.ops.resample import resize

from . import registry
from .framing import cut_out, frame_photo
from .params import Drive, design_scale, scaled
from .regions import (
    DERIVED,
    FOUND_MAPS,
    BoxFinder,
    Given,
    Partition,
    Regions,
    Selection,
    check_map,
    look_weights,
    map_error,
    maps_used,
    resolve,
    used,
)
from .steps import (
    Each,
    Recipe,
    Step,
    assignment,
    derived_maps,
    drives,
    fixed,
    maps_needed,
    named_fields,
    references,
)


@dataclass(frozen=True)
class Finder:
    """A function that finds a map in a photo, and the name of its answers: `identity` changes exactly when they could.

    `find` takes a photo, float32 (h, w, 3) in 0..1, and returns its map, float32 (h, w) in 0..1 (for the subject: its
    alpha, white = subject; for depth: 1 = nearest), or raises the map's error (`SubjectNotFoundError`,
    `DepthNotFoundError`, else `MapNotFoundError`) when there is nothing to find. `identity` is non-blank text naming
    the function and whatever shapes its answers (model, version, options): two finders with one identity are taken to
    answer alike, so a cache keyed by it (`pipeline.run`) reuses their answers. RecipeError, naming the field, for a
    blank `identity` or a `find` that cannot be called.
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
    maps: Mapping[str, np.ndarray] | None = None,
    finders: Mapping[str, Finder] | None = None,
    size: int | None = None,
) -> np.ndarray:
    """Run `recipe` on the photo `rgb` and return the result as a new image.

    The photo is first framed (`recipe.frame`: flip, turn, crop, borders) and scaled to `size`, else `recipe.size`,
    else kept as it is; its maps are framed and scaled with it. Steps run in order, each on the previous step's output,
    which the step's result is laid over: combined by its `blend_mode`, mixed in by its `opacity` and, for a masked
    step, through the selection its `mask` names (the subject, the background or a region of the recipe), so only
    that changes. An `Each` step renders each of its looks once over the whole picture and shows it through the pieces
    of its partition that have it, blending the looks across the borders.
    An effect with a field parameter gets the map it names (`field = "subject"`: the subject's silhouette; `"depth"`:
    the depth map). After the last step a recipe whose framing is `transparent` is cut out: the
    result then has four channels, RGB and straight alpha (h, w, 4), instead of three.

    rgb: the photo, a numpy array (h, w, 3): floats in 0..1, or unsigned integer levels (uint8 over 255, uint16 over
    65535); anything else is an InvalidImageError naming what was found and the fix (NaN or an infinity a
    NonFiniteImageError). recipe: what to run. maps: the photo's maps by name, each float32 (h, w)
    with the photo's height and width, 0..1: `subject` (white = subject), `depth` (1 = nearest), and one for each
    `Given` region the recipe declares, under that region's name; None for none. A map given or found must be finite
    and within 0..1. finders: a `Finder` by map name, called with the photo as given (before it is framed or scaled)
    once, when the recipe needs that map (`steps.maps_needed`) and it was not handed in; the `subject` finder is also
    called with the crop of the working picture of each piece for a `Subject` region with `per` (see `Finder`).
    Raises RecipeError for a name in `maps` or `finders` that is no map of the recipe (naming the ones it has);
    the map's error (SubjectNotFoundError for the subject, DepthNotFoundError for the depth, else MapNotFoundError),
    naming the map, when the recipe needs it and neither the map nor a finder was given, the finder returns None, the
    map is not (h, w) or not within 0..1; a `per` region has no `subject` finder or the finder's map does not fit its
    crop or is not within 0..1; SubjectNotFoundError also when the framing crops around a subject that is not there.

    size: the longest side of the result in px, 1 or more (the recipe's own size when None; RecipeError for anything
    else). The recipe's pixel values are written for 480 and multiplied by the result's longest side over 480, so one
    recipe gives the same look at any size; a value stops at its minimum when shrinking and has no ceiling when
    growing.
    """
    rgb = as_image(rgb)
    _check_size(size)
    found = _maps(rgb, [recipe], maps, finders)
    return _render(rgb, recipe, found, finders, size)


def _check_size(size: int | None) -> None:
    """RecipeError unless `size` is None or a whole number of pixels, 1 or more (a bool is not a number here)."""
    if size is not None and (isinstance(size, bool) or not isinstance(size, int) or size < 1):
        raise RecipeError(f"size must be a whole number of pixels, 1 or more, or None, got {size!r}")


def _render(
    rgb: np.ndarray,
    recipe: Recipe,
    found: Mapping[str, np.ndarray],
    finders: Mapping[str, Finder] | None,
    size: int | None,
) -> np.ndarray:
    """`render` once the maps are found (`_maps`)."""
    image, maps, regions, scale = _prepare(rgb, recipe, found, finders, size)
    stages = _stages(image, recipe.steps, regions, scale)
    del image  # the stages own the scaled photo now, and let it go once the first step has run
    result = _last(stages)
    return cut_out(recipe.frame, result, maps.get("subject")) if recipe.frame.has_alpha else result


@dataclass(frozen=True)
class Stage:
    """One picture from inside a render: what the photo looked like at some point on its way through the recipe."""

    label: str
    image: np.ndarray


def trace(
    rgb: np.ndarray,
    recipe: Recipe,
    *,
    maps: Mapping[str, np.ndarray] | None = None,
    finders: Mapping[str, Finder] | None = None,
    size: int | None = None,
) -> list[Stage]:
    """Every stage of a render, so you can see what each step did and where something went wrong.

    The stages are the photo as the recipe receives it (framed and scaled), the subject matte if the recipe uses one
    (white = subject), the depth map if it uses one (white = nearest), each region of the recipe that its steps use, in
    the order declared (a selection in white where it chooses, which shows a `Given` map too; a partition as the dimmed
    photo with its borders and the number of each piece, which is what `Pick` and `Each` count by), the image after
    each step, in order, and, when the recipe's framing cuts the picture out, that cut (an RGBA picture, four
    channels). The last stage is exactly what `render` returns, because both run the same code. Every stage stays in
    memory: one picture per step, plus the input and the maps.

    rgb, recipe, maps, finders, size: as for `render`.
    """
    rgb = as_image(rgb)
    _check_size(size)
    found = _maps(rgb, [recipe], maps, finders)
    rgb, scaled, regions, scale = _prepare(rgb, recipe, found, finders, size)
    stages = [Stage("input", rgb)]
    if "subject" in scaled:
        stages.append(Stage("subject matte (white = subject)", np.repeat(scaled["subject"][..., None], 3, axis=2)))
    if "depth" in scaled:
        stages.append(Stage("depth (white = nearest)", np.repeat(scaled["depth"][..., None], 3, axis=2)))
    wanted = used(recipe.regions, (name for step in recipe.steps for _, name, _ in references(step)))
    stages += [_region_stage(name, regions[name], rgb) for name in recipe.regions if name in wanted]
    for number, (step, image) in enumerate(_stages(rgb, recipe.steps, regions, scale), start=1):
        stages.append(Stage(f"{number}. {step.describe()}", image))
    if recipe.frame.has_alpha:
        label = f"{len(recipe.steps) + 1}. transparent({recipe.frame.transparent!r})"
        stages.append(Stage(label, cut_out(recipe.frame, stages[-1].image, scaled.get("subject"))))
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
    maps: Mapping[str, np.ndarray] | None = None,
    finders: Mapping[str, Finder] | None = None,
    size: int | None = None,
) -> list[np.ndarray]:
    """Run several recipes on one photo; each map is found at most once, however many recipes need it.

    rgb: the photo (see `render`). recipes: the recipes to run; the results come back in the same order, one still each.
    maps: the photo's maps by name, used by every recipe that needs them (see `render`), or None. finders: each `find`
    is called once with the photo as given when any recipe needs its map and the map was not handed in (the map's error
    if it is needed and missing); the `subject` finder is also asked about the pieces of a `Subject` with `per` as for
    `render`. A name that is no map of any recipe is a RecipeError. size: as for `render`, for every recipe.
    """
    rgb = as_image(rgb)
    _check_size(size)
    found = _maps(rgb, recipes, maps, finders)
    return [_render(rgb, recipe, found, finders, size) for recipe in recipes]


# What each map is, by name, for the messages about it; any other map is "the <name> map".
_WHAT = {"subject": "subject's alpha", "depth": "depth map (1 = nearest)"}


def _maps(
    rgb: np.ndarray,
    recipes: Sequence[Recipe],
    maps: Mapping[str, np.ndarray] | None,
    finders: Mapping[str, Finder] | None,
) -> dict[str, np.ndarray]:
    """The maps of the photo `rgb` that `recipes` need, by name: each the one handed in, else found once by its finder
    (an error naming that recipe when there is no finder either), and checked to be the photo's (h, w), finite and
    within 0..1. A name that is no map of any recipe (the built-in `subject` and `depth`, or a `Given` region of one) is
    a RecipeError listing the names there are."""
    maps, finders = dict(maps or {}), dict(finders or {})
    known = known_maps(recipes)
    for what, names in (("maps", maps), ("finders", finders)):
        if unknown := sorted(set(names) - known):
            raise RecipeError(
                f"`{what}` names {unknown[0]!r}, which is no map of the recipe; its maps: {', '.join(sorted(known))} "
                '(`subject` and `depth` are built in; any other is a region with `kind = "given"`)'
            )
    found = {}
    for name in sorted({name for recipe in recipes for name in maps_needed(recipe)}):
        if name in maps:
            found[name] = _checked(name, maps[name], rgb)
        elif name in finders:
            found[name] = find_map(name, finders[name], rgb)
        else:
            raise _missing(name, next(recipe for recipe in recipes if name in maps_needed(recipe)))
    for name in maps.keys() - found.keys():  # handed in but unused: still held to the rule, so a wrong map shows up
        _checked(name, maps[name], rgb)
    return found


def known_maps(recipes: Sequence[Recipe]) -> set[str]:
    """The names of the maps `recipes` can be given: the built-in `subject` and `depth`, and every `Given` region of
    any of them."""
    return {
        *FOUND_MAPS,
        *(name for recipe in recipes for name, kind in recipe.regions.items() if isinstance(kind, Given)),
    }


def _checked(name: str, layer: np.ndarray, rgb: np.ndarray) -> np.ndarray:
    """`layer`, the map `name` of the photo `rgb`, once it is known to be the photo's (h, w), finite and within 0..1:
    the one check of a map of the whole photo, for a map given and a finder's answer alike, where it enters."""
    error = map_error(name)
    if layer.shape != rgb.shape[:2]:
        raise error(f"{name} is {layer.shape} but the photo is {rgb.shape[:2]}")
    return check_map(name, layer, error)


def _missing(name: str, recipe: Recipe) -> MapNotFoundError:
    """The error for the map `name` that `recipe` needs and neither it nor a finder was given for."""
    who = "the recipe" if recipe.name is None else f"recipe {recipe.name!r}"
    if name == "subject":
        return SubjectNotFoundError(
            f"{who} uses the subject (a mask, a field or a region made from it), so "
            "it needs the subject: pass `maps={'subject': alpha}` (the subject's alpha) "
            "or `finders={'subject': Finder}` (e.g. bullterrier_image.finding.subject.subject_finder())"
        )
    if name == "depth":
        regions = [
            f"step {number} (its `{key}` {region!r}, a region made from depth)"
            for number, step in enumerate(recipe.steps, start=1)
            for key, region, _ in references(step)
            if "depth" in maps_used(recipe.regions, [region])
        ]
        return DepthNotFoundError(
            f"{who} needs depth for {', '.join(regions)}: pass `maps={{'depth': depth}}` "
            "(float32 (h, w), 0..1, 1 = nearest, e.g. from bullterrier_image.imageio.load_map) or "
            "`finders={'depth': Finder}`"
        )
    return MapNotFoundError(
        f'{who} uses the map {name!r} (a region with `kind = "given"`), so it needs it: pass '
        f"`maps={{{name!r}: values}}` (float32 (h, w), 0..1, the photo's size) or `finders={{{name!r}: Finder}}`"
    )


def find_map(name: str, finder: Finder, rgb: np.ndarray) -> np.ndarray:
    """Ask `finder` for the map `name` ("subject", "depth" or a given map's name) of the photo `rgb` and return it. The
    one place a finder of a whole photo is called and its answer checked (the runner and the pipeline both use it): the
    map's error (`map_error`) if it returned None, since a finder must answer or raise, and carrying on would drop the
    map without a word, or a map that is not the photo's (h, w), naming both sizes."""
    answer = finder.find(rgb)
    if answer is None:
        raise map_error(name)(
            f"the finder of {name!r} returned None: it must return the {_WHAT.get(name, f'{name} map')} (float32, the "
            "photo's height and width, 0..1) or raise when it finds none"
        )
    return _checked(name, answer, rgb)


def _prepare(
    rgb: np.ndarray,
    recipe: Recipe,
    found: Mapping[str, np.ndarray],
    finders: Mapping[str, Finder] | None,
    size: int | None,
) -> tuple[np.ndarray, dict[str, np.ndarray], Regions, float]:
    """Frame the photo and the maps the recipe needs together, scale them all to `size` (else the recipe's size) and
    set up the recipe's regions for that picture. The one place the photo is framed. Returns the working picture, its
    maps by name (those the recipe needs), its regions and the factor for the recipe's pixel values: the working
    size over the design size (`design_scale`)."""
    rgb, maps = frame_photo(recipe.frame, rgb, {name: found[name] for name in sorted(maps_needed(recipe))})
    working = size or recipe.size
    if working:
        rgb = resize(rgb, working)
        maps = {name: resize(layer, working) for name, layer in maps.items()}
    maps |= {name: DERIVED[name](rgb) for name in sorted(derived_maps(recipe))}  # made from the picture as it is worked
    find = _finder_in(rgb, (finders or {}).get("subject"))
    scale = design_scale(rgb.shape[:2])
    return rgb, maps, resolve(recipe.regions, rgb.shape[:2], maps=maps, find=find), scale


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
    out: np.ndarray, steps: Sequence[Step | Each], regions: Regions, scale: float
) -> Iterator[tuple[Step | Each, np.ndarray]]:
    """Run the steps in order, yielding each step with the image after it. The one place a step is applied. `scale`
    multiplies the pixel values of every effect (see `_prepare`)."""
    for step in steps:
        out = _apply(out, step, regions, scale)
        yield step, out


def _apply(before: np.ndarray, step: Step | Each, regions: Regions, scale: float) -> np.ndarray:
    """`before` with the step's result laid over it. A function of its own, like `_lay`, so that what a step needs on
    the way (the pieces' weights, a third of a picture or more) is freed before the next step runs."""
    result = _each(before, step, regions, scale) if isinstance(step, Each) else _effect(before, step, regions, scale)
    return _lay(before, result, step, regions)


def _effect(before: np.ndarray, step: Step, regions: Regions, scale: float) -> np.ndarray:
    """The step's effect run on `before`, given the fields it names and the pieces of the step's partition. A step
    with driven parameters is run once for each level of its drives, and each pixel takes the two renders either side
    of its field value (see `_driven`)."""
    extra = {parameter: regions.selection(field).alpha for parameter, field in named_fields(step).items()}
    if step.partition:
        extra["partition"] = regions.pieces(step.partition).labels
    driven = drives(step)
    if not driven:
        return _run(step.effect, before, step.params, extra, scale)
    return _driven(before, step, driven, regions.selection(next(iter(driven.values())).by).alpha, extra, scale)


def _run(
    effect: str, before: np.ndarray, params: Mapping[str, Any], extra: Mapping[str, np.ndarray], scale: float
) -> np.ndarray:
    """The effect `effect` run on `before` with `params` (their pixel values multiplied by `scale`) and its injected
    inputs `extra` (and `size_ratio`, `scale`, if it declares it); RuntimeError if it changed the image's shape."""
    function = registry.get(effect)
    given: dict[str, np.ndarray | float] = {**extra}
    if "size_ratio" in function.inputs:
        given["size_ratio"] = scale
    result = function(before, scaled(function.validate(params), scale), **given)
    if result.shape != before.shape:
        raise RuntimeError(f"effect {effect!r} changed the image shape {before.shape} -> {result.shape}")
    return result


def _driven(
    before: np.ndarray,
    step: Step,
    drives: Mapping[str, Drive],
    field: np.ndarray,
    extra: Mapping[str, np.ndarray],
    scale: float,
) -> np.ndarray:
    """`step`'s effect with the parameters of `drives` varying by `field` (float32 `(height, width)`, 0..1): the effect
    is rendered at each level of the drives (all of a step have the same number, validated with the step), and every
    pixel mixes the renders of the two levels either side of `field` x (levels - 1), linearly (`ops.levels`). A level
    no pixel is on or next to is never rendered, so the cost is at most the number of levels."""
    levels = next(iter(drives.values())).levels
    position = field * (levels - 1)
    out = np.zeros_like(before)
    for level in range(levels):
        weight = level_weight(position, level)
        if weight.max() > 0:
            values = {**fixed(step), **{name: drive.value(level) for name, drive in drives.items()}}
            out += weight[..., None] * _run(step.effect, before, values, extra, scale)
    return out  # within 0..1 as the renders are: the weights of a pixel add up to exactly 1 in float32


def _each(before: np.ndarray, step: Each, regions: Regions, scale: float) -> np.ndarray:
    """`before` with each piece of the step's partition replaced by its look: every look that has pieces is rendered
    once over the whole picture and shown by the weights of its pieces, which add up to 1 with the weights of the
    pieces left as they were ("none"). The pixels of a look's own pieces are changed by that look alone."""
    partition = regions.partition(step.each)
    weights = look_weights(partition, assignment(step, partition.count), len(step.looks), feather=step.feather * scale)
    out = before.copy()
    for look, weight in zip(step.looks, weights, strict=True):
        if isinstance(look, Step) and weight.any():  # a look no piece has is not rendered
            out += weight[..., None] * (_apply(before, look, regions, scale) - before)
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
