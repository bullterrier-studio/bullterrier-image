"""The Python builders of regions: the models that declare them, their documentation and the forms they accept.

Where a region's pieces fall, what a reference may name and the ranges of each kind are held by the Rust tests of the
core (`region_check`, `recipe_references`, `run_regions`, `pieces`, `shards`, `svg`, `torn`), and the renders by
`tests/_native/test_region_parity.py`.
"""

from dataclasses import fields

import numpy as np
import pytest

from bullterrier_image import RecipeError
from bullterrier_image.core.steps import Recipe, Step
from bullterrier_image.recipe import (
    Band,
    Bands,
    Combine,
    Curve,
    Cuts,
    Edge,
    Grid,
    Line,
    LinearGradient,
    Path,
    Pick,
    RadialGradient,
    Shards,
    Subject,
    Svg,
    render,
)

WAVE = "M 0.38 0 C 0.15 0.45, 0.85 0.55, 0.62 1"

BAD_VALUES = {
    "no columns": (Grid, {"columns": 0}, "columns"),
    "a share that is not positive": (Grid, {"rows": [1, 0]}, "rows"),
    "too many shards": (Shards, {"count": 501}, "count"),
    "an impact outside the frame": (Shards, {"impact": (1.5, 0.5)}, "impact"),
    "a preset that does not exist": (Shards, {"preset": "nope"}, "nope.*glass"),
    "an edge style that is not offered": (Shards, {"edge": {"style": "wavy"}}, "style"),
    "no pieces to pick": (Pick, {"pieces": []}, "pieces"),
    "context past 1": (Subject, {"context": 1.5}, "context"),
    "low above high": (Band, {"low": 0.6, "high": 0.3}, r"low.*0\.6.*high.*0\.3"),
    "a band count named by its kind as a recipe writes it": (Bands, {"count": 0}, "^region .bands.: .*count"),
    "a combination named by its kind as a recipe writes it": (Combine, {"op": "xor"}, "^region .combine.: .*op"),
    "a gradient named by its kind as a recipe writes it": (
        LinearGradient,
        {"angle": "up"},
        "^region .linear_gradient.: .*angle",
    ),
    "a radial gradient named by its kind as a recipe writes it": (
        RadialGradient,
        {"radius": 0},
        "^region .radial_gradient.: .*radius",
    ),
}


@pytest.mark.parametrize(("model", "values", "complaint"), BAD_VALUES.values(), ids=BAD_VALUES.keys())
def test_a_bad_value_is_refused_naming_the_field(model, values, complaint):
    with pytest.raises(RecipeError, match=complaint):
        model(**values)


def test_a_misspelt_field_is_refused_naming_it():
    with pytest.raises(TypeError, match="column"):
        Grid(column=3)  # type: ignore[call-arg]


@pytest.mark.parametrize(
    "lines",
    [[{"x": 0.5, "path": "M 0 0 L 1 1"}], []],
    ids=["a line and a curve at once", "no lines"],
)
def test_cuts_that_are_not_cuts_are_refused_when_declared(lines):
    with pytest.raises(RecipeError, match=r"lines|path|x|angle"):
        Cuts(lines=lines)


def test_values_written_as_a_recipe_file_writes_them_are_accepted():
    """TOML gives lists and tables, numpy arrays come from scripts: both make the same model."""
    from_file = Grid(columns=[1, 2], rows=[1, 1, 2], edge={"style": "torn", "amount": 9.6})
    from_script = Grid(columns=np.array([1, 2]), rows=(1, 1, 2), edge=Edge(style="torn", amount=9.6))  # type: ignore[arg-type]  # scripts pass arrays
    assert from_file == from_script
    assert Shards(impact=[0.2, 0.8]).impact == (0.2, 0.8)
    assert Pick(pieces=np.array([2, 3])).pieces == (2, 3)  # type: ignore[arg-type]  # scripts pass arrays
    cuts_from_file = Cuts(lines=[{"x": 0.2, "y": 0.4, "angle": 330}, {"path": WAVE}], edge={"style": "torn"})
    assert cuts_from_file == Cuts(lines=[Line(x=0.2, y=0.4, angle=330), Curve(path=WAVE)], edge=Edge(style="torn"))


def test_a_preset_sets_the_fields_and_a_field_given_beside_it_wins():
    glass = Shards(preset="glass")
    assert glass.focus > 0  # a preset's own value
    assert Shards(preset="glass", focus=0).focus == 0
    assert Shards(preset="glass", count=5).count == 5
    assert Shards(preset="glass", count=5).focus == glass.focus


@pytest.mark.parametrize(
    "model",
    [
        Grid,
        Shards,
        Pick,
        Subject,
        Edge,
        Cuts,
        Line,
        Curve,
        Path,
        Svg,
        Band,
        Bands,
        Combine,
        LinearGradient,
        RadialGradient,
    ],
)
def test_every_field_is_documented_in_help_with_its_unit_and_range(model):
    text = model.__doc__
    for each in fields(model):
        assert f"{each.name} : " in text
        assert each.metadata["doc"]


# --- Path.rect and Path.ellipse: a shape as shares of the frame ----------------------------------------------------


def painted_red(region: Path, picture: np.ndarray) -> np.ndarray:
    """A boolean map of the pixels a red `gradient` step masked to `region` turned red."""
    steps = [Step("gradient", params={"colors": ["#ff0000"]}, mask="shape")]
    out = render(picture, Recipe(steps, size=None, regions={"shape": region}))
    return (out == np.array([1.0, 0.0, 0.0], np.float32)).all(axis=-1)


def test_a_rect_paints_its_pixels_and_none_outside_it(photo):
    height, width = photo.shape[:2]
    red = painted_red(Path.rect(0.25, 0.25, 0.5, 0.5), photo)
    inside = np.zeros((height, width), bool)
    inside[height // 4 : 3 * height // 4, width // 4 : 3 * width // 4] = True  # whole pixels: the shares divide evenly
    np.testing.assert_array_equal(red, inside)


def test_a_rect_is_the_path_written_by_hand(photo):
    by_hand = Path(d="M 0.1 0.2 H 0.6 V 0.7 H 0.1 Z")
    np.testing.assert_array_equal(painted_red(Path.rect(0.1, 0.2, 0.5, 0.5), photo), painted_red(by_hand, photo))


def test_an_ellipse_covers_the_pixels_whose_centres_are_inside_it(photo):
    height, width = photo.shape[:2]
    cx, cy, rx, ry = 0.5, 0.45, 0.3, 0.25
    red = painted_red(Path.ellipse(cx, cy, rx, ry), photo)
    across = ((np.arange(width) + 0.5) / width - cx) / rx
    down = ((np.arange(height) + 0.5) / height - cy) / ry
    distance = np.hypot(down[:, None], across[None, :])  # 1 on the border
    far = np.abs(distance - 1) > 0.15  # leave the border pixels, which are partly covered
    np.testing.assert_array_equal(red[far], (distance < 1)[far])
    assert red[height // 2, width // 2]
    assert not red[0, 0]


@pytest.mark.parametrize(
    ("make", "name"),
    [
        (lambda size: Path.rect(0.1, 0.1, size, 0.5), "w"),
        (lambda size: Path.rect(0.1, 0.1, 0.5, size), "h"),
        (lambda size: Path.ellipse(0.5, 0.5, size, 0.2), "rx"),
        (lambda size: Path.ellipse(0.5, 0.5, 0.2, size), "ry"),
    ],
    ids=["w", "h", "rx", "ry"],
)
@pytest.mark.parametrize("size", [0, -0.2, float("nan")], ids=["zero", "negative", "nan"])
def test_a_size_that_is_not_above_zero_is_refused_naming_it(make, name, size):
    with pytest.raises(RecipeError, match=rf"`{name}`.*{size}"):
        make(size)


def test_a_shape_takes_the_other_fields_of_a_path():
    assert Path.rect(0, 0, 1, 1, samples=8).samples == 8
