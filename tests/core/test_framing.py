"""The `Framing` builder: the values it accepts and refuses. How a frame cuts, turns and pads a photo and its maps
is held by the Rust tests of the core (`run_frame`, `cutout`, `recipe_frame`)."""

import pytest

from bullterrier_image import RecipeError
from bullterrier_image.recipe import Framing


@pytest.mark.parametrize(
    ("values", "field"),
    [
        pytest.param({"aspect": 0}, "aspect", id="an aspect of 0"),
        pytest.param({"aspect": 11}, "aspect", id="an aspect past 10"),
        pytest.param({"angle": "tilted"}, "angle", id="an angle that is not a number"),
        pytest.param({"flip": "diagonal"}, "flip", id="a flip that is not offered"),
        pytest.param({"anchor": "middle"}, "anchor", id="an anchor that is not offered"),
        pytest.param({"pad_left": -0.1}, "pad_left", id="a negative border"),
        pytest.param({"fill": "red"}, "fill", id="a colour that is not written as one"),
        pytest.param({"transparent": "oval"}, "transparent", id="a cut that is not offered"),
        pytest.param({"corner_radius": 0.6}, "corner_radius", id="corners rounder than the picture allows"),
        pytest.param({"rotate": 10}, "rotate", id="a misspelt field"),
    ],
)
def test_a_bad_value_is_refused_naming_the_field(values, field):
    with pytest.raises(RecipeError, match=field):
        Framing(**values)


@pytest.mark.parametrize(
    "values",
    [
        {},
        {"aspect": 1.0, "anchor": "subject", "margin": 0.1},
        {"angle": -360, "fit": "expand", "fill": "#102030", "resample": "nearest"},
        {"pad_top": 2, "pad_right": 0.5, "pad_bottom": 0, "pad_left": 0.25},
        {"transparent": "rounded", "corner_radius": 0.5},
    ],
    ids=["the default", "a subject crop", "a turn", "borders", "rounded corners"],
)
def test_good_values_make_a_framing(values):
    assert isinstance(Framing(**values), Framing)


@pytest.mark.parametrize(
    ("written", "stored"),
    [
        pytest.param(180, 180, id="the half turn"),
        pytest.param(270, -90, id="past the half turn"),
        pytest.param(-450, -90, id="a turn and a quarter back"),
    ],
)
def test_an_angle_past_the_half_turn_is_the_same_turn_inside_it(written, stored):
    assert Framing(angle=written).angle == stored


# --- a crop around the subject --------------------------------------------------------------------------------------


# --- the cut --------------------------------------------------------------------------------------------------------
