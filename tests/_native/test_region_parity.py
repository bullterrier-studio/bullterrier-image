"""Regions seen from Python: a Python finder's failures reach the core's render as their own types, and the shipped
looks and examples that use regions render through the facade. The regions themselves are tested in Rust."""

from pathlib import Path
from typing import Any

import numpy as np
import pytest

from bullterrier_image.core import bridge
from bullterrier_image.core.framing import Framing
from bullterrier_image.core.params import Drive
from bullterrier_image.core.regions import (
    Edge,
    Grid,
    Shards,
    Subject,
    Svg,
)
from bullterrier_image.core.steps import Each, Recipe, Step, references
from bullterrier_image.errors import DepthNotFoundError, MapNotFoundError, SubjectNotFoundError
from bullterrier_image.loader import load_recipe

ROOT = Path(__file__).resolve().parents[2]
SHIPPED = sorted((ROOT / "src" / "bullterrier_image" / "looks").glob("*.toml"))
EXAMPLES = sorted(path for path in (ROOT / "examples").rglob("*.toml") if path.name != "bt-image.toml")
FILES = [*SHIPPED, *EXAMPLES]


def depth_of(photo: np.ndarray) -> np.ndarray:
    """Depth rising from 0 at the left to 1 at the right."""
    height, width = photo.shape[:2]
    return np.tile(np.linspace(0, 1, width, dtype=np.float32), (height, 1))


def masked_by(name: str, **regions: Any) -> Recipe:
    """A recipe that doubles the photo through the selection `name`, declaring `regions`."""
    return Recipe([Step("grade", params={"exposure": 1}, mask=name)], regions=regions)


def core_render(photo: np.ndarray, recipe: Recipe, **given: Any) -> np.ndarray:
    return bridge.render(photo, recipe, **given)


# --- subject found piece by piece -----------------------------------------------------------------------------------


class Truth:
    """A finder that knows the truth: the red of the crop where it is above 0.5. It keeps the shapes it was asked about
    and finds nothing (raises, as the package's finders do) in a crop without red above `at_least`."""

    def __init__(self, at_least: float = 0.0) -> None:
        self.at_least = at_least
        self.shapes: list[tuple[int, ...]] = []

    def find(self, rgb: np.ndarray) -> np.ndarray:
        self.shapes.append(rgb.shape[:2])
        if not (rgb[..., 0] > self.at_least).any():
            raise SubjectNotFoundError("nothing here")
        return np.where(rgb[..., 0] > 0.5, rgb[..., 0], 0).astype(np.float32)


def found_in_pieces(photo: np.ndarray) -> np.ndarray:
    """The truth over the whole picture: the red where it is above one half."""
    return np.where(photo[..., 0] > 0.5, photo[..., 0], 0)


PER_PIECE = {"r": Grid(columns=2, rows=2), "found": Subject(per="r", context=0.25, threshold=0)}


@pytest.mark.parametrize(
    ("raised", "error"),
    [(SubjectNotFoundError, None), (DepthNotFoundError, DepthNotFoundError), (MapNotFoundError, MapNotFoundError)],
    ids=["the subject", "the depth", "another map"],
)
def test_only_the_subject_not_being_found_leaves_a_piece_empty_the_others_are_the_renders_error(photo, raised, error):
    class Finds:
        def find(self, rgb: np.ndarray) -> np.ndarray:
            raise raised("not in this piece")

    recipe = masked_by("found", **PER_PIECE)
    if error is None:  # every piece adds nothing: the mask chooses nothing and the photo is as it was
        np.testing.assert_array_equal(core_render(photo, recipe, finders={"subject": Finds()}), photo)
        return
    with pytest.raises(error, match="not in this piece") as caught:
        core_render(photo, recipe, finders={"subject": Finds()})
    assert type(caught.value) is error


def test_an_exception_that_only_looks_like_the_cores_not_found_error_is_a_failure_as_any_other(photo):
    class Looks(ValueError):
        kind = "subject_not_found"

    raised = Looks("a ValueError with a kind of its own")

    class Finds:
        def find(self, rgb: np.ndarray) -> np.ndarray:
            raise raised

    with pytest.raises(Looks) as caught:
        core_render(photo, masked_by("found", **PER_PIECE), finders={"subject": Finds()})
    assert caught.value is raised


def test_another_failure_of_the_finder_is_the_renders_failure_as_it_was_raised(photo):
    class Breaks:
        def find(self, rgb: np.ndarray) -> np.ndarray:
            raise ZeroDivisionError("boom")

    with pytest.raises(ZeroDivisionError, match="boom"):
        core_render(photo, masked_by("found", **PER_PIECE), finders={"subject": Breaks()})


def test_a_subject_found_per_piece_needs_a_finder_and_a_map_of_the_whole_photo_does_not_do(photo, subject):
    with pytest.raises(SubjectNotFoundError, match="finder"):
        core_render(photo, masked_by("found", **PER_PIECE), maps={"subject": subject})


def test_a_depth_finder_that_finds_nothing_is_the_depth_not_found(photo):
    class Nothing:
        def find(self, rgb: np.ndarray) -> np.ndarray:
            raise DepthNotFoundError("no depth here")

    recipe = Recipe([Step("grade", params={"exposure": Drive(by="depth", start=0, end=1)})])
    with pytest.raises(DepthNotFoundError, match="no depth here"):
        core_render(photo, recipe, finders={"depth": Nothing()})


# --- the shipped looks and examples -------------------------------------------------------------------------------


def used_regions(recipe: Recipe) -> list[str]:
    """The regions the steps use, or that those are made from."""
    todo = [name for step in recipe.steps for _, name, _ in references(step)]
    names: set[str] = set()
    while todo:
        name = todo.pop()
        if name in names or name not in recipe.regions:
            continue
        names.add(name)
        for field in ("of", "per", "fit"):  # the fields of a region that name the regions it is made from
            made_from = getattr(recipe.regions[name], field, None)
            todo += [made_from] if isinstance(made_from, str) else list(made_from or ())
    return [name for name in recipe.regions if name in names]


def declares_regions(recipe: Recipe) -> bool:
    return bool(recipe.regions) or any(isinstance(step, Each) for step in recipe.steps)


def seeded(recipe: Recipe) -> bool:
    """Whether a region the steps use is shards or torn: both draw from the core's own stream, not numpy's."""
    return any(
        isinstance(recipe.regions[name], Shards) or getattr(recipe.regions[name], "edge", Edge()).style == "torn"
        for name in used_regions(recipe)
    )


def draws_svg(recipe: Recipe) -> bool:
    """Whether a region the steps use is read from an svg file (the core reads it with its own reader)."""
    return any(isinstance(recipe.regions[name], Svg) for name in used_regions(recipe))


RUNNABLE = [path for path in FILES if declares_regions(load_recipe(path))]
DRAWN = [path for path in RUNNABLE if draws_svg(load_recipe(path))]
MADE = [path for path in RUNNABLE if not seeded(load_recipe(path)) and path not in DRAWN]
SEEDED = [path for path in RUNNABLE if seeded(load_recipe(path))]


def ID(path: Path) -> str:
    return str(path.relative_to(ROOT))


def test_the_files_that_use_the_regions_the_core_makes_are_the_ones_the_scan_found():
    assert {path.stem for path in MADE} >= {
        "tilt-shift",
        "tonal-grade",
        "shadow-grain",
        "depth-grade",
        "depth-haze",
        "one-cell",
        "focus_blur",
        "bezier-shape",
        "diagonal",
        "bezier-cut",
    }
    assert {path.stem for path in SEEDED} >= {"tear", "glass", "torn"}
    assert {path.stem for path in DRAWN} >= {"svg"}


@pytest.mark.parametrize("path", MADE, ids=ID)
def test_a_shipped_file_that_uses_regions_renders_a_picture_that_the_look_changed(path, photo, subject):
    recipe = load_recipe(path)
    maps = {"subject": subject, "depth": depth_of(photo)}
    made = core_render(photo, recipe, maps=maps)
    assert made.dtype == np.float32
    assert np.isfinite(made).all()
    assert made.min() >= 0
    assert made.max() <= 1
    last = bridge.trace(photo, recipe, maps=maps)[-1]
    np.testing.assert_array_equal(last[2], made)  # the last stage of a trace is the render
    if recipe.frame == Framing() and recipe.size is None:
        assert np.abs(made - photo).max() > 1e-3  # the look does something


@pytest.mark.parametrize("path", DRAWN, ids=ID)
def test_a_shipped_file_that_draws_an_svg_renders_the_same_every_time_and_stays_a_picture(path, photo, subject):
    recipe = load_recipe(path)
    maps = {"subject": subject, "depth": depth_of(photo)}
    made = core_render(photo, recipe, maps=maps)
    assert made.dtype == np.float32
    assert np.isfinite(made).all()
    assert made.min() >= 0
    assert made.max() <= 1
    np.testing.assert_array_equal(bridge.trace(photo, recipe, maps=maps)[-1][2], made)
    np.testing.assert_array_equal(core_render(photo, recipe, maps=maps), made)


@pytest.mark.parametrize("path", SEEDED, ids=ID)
def test_a_shipped_file_with_shards_or_a_torn_edge_renders_the_same_every_time_and_does_something(path, photo):
    # seeded: the shards and the tear are not numpy's, so only properties are compared (the python engine's differ)
    recipe = load_recipe(path)
    given = {"maps": {"depth": depth_of(photo)}, "finders": {"subject": Truth()}}
    made = core_render(photo, recipe, **given)
    assert made.dtype == np.float32
    assert np.isfinite(made).all()
    assert made.min() >= 0
    assert made.max() <= 1
    np.testing.assert_array_equal(core_render(photo, recipe, **given), made)
    if recipe.frame == Framing() and recipe.size is None:
        assert np.abs(made - photo).max() > 1e-3
