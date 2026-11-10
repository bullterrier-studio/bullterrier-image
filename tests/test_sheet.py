import numpy as np
import pytest
from PIL import Image

from bullterrier_image import RecipeError
from bullterrier_image.core.params import Drive, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.core.steps import runs
from bullterrier_image.recipe import Band, Each, Framing, Grid, Recipe, Stage, Step, render
from bullterrier_image.sheet import sweep, write_stages

SLIDE_PARAMS = {"bands": 3, "seed": 5}
GRAIN_PARAMS = {"amount": 0.1, "size": 1.6, "seed": 2}
SLIDE = Step("slice_shift", params=SLIDE_PARAMS)
GRAIN = Step("grain", params=GRAIN_PARAMS)
LOOK = Recipe([SLIDE, GRAIN], name="look", size=48)
TWICE = Recipe([GRAIN, SLIDE, Step("grain", params={"amount": 0.3})], name="twice", size=48)


def labels(tiles) -> list[str]:
    return [label for label, _ in tiles]


# Each row: the target, the range, the labels the tiles carry and the values the swept parameter takes (written out by
# hand: an even spacing of the range, a whole-number parameter rounded to the nearest and made unique).
SWEEPS = {
    "an even spacing of a number": ("grain.amount", 0, 0.2, 5, [0, 0.05, 0.1, 0.15, 0.2]),
    "a descending range": ("grain.amount", 0.2, 0, 3, [0.2, 0.1, 0]),
    "whole numbers rounded to the nearest": ("slice_shift.bands", 0, 10, 4, [0, 3, 7, 10]),
    "whole numbers made unique": ("slice_shift.bands", 0, 2, 4, [0, 1, 2]),
}


@pytest.mark.parametrize(("target", "start", "end", "count", "values"), SWEEPS.values(), ids=SWEEPS.keys())
def test_a_sweep_renders_the_recipe_once_for_each_value_of_the_parameter(photo, target, start, end, count, values):
    effect, name = target.split(".")
    tiles = sweep(LOOK, target, start, end, count)
    assert labels(tiles) == [f"{name}={value:g}" for value in values]
    for (_, tile), value in zip(tiles, values, strict=True):
        if effect == "slice_shift":
            steps = [Step("slice_shift", params=SLIDE_PARAMS | {name: value}), GRAIN]
        else:
            steps = [SLIDE, Step("grain", params=GRAIN_PARAMS | {name: value})]
        np.testing.assert_array_equal(render(photo, tile), render(photo, Recipe(steps, name="by-hand", size=48)))


class OffsetParams(EffectParams):
    """The one whole number of the probe, which may be negative."""

    offset: int = param(0, unit="px", min=-10, max=10, doc="A whole number from -10 to 10 that the probe ignores.")


def test_a_sweep_through_negative_whole_numbers_rounds_each_to_the_nearest_whole_number(throwaway_effects):
    @effect(category="utilities", params=OffsetParams)
    def offset_probe(rgb: np.ndarray, params: OffsetParams) -> np.ndarray:
        """Returns the picture as it is."""
        return rgb

    tiles = sweep(Recipe([Step("offset_probe", params={})], name="probe", size=48), "offset_probe.offset", -4, 4, 4)
    assert labels(tiles) == ["offset=-4", "offset=-1", "offset=1", "offset=4"]  # -4, -1.33, 1.33, 4


def test_a_sweep_keeps_the_other_steps_as_they_are():
    tiles = sweep(LOOK, "grain.amount", 0, 0.2, 3)
    assert all(tile.steps[0] == SLIDE and tile.size == LOOK.size and tile.name == LOOK.name for _, tile in tiles)


def test_a_sweep_keeps_the_recipes_framing():
    framed = Recipe([SLIDE, GRAIN], name="framed", size=48, frame=Framing(aspect=1.0, transparent="circle"))
    assert all(tile.frame == framed.frame for _, tile in sweep(framed, "grain.amount", 0, 0.2, 3))


def test_a_sweep_keeps_what_the_swept_step_has_around_the_parameter():
    masked = Step("grain", params=GRAIN_PARAMS, mask="subject", opacity=0.5, blend_mode="screen")
    recipe = Recipe([SLIDE, masked], name="masked", size=48)
    for _, tile in sweep(recipe, "grain.amount", 0, 0.2, 3):
        assert (tile.steps[1].mask, tile.steps[1].opacity, tile.steps[1].blend_mode) == ("subject", 0.5, "screen")
        assert tile.steps[0] == SLIDE


def test_effect_number_picks_one_of_an_effect_the_recipe_has_twice():
    tiles = sweep(TWICE, "grain#2.amount", 0, 0.2, 3)
    assert [tile.steps[0] for _, tile in tiles] == [GRAIN] * 3  # the first grain stays as written
    assert [runs(tile.steps[2])[0].params["amount"] for _, tile in tiles] == [0, 0.1, 0.2]
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
    "a parameter that is not a number": (("gradient.colors", 0, 1, 5), "color"),
    "a value above the parameter's range": (("grain#1.amount", 0, 5, 5), "amount"),
    "one tile": (("grain#1.amount", 0, 0.2, 1), "count"),
    "a range of one value": (("grain#1.amount", 0.1, 0.1, 5), "start and end"),
    "more tiles than a sheet can show": (("grain#1.amount", 0, 0.2, 25), "count"),
}


@pytest.mark.parametrize(("arguments", "offender"), MISTAKES.values(), ids=MISTAKES.keys())
def test_a_sweep_that_cannot_be_made_is_refused_naming_the_offender(arguments, offender):
    recipe = Recipe([*TWICE.steps, Step("gradient", params={"colors": ["#000000"]})], name="twice", size=48)
    with pytest.raises(RecipeError, match=offender):
        sweep(recipe, *arguments)


@pytest.mark.parametrize("count", [2, 24])
def test_the_smallest_and_the_largest_sheet_are_accepted(count):
    assert len(sweep(TWICE, "grain#1.amount", 0, 0.2, count)) == count


def test_write_stages_names_each_stage_and_saves_one_with_transparency_as_png_and_the_others_as_jpg(photo, tmp_path):
    stages = [
        Stage("input", photo),
        Stage("subject matte (white = subject)", photo),
        Stage("depth (white = nearest)", photo),
        Stage("1. transparent('circle')", np.dstack([photo, np.ones((96, 128), np.float32)])),
    ]
    sheet = write_stages(stages, tmp_path)
    assert sorted(path.name for path in tmp_path.iterdir()) == [
        "00-input.jpg",
        "01-matte.jpg",
        "02-depth.jpg",
        "03-transparent.png",
        "_sheet.jpg",
    ]
    assert sheet == tmp_path / "_sheet.jpg"
    with Image.open(tmp_path / "03-transparent.png") as saved:
        assert saved.mode == "RGBA"


def test_a_sweep_keeps_the_partition_a_step_draws_and_the_regions_and_each_steps_of_the_recipe():
    seams = Step("seams", params={"width": 0.5}, partition="panes")
    look = Recipe([Each("panes", looks=[None]), seams], name="panes", size=48, regions={"panes": Grid(columns=2)})
    tiles = sweep(look, "seams.width", 0.1, 1, 3)
    assert [tile.steps[1].partition for _, tile in tiles] == ["panes"] * 3  # type: ignore[union-attr]
    assert all(tile.steps[0] == look.steps[0] and tile.regions == look.regions for _, tile in tiles)


# --- a sweep by any address -------------------------------------------------------------------------------------------


def test_a_sweep_can_vary_the_frame_a_region_a_named_step_or_the_end_of_a_drive():
    look = Recipe(
        [
            Step("grain", params=GRAIN_PARAMS, name="dust"),
            Step("levels", params={"output_black": Drive(by="near", start=0.0, end=0.2)}),
        ],
        name="look",
        size=48,
        frame=Framing(angle=10),
        regions={"near": Band(of="depth", low=0.4)},
    )
    assert [r.frame.angle for _, r in sweep(look, "frame.angle", 0, 30, 3)] == [0, 15, 30]
    assert [r.parameters()["region.near.low"].value for _, r in sweep(look, "region.near.low", 0, 0.8, 3)] == [
        0,
        0.4,
        0.8,
    ]
    assert [r.parameters()["dust.amount"].value for _, r in sweep(look, "dust.amount", 0, 0.2, 3)] == [0, 0.1, 0.2]
    ends = sweep(look, "levels.output_black.end", 0.2, 0.6, 3)
    assert [r.parameters()["levels.output_black.end"].value for _, r in ends] == pytest.approx([0.2, 0.4, 0.6])
    assert labels(ends) == ["output_black.end=0.2", "output_black.end=0.4", "output_black.end=0.6"]
    assert labels(sweep(look, "frame.angle", 0, 30, 3)) == ["angle=0", "angle=15", "angle=30"]
    assert labels(sweep(look, "region.near.low", 0, 0.8, 3)) == ["near.low=0", "near.low=0.4", "near.low=0.8"]


def test_a_sweep_of_the_levels_of_a_drive_takes_whole_numbers():
    look = Recipe(
        [Step("levels", params={"output_black": Drive(by="near", start=0.0, end=0.2)})],
        name="look",
        size=48,
        regions={"near": Band(of="depth")},
    )
    tiles = sweep(look, "levels.output_black.levels", 2, 16, 4)
    assert labels(tiles) == [
        "output_black.levels=2",
        "output_black.levels=7",
        "output_black.levels=11",
        "output_black.levels=16",
    ]
