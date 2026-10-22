"""A step's values are validated by its effect's parameter model, at once and over the whole animation; the recipe
objects (Step, Recipe, Animated, Animation) are validated, frozen models wherever they are made."""

import numpy as np
import pytest

from photo_fx.core.loader import load_recipe
from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.core.steps import Recipe, Step
from photo_fx.core.timing import Animated, Animation


class GainParams(EffectParams):
    """Gain of the probe."""

    gain: float = param(1.0, unit="ratio", min=0, max=4, doc="Multiplies every value. Raising it brightens.")
    offset: float = param(0.0, unit="fraction", min=-1, max=1, doc="Added after the gain.")


@pytest.fixture
def gain(throwaway_effects):
    def gain_probe(rgb: np.ndarray, params: GainParams) -> np.ndarray:
        """Brightens by a gain."""
        return np.clip(rgb * params.gain + params.offset, 0, 1).astype(np.float32)

    return effect(category="light", params=GainParams)(gain_probe)


OVERSHOOT = (0.3, 1.6, 0.7, 1.0)  # y1 above 1: the curve rises past its end before settling on it


# --- values through the model ----------------------------------------------------------------------------------------


def test_a_value_out_of_range_names_the_effect_the_field_the_limit_and_the_value(gain):
    with pytest.raises(ValueError, match="gain_probe") as raised:
        Step("gain_probe", {"gain": 9})
    message = str(raised.value)
    assert "`gain`" in message
    assert "<= 4" in message
    assert "9" in message


def test_an_unknown_value_lists_the_valid_ones(gain):
    with pytest.raises(ValueError, match=r"`gian`.*gain, offset"):
        Step("gain_probe", {"gian": 2})


def test_a_value_of_the_wrong_kind_is_refused(gain):
    with pytest.raises(ValueError, match=r"`gain`.*number"):
        Step("gain_probe", {"gain": "2"})


def test_valid_values_make_a_step_that_keeps_them_as_written(gain):
    step = Step("gain_probe", {"gain": 2})
    assert step.params == {"gain": 2}
    assert step.describe() == "gain_probe(gain=2)"


# --- animated values: both ends and the way between ------------------------------------------------------------------


def test_an_animation_inside_the_range_is_accepted(gain):
    assert Step("gain_probe", {"gain": Animated(0.0, 4.0, "gentle")}).animated


def test_an_animation_ending_out_of_range_is_refused(gain):
    with pytest.raises(ValueError, match=r"`gain`.*<= 4.*9"):
        Step("gain_probe", {"gain": Animated(1.0, 9.0)})


def test_an_animation_starting_out_of_range_is_refused(gain):
    with pytest.raises(ValueError, match=r"`offset`.*>= -1"):
        Step("gain_probe", {"offset": Animated(-3.0, 0.0)})


def test_an_easing_curve_that_overshoots_the_range_between_its_ends_is_refused(gain):
    """Both ends (0 and 4) are allowed; the curve passes 4 on the way, so frames in the middle would be invalid."""
    assert Animated(0.0, 4.0, OVERSHOOT).at(0.5) > 4.0  # the premise: it really overshoots
    with pytest.raises(ValueError, match=r"`gain`.*<= 4"):
        Step("gain_probe", {"gain": Animated(0.0, 4.0, OVERSHOOT)})


def test_an_overshooting_curve_that_stays_inside_the_range_is_accepted(gain):
    assert Step("gain_probe", {"gain": Animated(0.0, 2.0, OVERSHOOT)}).animated  # peaks below 4


def test_an_animation_that_overshoots_in_a_recipe_file_names_the_file_step_and_field(gain, tmp_path):
    path = tmp_path / "bouncy.toml"
    path.write_text(
        "[animation]\nseconds = 2\n\n[[step]]\neffect = 'grade'\n\n[[step]]\neffect = 'gain_probe'\n"
        "gain = { from = 0, to = 4, ease = [0.3, 1.6, 0.7, 1.0] }\n"
    )
    with pytest.raises(ValueError, match=r"bouncy\.toml, step 2.*`gain`.*<= 4"):
        load_recipe(path)


def test_a_value_out_of_range_in_a_recipe_file_names_the_file_step_field_and_value(gain, tmp_path):
    path = tmp_path / "hot.toml"
    path.write_text("[[step]]\neffect = 'gain_probe'\ngain = 7.5\n")
    with pytest.raises(ValueError, match=r"hot\.toml, step 1: effect 'gain_probe'.*`gain`.*<= 4.*7\.5"):
        load_recipe(path)


# --- the recipe objects are validated, frozen models -----------------------------------------------------------------


@pytest.mark.parametrize(
    ("make", "complaint"),
    [
        (lambda: Animated(0.0, 1.0, (0.4, 0.0, 0.2)), "ease"),  # three numbers
        (lambda: Animated(0.0, 1.0, (0.4, 0.0, 0.2, 1.0, 0.5)), "ease"),  # five
        (lambda: Animated(0.0, 1.0, (1.5, 0.0, 0.2, 1.0)), "ease"),  # x1 beyond 0..1
        (lambda: Animated(0.0, 1.0, (0.4, 0.0, -0.1, 1.0)), "ease"),  # x2 beyond 0..1
        (lambda: Animated(0.0, 1.0, (True, 0.0, 0.2, 1.0)), "ease"),
        (lambda: Animated(0.0, 1.0, "wobbly"), "wobbly"),
        (lambda: Animated(0.0, 1.0, over="forever"), "over"),
        (lambda: Animated(True, 1.0), "start"),
        (lambda: Animated("0", 1.0), "start"),  # type: ignore[arg-type]  # the point of the test
    ],
)
def test_a_bad_animated_value_made_in_code_is_refused(make, complaint):
    with pytest.raises(ValueError, match=complaint):
        make()


def test_a_custom_cubic_bezier_of_four_numbers_with_x_in_range_is_accepted():
    assert Animated(0, 1, [0.0, -0.5, 1.0, 1.5]).ease == (0.0, -0.5, 1.0, 1.5)  # y may leave 0..1, x may not


@pytest.mark.parametrize(
    ("values", "complaint"),
    [
        ({"seconds": 0}, "seconds"),
        ({"seconds": -1.0}, "seconds"),
        ({"seconds": 2, "fps": 0}, "fps"),
        ({"seconds": 2, "fps": 2.5}, "fps"),
        ({"seconds": 2, "hold_start": -0.5}, "hold_start"),
        ({"seconds": 2, "hold_end": -0.5}, "hold_end"),
        ({"seconds": 2, "hold_start": 1.0, "hold_end": 1.0}, "no time"),
        ({"seconds": 2, "loop": "spiral"}, "loop"),
    ],
)
def test_a_bad_animation_made_in_code_is_refused(values, complaint):
    with pytest.raises(ValueError, match=complaint):
        Animation(**values)


def test_a_recipe_needs_a_positive_size_or_none():
    assert Recipe("r", [Step("grade")], size=None).size is None
    with pytest.raises(ValueError, match="size"):
        Recipe("r", [Step("grade")], size=0)


def test_a_step_needs_a_known_mask_and_an_opacity_from_0_to_1():
    with pytest.raises(ValueError, match=r"mask.*'sideways'"):
        Step("grade", mask="sideways")
    with pytest.raises(ValueError, match="opacity"):
        Step("grade", opacity=-0.1)


def test_the_recipe_objects_are_frozen():
    step, animation = Step("grade"), Animation(seconds=1)
    with pytest.raises(ValueError, match="frozen"):
        step.opacity = 0.5  # type: ignore[misc]  # the point of the test
    with pytest.raises(ValueError, match="frozen"):
        animation.fps = 12  # type: ignore[misc]  # the point of the test


def test_the_recipe_objects_refuse_unknown_fields():
    with pytest.raises(ValueError, match="speed"):
        Animation.model_validate({"seconds": 1, "speed": 2})
    with pytest.raises(ValueError, match="colour"):
        Step.model_validate({"effect": "grade", "colour": "red"})
