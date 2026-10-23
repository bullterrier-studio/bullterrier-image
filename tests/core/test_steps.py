"""A step's values are validated by its effect's parameter model, at once and over the whole animation; the recipe
objects (Step, Recipe, Animated, Animation) are validated, frozen models wherever they are made."""

import re

import numpy as np
import pytest

from photo_fx.core.loader import load_recipe
from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.core.steps import Recipe, Step
from photo_fx.core.timing import Animated, Animation
from photo_fx.errors import RecipeError


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


# --- every rendered frame of a recipe is checked, not only a few moments ---------------------------------------------

NARROW = [0.95, 1.0, 1.0, 1.08]  # overshoots only very near its end: between the 11 moments a lone step checks


def _first_frame_past_one(seconds: float, fps: int) -> int:
    """The first frame at which a 0 -> 1 animation with the NARROW curve goes above 1, found by walking the frames."""
    curve = Animated(0.0, 1.0, NARROW)
    return next(i for i, (motion, clip) in enumerate(Animation(seconds, fps).frames()) if curve.at(motion, clip) > 1)


def test_a_narrow_overshoot_passes_the_steps_own_check_but_fails_the_recipe_at_the_frame_it_would_break(tmp_path):
    assert Step("split_tone", {"amount": Animated(0.0, 1.0, NARROW)}).animated  # alone, the step's 11 moments pass
    path = tmp_path / "narrow.toml"
    path.write_text(
        "[animation]\nseconds = 4\nfps = 30\n\n[[step]]\neffect = 'grade'\n\n[[step]]\neffect = 'split_tone'\n"
        "amount = { from = 0, to = 1, ease = [0.95, 1.0, 1.0, 1.08] }\n",
        encoding="utf-8",
    )
    frame = _first_frame_past_one(4, 30)
    with pytest.raises(ValueError, match=r"narrow\.toml.*step 2.*split_tone.*`amount`.*\b1\b.*\b1\.00\d*") as raised:
        load_recipe(path)
    message = str(raised.value)
    assert re.search(rf"frame\D*\b{frame}\b", message)  # the frame that would break, by number
    assert re.search(rf"\b{frame / 30:.1f}\d*", message)  # and its moment in the clip, in seconds


def test_every_error_about_a_step_in_a_file_names_the_file_and_the_step_the_same_way(gain, tmp_path):
    """A step's own check and the recipe's frame check report in one form, so a reader (or a tool) finds both alike."""
    plain, framed = tmp_path / "plain.toml", tmp_path / "framed.toml"
    plain.write_text("[[step]]\neffect = 'grade'\n\n[[step]]\neffect = 'gain_probe'\ngain = 7.5\n", encoding="utf-8")
    framed.write_text(
        "[animation]\nseconds = 4\n\n[[step]]\neffect = 'grade'\n\n[[step]]\neffect = 'gain_probe'\n"
        "gain = { from = 0, to = 4, ease = [0.95, 1.0, 1.0, 1.08] }\n",
        encoding="utf-8",
    )
    prefixes = []
    for path in (plain, framed):
        with pytest.raises(ValueError, match=r"`gain`") as raised:
            load_recipe(path)
        prefixes.append(str(raised.value).split("`gain`")[0].replace(path.name, "FILE"))
    assert "step 2" in prefixes[0]
    assert prefixes[0] == prefixes[1]


def test_a_recipe_whose_every_frame_is_valid_loads(gain):
    recipe = Recipe("ok", [Step("gain_probe", {"gain": Animated(0.0, 3.6, NARROW)})], None, Animation(4, 30))
    assert recipe.animation == Animation(4, 30)  # peaks just above 3.6, still inside 0..4


# --- whole-number fields move in whole steps -------------------------------------------------------------------------


class LevelsParams(EffectParams):
    """Levels of the probe."""

    levels: int = param(4, unit="count", min=2, max=16, doc="How many levels.")


@pytest.fixture
def levels(throwaway_effects):
    def levels_probe(rgb: np.ndarray, params: LevelsParams) -> np.ndarray:
        """Fills the picture with levels / 16, so the level used can be read back from any pixel."""
        assert isinstance(params.levels, int)
        return np.full_like(rgb, params.levels / 16)

    return effect(category="utilities", params=LevelsParams)(levels_probe)


def test_an_animated_whole_number_is_rounded_to_the_nearest_whole_number(levels):
    step = Step("levels_probe", {"levels": Animated(2, 8)})  # 2 + 6 t
    assert step.params_at(0.0) == {"levels": 2}
    assert step.params_at(1.0) == {"levels": 8}
    assert step.params_at(0.5) == {"levels": 5}
    assert step.params_at(0.25) == {"levels": 4}  # 3.5: halves go to the even neighbour (Python's round)
    assert step.params_at(0.75) == {"levels": 6}  # 6.5
    assert all(isinstance(value, int) for value in step.params_at(0.4).values())


def test_a_rendered_frame_uses_the_rounded_whole_number(levels, photo):
    from photo_fx.core.runner import render

    recipe = Recipe("lv", [Step("levels_probe", {"levels": Animated(2, 8)})], None, Animation(2, 10))
    np.testing.assert_array_equal(render(photo, recipe, at=0.5), np.float32(5 / 16))


def test_an_animated_whole_number_is_checked_after_rounding(levels):
    assert Step("levels_probe", {"levels": Animated(2, 16.4)}).animated  # ends at 16: inside 2..16
    with pytest.raises(ValueError, match=r"levels.*16.*17"):
        Step("levels_probe", {"levels": Animated(2, 16.6)})  # ends at 17


# --- values through the model, animated values at both ends and on the way between ----------------------------------


@pytest.mark.parametrize(
    ("values", "complaint"),
    [
        ({"gain": 9}, r"`gain`.*<= 4.*\b9\b"),  # the field, the limit and the value
        ({"gian": 2}, r"`gian`.*gain, offset"),  # an unknown value lists the valid ones
        ({"gain": "2"}, r"`gain`.*number"),
        ({"gain": Animated(1.0, 9.0)}, r"`gain`.*<= 4.*9"),  # ending out of range
        ({"offset": Animated(-3.0, 0.0)}, r"`offset`.*>= -1"),  # starting out of range
        # both ends (0 and 4) are allowed, but the curve passes 4 on the way: frames in the middle would be invalid
        ({"gain": Animated(0.0, 4.0, OVERSHOOT)}, r"`gain`.*<= 4"),
    ],
    ids=["out of range", "unknown", "wrong kind", "animation ends out", "animation starts out", "curve overshoots"],
)
def test_a_bad_value_is_refused_naming_the_effect_and_the_problem(gain, values, complaint):
    with pytest.raises(ValueError, match=f"gain_probe.*{complaint}"):
        Step("gain_probe", values)


def test_the_overshoot_premise_holds():
    assert Animated(0.0, 4.0, OVERSHOOT).at(0.5) > 4.0  # the curve refused above really leaves 0..4


def test_valid_values_make_a_step_that_keeps_them_as_written(gain):
    step = Step("gain_probe", {"gain": 2})
    assert step.params == {"gain": 2}
    assert step.describe() == "gain_probe(gain=2)"


@pytest.mark.parametrize(
    "animated", [Animated(0.0, 4.0, "gentle"), Animated(0.0, 2.0, OVERSHOOT)], ids=["inside", "overshoot peaks below 4"]
)
def test_an_animation_inside_the_range_all_the_way_is_accepted(gain, animated):
    assert Step("gain_probe", {"gain": animated}).animated


def test_an_animation_that_overshoots_in_a_recipe_file_names_the_file_step_and_field(gain, tmp_path):
    path = tmp_path / "bouncy.toml"
    path.write_text(
        "[animation]\nseconds = 2\n\n[[step]]\neffect = 'grade'\n\n[[step]]\neffect = 'gain_probe'\n"
        "gain = { from = 0, to = 4, ease = [0.3, 1.6, 0.7, 1.0] }\n",
        encoding="utf-8",
    )
    with pytest.raises(ValueError, match=r"bouncy\.toml.*step 2.*`gain`.*\b4\b"):
        load_recipe(path)


def test_a_value_out_of_range_in_a_recipe_file_names_the_file_step_field_and_value(gain, tmp_path):
    path = tmp_path / "hot.toml"
    path.write_text("[[step]]\neffect = 'gain_probe'\ngain = 7.5\n", encoding="utf-8")
    with pytest.raises(ValueError, match=r"hot\.toml.*step 1.*gain_probe.*`gain`.*\b4\b.*7\.5"):
        load_recipe(path)


# --- the recipe objects are validated, frozen models -----------------------------------------------------------------


@pytest.mark.parametrize(
    ("make", "complaint"),
    [
        (lambda: Animated(0.0, 1.0, (0.4, 0.0, 0.2)), "ease"),  # three numbers
        (lambda: Animated(0.0, 1.0, (0.4, 0.0, 0.2, 1.0, 0.5)), "ease"),  # five
        (lambda: Animated(0.0, 1.0, (1.5, 0.0, 0.2, 1.0)), "ease"),  # x1 beyond 0..1
        (lambda: Animated(0.0, 1.0, (-0.1, 0.0, 0.2, 1.0)), "ease"),
        (lambda: Animated(0.0, 1.0, (0.4, 0.0, -0.1, 1.0)), "ease"),  # x2 beyond 0..1
        (lambda: Animated(0.0, 1.0, (0.4, 0.0, 1.5, 1.0)), r"ease.*1\.5"),  # names the value given
        (lambda: Animated(0.0, 1.0, (True, 0.0, 0.2, 1.0)), "ease"),
        (lambda: Animated(0.0, 1.0, (0.4, 0.0, "x", 1.0)), "ease"),  # type: ignore[arg-type]  # the point of the test
        (lambda: Animated(0.0, 1.0, "wobbly"), "wobbly"),
        (lambda: Animated(0.0, 1.0, over="forever"), "over"),
        (lambda: Animated(True, 1.0), "start"),
        (lambda: Animated("0", 1.0), "start"),  # type: ignore[arg-type]  # the point of the test
    ],
)
def test_a_bad_animated_value_made_in_code_is_a_recipe_error_naming_it(make, complaint):
    with pytest.raises(RecipeError, match=complaint):
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
def test_a_bad_animation_made_in_code_is_a_recipe_error_naming_it(values, complaint):
    with pytest.raises(RecipeError, match=complaint):
        Animation(**values)


def test_a_recipe_needs_a_positive_size_or_none():
    assert Recipe("r", [Step("grade")], size=None).size is None
    with pytest.raises(ValueError, match="size"):
        Recipe("r", [Step("grade")], size=0)


# A recipe's name becomes an output file name, and recipes are shared as data: a name that could leave the output folder
# (or hide a file there) is refused when the recipe is made (CWE-22, path traversal).
@pytest.mark.parametrize(
    "name", ["../../x", "/abs/x", "a/b", "", ".hidden", "a" * 65, "a\\b", "look\n", "..", "-rf", "my look"]
)
def test_a_recipe_name_that_is_not_a_safe_file_name_is_refused_naming_it_and_the_allowed_pattern(name):
    with pytest.raises(RecipeError) as raised:
        Recipe(name, [Step("grade")])
    message = str(raised.value)
    assert repr(name)[:20] in message  # the value given
    assert "A-Za-z0-9" in message  # the pattern a name must match


# Windows reserves these device names in any case, with or without an extension: a file so named cannot be made there.
@pytest.mark.parametrize(
    "name", ["CON", "con", "Prn", "AUX", "nul", "COM1", "com9", "LPT1", "lpt9", "CON.png", "nul.tar.gz", "Aux.v2"]
)
def test_a_name_windows_reserves_for_a_device_is_refused_naming_it(name):
    with pytest.raises(RecipeError, match="reserved") as raised:
        Recipe(name, [Step("grade")])
    assert repr(name) in str(raised.value)


@pytest.mark.parametrize(
    "name",
    [
        *("my-look", "look_2.v1", "a" * 64, "7", "Neon.Pulse-2"),  # safe file names
        *("CONSOLE", "com10", "nullify", "aux-look", "my.con", "LPT", "printer"),  # only resembling a device name
    ],
)
def test_a_safe_name_is_kept_as_the_recipe_name(name):
    assert Recipe(name, [Step("grade")]).name == name


def test_an_explicit_name_in_a_file_is_not_repaired(tmp_path):
    path = tmp_path / "fine.toml"
    path.write_text("name = 'my look'\n[[step]]\neffect = 'grade'\n", encoding="utf-8")
    with pytest.raises(RecipeError, match=r"^fine\.toml: .*'my look'"):
        load_recipe(path)


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
