"""A recipe run by the Rust core: the Python `Recipe` as the dict the core reads (`steps.recipe_dict`), and `render`,
`render_all` and `trace` over it.

The core renders one recipe on one photo. What is added here is what the package promises for the whole call: the
photo is made a picture once (`as_image`), `size` is a whole number of pixels, a name in `maps` or `finders` that is no
map of any recipe is refused, and each map the recipes need is handed in or found by its finder once, however many
recipes use it, before the first renders. A finder is called only here with the whole photo; the core calls it, with
the crop of the working picture of a piece, only for a region that finds the subject piece by piece.

The stages `trace` returns are the core's, as tuples; naming them is `recipe.trace`'s.
"""

from collections.abc import Mapping, Sequence
from typing import Any

import numpy as np

from bullterrier_image.errors import (
    DepthNotFoundError,
    MapNotFoundError,
    RecipeError,
    SubjectNotFoundError,
)
from bullterrier_image.ops.image import as_image

from . import native, registry
from .finder import answered, check_map, find_map
from .steps import MAX_SIZE, MAX_THREADS, Recipe, recipe_dict

Stage = tuple[str, int | str | None, np.ndarray]  # (kind, step number or region name, picture or map); see `trace`


def render(
    rgb: np.ndarray,
    recipe: Recipe,
    *,
    maps: Mapping[str, np.ndarray] | None = None,
    finders: Mapping[str, Any] | None = None,
    size: int | None = None,
    threads: int = 1,
) -> np.ndarray:
    """`recipe` run on the photo `rgb` by the core: the picture it makes, (h, w, 3), or (h, w, 4) (RGB and straight
    alpha) when its frame cuts the picture out.

    rgb: the photo, a numpy array (h, w, 3): floats in 0..1, or unsigned integer levels (uint8 over 255, uint16 over
    65535); anything else is an InvalidImageError naming what was found and the fix (NaN or an infinity a
    NonFiniteImageError). It is read in place and never modified. maps: the photo's maps by name, each float32 (h, w)
    with the photo's height and width, 0..1: `subject` (white = subject), `depth` (1 = nearest), and one for each
    `Given` region the recipe declares, under that region's name; None for none. finders: a `Finder` by map name,
    (anything with a `find` method) called with the photo as given, once, when the recipe needs that map and it was
    not handed in; the `subject`
    finder is also called with the crop of the working picture of each piece for a `Subject` region with `per`. size:
    the longest side of the result in px, 1 to 16384 (the recipe's own size when None). threads: how many threads the
    core may use for the rows of its work, a whole number from 0 to 256: 1 starts none, 0 is one for each core (the
    result is the same at any number).

    Raises RecipeError for a recipe the core refuses, a `size` or `threads` that is no whole number, or a name in `maps`
    or `finders` that is no map of the recipe (naming the ones it has); the map's error (SubjectNotFoundError for the
    subject, DepthNotFoundError for the depth, else MapNotFoundError) when the recipe needs a map and neither the map
    nor a finder was given, the finder returns None, or the map is not (h, w) or not within 0..1.
    """
    return render_all(rgb, [recipe], maps=maps, finders=finders, size=size, threads=threads)[0]


def render_all(
    rgb: np.ndarray,
    recipes: Sequence[Recipe],
    *,
    maps: Mapping[str, np.ndarray] | None = None,
    finders: Mapping[str, Any] | None = None,
    size: int | None = None,
    threads: int = 1,
) -> list[np.ndarray]:
    """Each recipe of `recipes` run on the photo `rgb`, as `render` does; each map is found at most once, however many
    recipes need it. A name in `maps` or `finders` that is no map of any recipe is a RecipeError."""
    return _each(native.render, rgb, recipes, maps, finders, size, threads)


def trace(
    rgb: np.ndarray,
    recipe: Recipe,
    *,
    maps: Mapping[str, np.ndarray] | None = None,
    finders: Mapping[str, Any] | None = None,
    size: int | None = None,
    threads: int = 1,
) -> list[Stage]:
    """Every stage of `render`, as the core reports them: `("input", None, photo)`, `("subject", None, map)` and
    `("depth", None, map)` for the maps the recipe uses (float32 (h, w)), `("selection", name, map)` (float32 (h, w))
    and `("partition", name, labels)` (int32 (h, w), the pieces numbered from 0) for each region the steps use, in the
    order declared, then `("step", n, picture)` after step `n` (from 1), all at the working size, and `("cut_out", None,
    picture)` (h, w, 4) when the frame cuts the picture out. The last is exactly what `render` returns."""
    return _each(native.trace, rgb, [recipe], maps, finders, size, threads)[0]


def maps_needed(recipe: Recipe) -> frozenset[str]:
    """The names of the maps of the photo that rendering `recipe` must be given or find: `subject` when the frame crops
    around it or keeps only it, or a region a step uses (a mask, a field, a drive's `by`) is made from it; `depth`
    likewise; and the name of every `Given` region a step uses. A region that is declared and not used needs nothing,
    and `light` is made from the picture. A `Subject` region with `per` needs a finder, not a map (`needs_finder`)."""
    return frozenset(_needs(recipe)["maps"])


def needs_finder(recipe: Recipe) -> bool:
    """True if a region a step of `recipe` uses finds the subject piece by piece: rendering needs a `subject` finder
    whatever else it is given, because it is asked about parts of the working picture, which no map of the whole photo
    answers."""
    return bool(_needs(recipe)["finds_pieces"])


def known_maps(recipe: Recipe) -> list[str]:
    """The names a photo can be handed a map or a finder under for `recipe`: `subject`, `depth`, then its `Given`
    regions in the order declared."""
    with registry.holding() as hold:
        return native.known_maps(recipe_dict(recipe, hold))


def _needs(recipe: Recipe) -> dict[str, Any]:
    with registry.holding() as hold:
        return native.needs(recipe_dict(recipe, hold))


def _check_size(size: object) -> None:
    """RecipeError unless `size` is None or a whole number of pixels, 1 to `MAX_SIZE` (a bool is not a number here)."""
    if size is not None and (isinstance(size, bool) or not isinstance(size, int) or not 1 <= size <= MAX_SIZE):
        raise RecipeError(f"size must be a whole number of pixels, 1 to {MAX_SIZE}, or None, got {size!r}")


def _check_threads(threads: object) -> None:
    """RecipeError unless `threads` is a whole number from 0 to `MAX_THREADS` (a bool is not a number here)."""
    if isinstance(threads, bool) or not isinstance(threads, int) or not 0 <= threads <= MAX_THREADS:
        raise RecipeError(
            f"threads must be a whole number of threads, 0 (every core) to {MAX_THREADS}, got {threads!r}"
        )


def _each(
    run: Any,
    rgb: np.ndarray,
    recipes: Sequence[Recipe],
    maps: Mapping[str, np.ndarray] | None,
    finders: Mapping[str, Any] | None,
    size: int | None,
    threads: int,
) -> list[Any]:
    """`run` (the core's `render` or `trace`) on `rgb` for each recipe, once the maps they all need are in hand: each
    the one handed in, else found once by its finder, and held to the rules of a map (`check_map`). Each recipe is given
    the maps it knows, and the subject finder only if it finds pieces."""
    rgb = as_image(rgb)
    _check_size(size)
    _check_threads(threads)
    maps, finders = dict(maps or {}), dict(finders or {})
    with registry.holding() as hold:
        documents = [recipe_dict(recipe, hold) for recipe in recipes]
        wants = [native.needs(document) for document in documents]
        knowns = [native.known_maps(document) for document in documents]
        _refuse_unknown(maps, finders, {name for known in knowns for name in known})
        found = {name: check_map(name, layer, rgb) for name, layer in maps.items()}
        for name in sorted({name for want in wants for name in want["maps"]} - maps.keys()):
            if name not in finders:
                raise _missing(
                    name, next(recipe for recipe, want in zip(recipes, wants, strict=True) if name in want["maps"])
                )
            found[name] = find_map(name, finders[name], rgb)
        return [
            run(
                rgb,
                document,
                maps={name: found[name] for name in known if name in maps or name in want["maps"]},
                finders={"subject": _Finds(finders["subject"])}
                if want["finds_pieces"] and "subject" in finders
                else None,
                size=size,
                threads=threads,
            )
            for document, want, known in zip(documents, wants, knowns, strict=True)
        ]


def _refuse_unknown(maps: Mapping[str, Any], finders: Mapping[str, Any], known: set[str]) -> None:
    """RecipeError for a name in `maps` or `finders` that is no map of any recipe, listing the names there are."""
    for what, names in (("maps", maps), ("finders", finders)):
        if unknown := sorted(set(names) - known):
            raise RecipeError(
                f"`{what}` names {unknown[0]!r}, which is no map of the recipe; its maps: {', '.join(sorted(known))} "
                '(`subject` and `depth` are built in; any other is a region with `kind = "given"`)'
            )


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
        return DepthNotFoundError(
            f"{who} needs depth (an effect or a region made from it): pass `maps={{'depth': depth}}` "
            "(float32 (h, w), 0..1, 1 = nearest, e.g. from bullterrier_image.imageio.load_map) or "
            "`finders={'depth': Finder}`"
        )
    return MapNotFoundError(
        f'{who} uses the map {name!r} (a region with `kind = "given"`), so it needs it: pass '
        f"`maps={{{name!r}: values}}` (float32 (h, w), 0..1, the photo's size) or `finders={{{name!r}: Finder}}`"
    )


class _Finds:
    """A finder as the core asks it, about the crop of a piece: one that finds nothing, by raising a `MapNotFoundError`,
    raises the `CoreError` that says so (the core must tell "found nothing" from a failure); an answer that is no map
    of the crop (none at all, the wrong size, values outside 0..1) is a failure, raised as it is."""

    def __init__(self, finder: Any) -> None:
        self._finder = finder

    def find(self, rgb: np.ndarray) -> np.ndarray:
        try:
            answer = self._finder.find(rgb)
        except MapNotFoundError as error:
            raise native.found_nothing(error) from error
        return answered("subject", answer, rgb)
