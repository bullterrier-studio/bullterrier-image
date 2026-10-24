import numpy as np
import pytest
from PIL import Image

from bullterrier_image import RecipeError
from bullterrier_image.core.framing import Framing
from bullterrier_image.recipe import Recipe, Stage, Step, render
from bullterrier_image.sheet import contact_sheet, sweep, write_stages

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


def test_a_sweep_keeps_the_other_steps_as_they_are():
    tiles = sweep(LOOK, "grain.amount", 0, 0.2, 3)
    assert all(tile.steps[0] == GLITCH and tile.size == LOOK.size and tile.name == LOOK.name for _, tile in tiles)


def test_a_sweep_keeps_the_recipes_framing():
    framed = Recipe("framed", [GLITCH, GRAIN], size=48, frame=Framing(aspect=1.0, transparent="circle"))
    assert all(tile.frame == framed.frame for _, tile in sweep(framed, "grain.amount", 0, 0.2, 3))


def test_a_sweep_keeps_what_the_swept_step_has_around_the_parameter():
    masked = Step("grain", GRAIN_PARAMS, mask="subject", opacity=0.5, blend_mode="screen")
    recipe = Recipe("masked", [GLITCH, masked], size=48)
    for _, tile in sweep(recipe, "grain.amount", 0, 0.2, 3):
        assert (tile.steps[1].mask, tile.steps[1].opacity, tile.steps[1].blend_mode) == ("subject", 0.5, "screen")
        assert tile.steps[0] == GLITCH


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


def clear_red(size=8) -> np.ndarray:
    """A picture that is red and entirely transparent."""
    return np.dstack([np.tile(np.array([1, 0, 0], np.float32), (size, size, 1)), np.zeros((size, size), np.float32)])


def test_a_picture_with_transparency_is_shown_over_a_checkerboard_so_the_colour_under_it_never_shows():
    sheet = contact_sheet([("clear", clear_red())], tile=64)
    tile = sheet[14:78, 14:78]  # the gap is 14 px
    assert tile.shape == (64, 64, 3)
    assert not (tile[..., 0] - tile[..., 1] > 0.1).any()  # no red: the colours are greys
    assert len({tuple(np.round(pixel, 3)) for pixel in tile.reshape(-1, 3)}) == 2  # a light and a dark square


def test_an_opaque_four_channel_picture_is_shown_as_its_colour_and_a_half_clear_one_as_a_mix():
    photo = np.tile(np.array([0.2, 0.4, 0.6], np.float32), (8, 8, 1))
    opaque = contact_sheet([("a", np.dstack([photo, np.ones((8, 8), np.float32)]))], tile=64)
    np.testing.assert_allclose(opaque, contact_sheet([("a", photo)], tile=64), atol=1e-6)
    half = contact_sheet([("a", np.dstack([photo, np.full((8, 8), 0.5, np.float32)]))], tile=64)[14:78, 14:78]
    checker = contact_sheet([("a", clear_red())], tile=64)[14:78, 14:78]
    np.testing.assert_allclose(
        half, 0.5 * photo[0, 0] + 0.5 * checker, atol=1 / 255
    )  # half the colour, half the board, to 8 bits


def test_write_stages_saves_a_stage_with_transparency_as_png_and_the_others_as_jpg(photo, tmp_path):
    stages = [
        Stage("input", photo),
        Stage("1. transparent('circle')", np.dstack([photo, np.ones((96, 128), np.float32)])),
    ]
    sheet = write_stages(stages, tmp_path)
    assert sorted(path.name for path in tmp_path.iterdir()) == ["00-input.jpg", "01-transparent.png", "_sheet.jpg"]
    assert sheet == tmp_path / "_sheet.jpg"
    with Image.open(tmp_path / "01-transparent.png") as saved:
        assert saved.mode == "RGBA"
