import numpy as np
import pytest

from photo_fx import RecipeError
from photo_fx.recipe import Animation, Recipe, Step, ramp, render
from photo_fx.sheet import sweep

GLITCH_PARAMS = {"split": 4, "bands": 3, "seed": 5}
GRAIN_PARAMS = {"amount": 0.1, "size": 1.6, "seed": 2}
GLITCH = Step("glitch", GLITCH_PARAMS)
GRAIN = Step("grain", GRAIN_PARAMS)
LOOK = Recipe("look", [GLITCH, GRAIN], size=48)
TWICE = Recipe("twice", [GRAIN, GLITCH, Step("grain", {"amount": 0.3})], size=48)


def labels(tiles) -> list[str]:
    return [label for label, _ in tiles]


# Each row: the target, the range, the labels the tiles carry and the values the swept parameter takes (written out by
# hand: an even spacing of the range, a whole-number parameter rounded to the nearest and made unique).
SWEEPS = {
    "an even spacing of a number": ("grain.amount", 0, 0.2, 5, [0, 0.05, 0.1, 0.15, 0.2]),
    "a descending range": ("grain.amount", 0.2, 0, 3, [0.2, 0.1, 0]),
    "whole numbers rounded to the nearest": ("glitch.split", 0, 10, 4, [0, 3, 7, 10]),
    "whole numbers made unique": ("glitch.split", 0, 2, 4, [0, 1, 2]),
    "a negative whole number": ("glitch.split", -4, 4, 3, [-4, 0, 4]),
}


@pytest.mark.parametrize(("target", "start", "end", "count", "values"), SWEEPS.values(), ids=SWEEPS.keys())
def test_a_sweep_renders_the_recipe_once_for_each_value_of_the_parameter(photo, target, start, end, count, values):
    effect, name = target.split(".")
    tiles = sweep(LOOK, target, start, end, count)
    assert labels(tiles) == [f"{name}={value:g}" for value in values]
    for (_, tile), value in zip(tiles, values, strict=True):
        if effect == "glitch":
            steps = [Step("glitch", GLITCH_PARAMS | {name: value}), GRAIN]
        else:
            steps = [GLITCH, Step("grain", GRAIN_PARAMS | {name: value})]
        np.testing.assert_array_equal(render(photo, tile), render(photo, Recipe("by-hand", steps, size=48)))


def test_the_value_replaces_an_animation_of_the_parameter(photo):
    animated = Recipe("moving", [Step("grain", {"amount": ramp(0.5, 0.9)})], size=48, animation=Animation(1, 5))
    _, tile = sweep(animated, "grain.amount", 0.2, 0.4, 2)[0]
    by_hand = Recipe("by-hand", [Step("grain", {"amount": 0.2})], size=48)
    np.testing.assert_array_equal(render(photo, tile), render(photo, by_hand))


def test_a_sweep_keeps_the_other_steps_as_they_are():
    tiles = sweep(LOOK, "grain.amount", 0, 0.2, 3)
    assert all(tile.steps[0] == GLITCH and tile.size == LOOK.size and tile.name == LOOK.name for _, tile in tiles)


def test_a_sweep_keeps_what_the_swept_step_and_the_recipe_have_around_the_parameter():
    masked = Step("grain", GRAIN_PARAMS, mask="subject", opacity=0.5)
    moving = Step("glitch", {"shift": ramp(0.1, 0.3)})
    recipe = Recipe("moving", [moving, masked], size=48, animation=Animation(1, 5))
    for _, tile in sweep(recipe, "grain.amount", 0, 0.2, 3):
        assert (tile.steps[1].mask, tile.steps[1].opacity) == ("subject", 0.5)
        assert tile.steps[0] == moving
        assert tile.animation == recipe.animation


def test_effect_number_picks_one_of_an_effect_the_recipe_has_twice():
    tiles = sweep(TWICE, "grain#2.amount", 0, 0.2, 3)
    assert [tile.steps[0] for _, tile in tiles] == [GRAIN] * 3  # the first grain stays as written
    assert [tile.steps[2].params["amount"] for _, tile in tiles] == [0, 0.1, 0.2]
    assert sweep(TWICE, "grain#1.amount", 0, 0.2, 3)[0][1].steps[2] == TWICE.steps[2]


# Each row differs from a working sweep of `TWICE` (`grain#1.amount`, 0 to 0.2, 5 tiles) by its mistake only; the
# error names the offender.
MISTAKES = {
    "a target without a parameter": (("grain", 0, 0.2, 5), "'grain'"),
    "an effect the recipe does not have": (("blur.radius", 0, 0.2, 5), "'blur'"),
    "an effect twice without its number": (("grain.amount", 0, 0.2, 5), "grain#1.amount"),
    "an effect number beyond the steps": (("grain#3.amount", 0, 0.2, 5), "#3"),
    "an effect number of zero": (("grain#0.amount", 0, 0.2, 5), "#0"),
    "a parameter the effect does not have": (("grain#1.amoun", 0, 0.2, 5), "amoun"),
    "a parameter that is not a number": (("fill.color", 0, 1, 5), "color"),
    "a value above the parameter's range": (("grain#1.amount", 0, 5, 5), "amount"),
    "a value below the parameter's range": (("grain#1.amount", -1, 0.2, 5), "amount"),
    "one tile": (("grain#1.amount", 0, 0.2, 1), "count"),
    "a range of one value": (("grain#1.amount", 0.1, 0.1, 5), "start and end"),
    "more tiles than a sheet can show": (("grain#1.amount", 0, 0.2, 25), "count"),
}


@pytest.mark.parametrize(("arguments", "offender"), MISTAKES.values(), ids=MISTAKES.keys())
def test_a_sweep_that_cannot_be_made_is_refused_naming_the_offender(arguments, offender):
    recipe = Recipe("twice", [*TWICE.steps, Step("fill", {"color": "#000000"})], size=48)
    with pytest.raises(RecipeError, match=offender):
        sweep(recipe, *arguments)


@pytest.mark.parametrize("count", [2, 24])
def test_the_smallest_and_the_largest_sheet_are_accepted(count):
    assert len(sweep(TWICE, "grain#1.amount", 0, 0.2, count)) == count
