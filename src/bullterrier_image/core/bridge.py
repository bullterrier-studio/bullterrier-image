"""A recipe run by the Rust core: the Python `Recipe` as the dict the core reads, and `render` and `trace` over it.

Transitional (R4a to R4e): the Python runner (`runner`) still renders for users; this is the path R4e switches them to,
and what the parity tests (`tests/_native/test_runner_parity.py`, `test_region_parity.py`) hold the core's runner to.
The core does not make the regions `shards`, `cuts`, `path` or `svg` yet: a recipe whose steps use one is
a `RecipeError` from the core.
"""

import re
from collections.abc import Callable, Mapping
from typing import Any

import numpy as np

from bullterrier_image.errors import MapNotFoundError
from bullterrier_image.ops.image import as_image

from . import native, registry
from .framing import Framing
from .params import Drive, Lut, Picture
from .regions import Kind
from .steps import Each, Recipe, Step

Stage = tuple[str, int | str | None, np.ndarray]  # (kind, step number or region name, picture or map); see `trace`
Hold = Callable[[object], dict[str, int]]  # `registry.holding`'s: keeps an object, returns the reference to it


def render(
    rgb: np.ndarray,
    recipe: Recipe,
    *,
    maps: Mapping[str, np.ndarray] | None = None,
    finders: Mapping[str, Any] | None = None,
    size: int | None = None,
) -> np.ndarray:
    """`recipe` run on the photo `rgb` by the core; the arguments and the result are those of `runner.render` (a
    `finders` value needs only a `find` method). The core resamples the photo and its maps to `size`, else the recipe's,
    and scales the recipe's pixel values to the picture it works on. A recipe whose frame is `transparent` gives
    (h, w, 4): RGB and straight alpha."""
    rgb = as_image(rgb)
    with registry.holding() as hold:
        return native.render(rgb, recipe_dict(recipe, hold), maps=maps, finders=_asked(finders), size=size)


def trace(
    rgb: np.ndarray,
    recipe: Recipe,
    *,
    maps: Mapping[str, np.ndarray] | None = None,
    finders: Mapping[str, Any] | None = None,
    size: int | None = None,
) -> list[Stage]:
    """Every stage of `render`, as the core reports them: `("input", None, photo)`, `("subject", None, map)` and
    `("depth", None, map)` for the maps the recipe uses (float32 (h, w)), `("selection", name, map)` (float32 (h, w))
    and `("partition", name, labels)` (int32 (h, w), the pieces numbered from 0) for each region the steps use, in the
    order declared, then `("step", n, picture)` after step `n` (from 1), all at the working size, and `("cut_out", None,
    picture)` (h, w, 4) when the frame cuts the picture out. The last is exactly what `render` returns. Labels, and
    drawing the numbered pieces, are for the caller to make."""
    rgb = as_image(rgb)
    with registry.holding() as hold:
        return native.trace(rgb, recipe_dict(recipe, hold), maps=maps, finders=_asked(finders), size=size)


def needs(recipe: Recipe) -> dict[str, Any]:
    """What rendering `recipe` asks of the photo, as the core says: `{"maps": [names, sorted], "finds_pieces": bool}`
    (`maps`: the maps to hand in or find; `finds_pieces`: a `subject` finder is asked about pieces)."""
    with registry.holding() as hold:
        return native.needs(recipe_dict(recipe, hold))


def known_maps(recipe: Recipe) -> list[str]:
    """The names a photo can be handed a map or a finder under for `recipe`: `subject`, `depth`, its `Given` regions."""
    with registry.holding() as hold:
        return native.known_maps(recipe_dict(recipe, hold))


class _Finds:
    """A finder as the core asks it: one that finds nothing, by raising a `MapNotFoundError`, raises the `CoreError`
    that says so (the core must tell "found nothing" from a failure)."""

    def __init__(self, finder: Any) -> None:
        self._finder = finder

    def find(self, rgb: np.ndarray) -> np.ndarray:
        try:
            return self._finder.find(rgb)
        except MapNotFoundError as error:
            raise native.found_nothing(error) from error


def _asked(finders: Mapping[str, Any] | None) -> dict[str, _Finds] | None:
    return None if finders is None else {name: _Finds(finder) for name, finder in finders.items()}


def recipe_dict(recipe: Recipe, hold: Hold) -> dict[str, Any]:
    """`recipe` as the dict the core reads (the form of `_core.read_recipe`): a key is there when it is set. A step's
    preset is already in its parameters. A parameter that cannot be written as data, a `Picture` or a `Lut`, is
    `hold`'s (`registry.holding`): the dict has the reference `hold` returns."""
    document: dict[str, Any] = {"steps": [_entry(step, hold) for step in recipe.steps]}
    for key in ("name", "description", "size"):
        if (value := getattr(recipe, key)) is not None:
            document[key] = value
    if recipe.frame != Framing():
        document["frame"] = recipe.frame.model_dump()
    if recipe.regions:
        document["regions"] = {name: _region(region) for name, region in recipe.regions.items()}
    return document


def _entry(step: Step | Each | None, hold: Hold) -> dict[str, Any] | None:
    if step is None:  # a piece of an `each` step left as it was
        return None
    if isinstance(step, Each):
        entry: dict[str, Any] = {
            "each": step.each,
            "looks": [_entry(look, hold) for look in step.looks],
            "assign": step.assign,
            "seed": step.seed,
            "feather": step.feather,
        }
    else:
        entry = {"effect": step.effect}
        if step.params:
            entry["params"] = {name: _value(value, hold) for name, value in step.params.items()}
        if step.partition is not None:
            entry["partition"] = step.partition
    entry |= {"opacity": step.opacity, "blend_mode": step.blend_mode}
    for key in ("mask", "name"):
        if (value := getattr(step, key)) is not None:
            entry[key] = value
    return entry


def _region(region: Kind) -> dict[str, Any]:
    """A region as the core reads it: its fields and its `kind`, the class's name in snake case (`LinearGradient` is
    `linear_gradient`)."""
    kind = re.sub(r"(?<!^)(?=[A-Z])", "_", type(region).__name__).lower()
    return {"kind": kind, **region.model_dump()}


def _value(value: object, hold: Hold) -> object:
    """A parameter value as the plain data it is written as: a drive as its table, an array as a list, a `Picture` or
    a `Lut` as the reference `hold` gives."""
    if isinstance(value, Picture | Lut):
        return hold(value)
    if isinstance(value, Drive):
        return value.model_dump()
    if isinstance(value, Mapping):
        return {key: _value(item, hold) for key, item in value.items()}
    if isinstance(value, list | tuple):
        return [_value(item, hold) for item in value]
    if isinstance(value, np.ndarray):
        return value.tolist()
    if isinstance(value, np.generic):
        return value.item()
    return value
