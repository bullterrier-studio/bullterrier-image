"""The Python builders of regions: the models that declare them, their documentation and the forms they accept.

Where a region's pieces fall, what a reference may name and the ranges of each kind are held by the Rust tests of the
core (`region_check`, `recipe_references`, `run_regions`, `pieces`, `shards`, `svg`, `torn`), and the renders by
`tests/_native/test_region_parity.py`.
"""

import numpy as np
import pytest

from bullterrier_image import RecipeError
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
)

WAVE = "M 0.38 0 C 0.15 0.45, 0.85 0.55, 0.62 1"

BAD_VALUES = {
    "no columns": (Grid, {"columns": 0}, "columns"),
    "a share that is not positive": (Grid, {"rows": [1, 0]}, "rows"),
    "a misspelt field": (Grid, {"column": 3}, "column"),
    "too many shards": (Shards, {"count": 501}, "count"),
    "an impact outside the frame": (Shards, {"impact": (1.5, 0.5)}, "impact"),
    "a preset that does not exist": (Shards, {"preset": "nope"}, "nope.*glass"),
    "an edge style that is not offered": (Shards, {"edge": {"style": "wavy"}}, "style"),
    "no pieces to pick": (Pick, {"pieces": []}, "pieces"),
    "context past 1": (Subject, {"context": 1.5}, "context"),
    "low above high": (Band, {"low": 0.6, "high": 0.3}, r"low.*0\.6.*high.*0\.3"),
    "a band count named by its kind as a recipe writes it": (Bands, {"count": 0}, "^bands: .*count"),
    "a combination named by its kind as a recipe writes it": (Combine, {"op": "xor"}, "^combine: .*op"),
    "a gradient named by its kind as a recipe writes it": (
        LinearGradient,
        {"angle": "up"},
        "^linear_gradient: .*angle",
    ),
    "a radial gradient named by its kind as a recipe writes it": (
        RadialGradient,
        {"radius": 0},
        "^radial_gradient: .*radius",
    ),
}


@pytest.mark.parametrize(("model", "values", "complaint"), BAD_VALUES.values(), ids=BAD_VALUES.keys())
def test_a_bad_value_is_refused_naming_the_field(model, values, complaint):
    with pytest.raises(RecipeError, match=complaint):
        model(**values)


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
    from_script = Grid(columns=np.array([1, 2]), rows=(1, 1, 2), edge=Edge(style="torn", amount=9.6))
    assert from_file == from_script
    assert Shards(impact=[0.2, 0.8]).impact == (0.2, 0.8)
    assert Pick(pieces=np.array([2, 3])).pieces == (2, 3)
    from_file = Cuts(lines=[{"x": 0.2, "y": 0.4, "angle": 330}, {"path": WAVE}], edge={"style": "torn"})
    assert from_file == Cuts(lines=[Line(x=0.2, y=0.4, angle=330), Curve(path=WAVE)], edge=Edge(style="torn"))


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
def test_every_field_is_documented_with_its_unit_and_range(model):
    text = model.documentation()
    for doc in model.fields_doc():
        assert doc.name in text
        assert doc.doc
        if doc.minimum is not None:
            assert doc.unit
