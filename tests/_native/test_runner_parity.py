"""The Rust core's runner (`_core.render`, `_core.trace`) against what the image should be.

Every row has an independent column: a pixel worked by hand, a numpy formula written from the definition (the W3C
compositing formulas, the Rec. 709 weights, a tent-weighted mix of two renders) or a property (outside the mask the
photo stays; opacity 0 is the photo). The Python engine (`core.runner`) is the second comparison only, within 1e-5, and
goes when the engine does. Errors are compared through the facade, which raises the package's exceptions.
"""

from collections.abc import Callable
from typing import Any

import numpy as np
import pytest

from bullterrier_image._native import _core  # noqa: PLC2701  architecture: the Rust core's runner is compared with Python's
from bullterrier_image.core import bridge, native, registry, runner
from bullterrier_image.core.params import Drive, EffectParams, Lut, Picture, design_scale, param
from bullterrier_image.core.presets import SHIPPED
from bullterrier_image.core.regions import Given, Grid
from bullterrier_image.core.registry import effect, unregister
from bullterrier_image.core.steps import Recipe, Step
from bullterrier_image.errors import (
    DepthNotFoundError,
    MapNotFoundError,
    RecipeError,
    SubjectNotFoundError,
    UnknownEffectError,
)

Row = tuple[np.ndarray, Recipe, dict[str, np.ndarray], np.ndarray]  # photo, recipe, maps, the picture it must make


def ramp(photo: np.ndarray) -> np.ndarray:
    """A map that rises from 0 on the left to 1 on the right, one value per pixel of `photo`."""
    height, width = photo.shape[:2]
    return np.tile(np.linspace(0, 1, width, dtype=np.float32), (height, 1))


def doubled(photo: np.ndarray) -> np.ndarray:
    """What `grade(exposure=1)` makes of `photo`: a stop doubles every value (the definition of a stop), up to white."""
    return np.clip(photo * 2.0, 0.0, 1.0)


def over(photo: np.ndarray, source: np.ndarray, alpha: np.ndarray | float) -> np.ndarray:
    """`source` laid over `photo` by `alpha` (the compositing equation for the normal blend mode)."""
    alpha = np.asarray(alpha, np.float64)
    alpha = alpha[..., None] if alpha.ndim == 2 else alpha
    return photo + alpha * (source - photo)


def grade_one_stop(**step: object) -> Recipe:
    return Recipe([Step("grade", params={"exposure": 1}, **step)])  # type: ignore[arg-type]


# --- the W3C compositing formulas, written from the specification (Compositing and Blending Level 1, 10.1 and 10.2) ---


def multiply(backdrop: np.ndarray, source: np.ndarray) -> np.ndarray:
    return backdrop * source


def screen(backdrop: np.ndarray, source: np.ndarray) -> np.ndarray:
    return backdrop + source - backdrop * source


def overlay(backdrop: np.ndarray, source: np.ndarray) -> np.ndarray:
    # hard light with the roles swapped: multiply below the backdrop's half, screen above
    return np.where(backdrop <= 0.5, source * 2 * backdrop, screen(source, 2 * backdrop - 1))


def soft_light(backdrop: np.ndarray, source: np.ndarray) -> np.ndarray:
    darker = ((16 * backdrop - 12) * backdrop + 4) * backdrop
    lift = np.where(backdrop <= 0.25, darker, np.sqrt(backdrop))
    return np.where(
        source <= 0.5,
        backdrop - (1 - 2 * source) * backdrop * (1 - backdrop),
        backdrop + (2 * source - 1) * (lift - backdrop),
    )


def difference(backdrop: np.ndarray, source: np.ndarray) -> np.ndarray:
    return np.abs(backdrop - source)


def color(backdrop: np.ndarray, source: np.ndarray) -> np.ndarray:
    # SetLum(Cs, Lum(Cb)) with the W3C weights; the rows keep it in gamut, where no ClipColor is needed
    weights = np.array([0.3, 0.59, 0.11])
    return source + ((backdrop - source) @ weights)[..., None]


SEPARABLE = {"multiply": multiply, "screen": screen, "overlay": overlay, "soft_light": soft_light}
SEPARABLE_AND_DIFFERENCE = {**SEPARABLE, "difference": difference}


# --- rows: the photo, the recipe, the maps handed in, and the picture the recipe must make -------------------------


def blend_row(mode: str, formula: Callable[[np.ndarray, np.ndarray], np.ndarray]) -> Callable[..., Row]:
    def build(photo: np.ndarray, subject: np.ndarray) -> Row:
        return photo, grade_one_stop(blend_mode=mode), {}, formula(photo, doubled(photo))

    return build


def a_stop_row(photo: np.ndarray, subject: np.ndarray) -> Row:
    return photo, grade_one_stop(), {}, doubled(photo)


def masked_row(photo: np.ndarray, subject: np.ndarray) -> Row:
    return photo, grade_one_stop(mask="subject"), {"subject": subject}, over(photo, doubled(photo), subject)


def background_row(photo: np.ndarray, subject: np.ndarray) -> Row:
    return photo, grade_one_stop(mask="background"), {"subject": subject}, over(photo, doubled(photo), 1 - subject)


def light_row(photo: np.ndarray, subject: np.ndarray) -> Row:
    light = photo @ np.array([0.2126, 0.7152, 0.0722])  # Rec. 709
    return photo, grade_one_stop(mask="light"), {}, over(photo, doubled(photo), light)


def opacity_row(photo: np.ndarray, subject: np.ndarray) -> Row:
    return photo, grade_one_stop(opacity=0.25), {}, over(photo, doubled(photo), 0.25)


def opacity_zero_row(photo: np.ndarray, subject: np.ndarray) -> Row:
    return photo, grade_one_stop(opacity=0), {}, photo


def multiply_masked_row(photo: np.ndarray, subject: np.ndarray) -> Row:
    recipe = grade_one_stop(blend_mode="multiply", mask="subject", opacity=0.5)
    blended = multiply(photo, doubled(photo))
    return photo, recipe, {"subject": subject}, over(photo, blended, 0.5 * subject)


def color_row(photo: np.ndarray, subject: np.ndarray) -> Row:
    base = (photo * 0.3 + 0.35).astype(np.float32)  # 0.35..0.65: shifting its brightness stays within 0..1
    recipe = Recipe([Step("grade", params={"exposure": 0.2}, blend_mode="color")])
    return base, recipe, {}, color(base, np.clip(base * 2**0.2, 0, 1))


def constant_drive_row(photo: np.ndarray, subject: np.ndarray) -> Row:
    # the depth 0.5 is level 2 of 5 exactly: the plain step at that level's value, 0 + (2 - 0) * 2 / 4 = 1 stop
    drive = Drive(by="depth", start=0, end=2, levels=5)
    depth = np.full(photo.shape[:2], 0.5, np.float32)
    return photo, Recipe([Step("grade", params={"exposure": drive})]), {"depth": depth}, doubled(photo)


def ramp_drive_row(photo: np.ndarray, subject: np.ndarray) -> Row:
    # three levels: 0, 1 and 2 stops; a pixel at depth d mixes the two levels either side of 2d, linearly
    drive = Drive(by="depth", start=0, end=2, levels=3)
    depth = ramp(photo)
    renders = np.stack([np.clip(photo * 2.0**stops, 0, 1) for stops in (0, 1, 2)])
    position = depth * 2
    low = np.minimum(np.floor(position), 1).astype(int)
    fraction = (position - low)[..., None]
    rows, columns = np.indices(depth.shape)
    expected = (1 - fraction) * renders[low, rows, columns] + fraction * renders[low + 1, rows, columns]
    return photo, Recipe([Step("grade", params={"exposure": drive})]), {"depth": depth}, expected


def two_steps_row(photo: np.ndarray, subject: np.ndarray) -> Row:
    recipe = Recipe([Step("grade", params={"exposure": 1}), Step("grade", params={"exposure": -1}, mask="subject")])
    first = doubled(photo)
    return photo, recipe, {"subject": subject}, over(first, np.clip(first * 0.5, 0, 1), subject)


ROWS: dict[str, Callable[..., Row]] = {
    "a core effect: one stop doubles every value": a_stop_row,
    "a mask limits the step to where it is and weights it by its value": masked_row,
    "the background is the subject's complement": background_row,
    "light is the picture's Rec. 709 brightness": light_row,
    "opacity mixes the result over the input": opacity_row,
    "opacity 0 is the photo": opacity_zero_row,
    "blend, opacity and mask together": multiply_masked_row,
    "the color blend takes the source's hue and keeps the backdrop's brightness": color_row,
    "a drive on a constant field is the plain step at that value": constant_drive_row,
    "a drive mixes the two levels either side of the field": ramp_drive_row,
    "each step runs on the output of the one before": two_steps_row,
    **{f"blend {mode}": blend_row(mode, formula) for mode, formula in SEPARABLE_AND_DIFFERENCE.items()},
}


def core_render(photo: np.ndarray, recipe: Recipe, **given: object) -> np.ndarray:
    return bridge.render(photo, recipe, **given)  # type: ignore[arg-type]


@pytest.mark.parametrize("row", ROWS.values(), ids=ROWS.keys())
def test_the_core_makes_the_picture_the_definition_says_and_the_python_engine_agrees(row, photo, subject):
    photo, recipe, maps, expected = row(photo, subject)
    made = core_render(photo, recipe, maps=maps)
    assert made.shape == photo.shape
    assert made.dtype == np.float32
    np.testing.assert_allclose(made, expected, atol=2e-6)
    np.testing.assert_allclose(made, runner.render(photo, recipe, maps=maps), atol=1e-5)


def test_outside_the_mask_the_photo_is_exactly_what_it_was(photo, subject):
    made = core_render(photo, grade_one_stop(mask="subject"), maps={"subject": subject})
    outside = subject == 0
    assert outside.any()
    assert (made[outside] == photo[outside]).all()
    assert not np.allclose(made[subject == 1], photo[subject == 1])  # and inside it the step did something


def test_the_input_photo_is_not_modified(photo, subject):
    before = photo.copy()
    core_render(photo, grade_one_stop(mask="subject", blend_mode="screen"), maps={"subject": subject})
    assert (photo == before).all()


# --- pixels worked by hand -----------------------------------------------------------------------------------------

PIXEL = np.array([[[0.1, 0.2, 0.3]]], np.float32)  # grade at one stop makes it [0.2, 0.4, 0.6]

HAND = {
    "one stop doubles": (grade_one_stop(), [0.2, 0.4, 0.6]),
    "multiply: 0.1 x 0.2, 0.2 x 0.4, 0.3 x 0.6": (grade_one_stop(blend_mode="multiply"), [0.02, 0.08, 0.18]),
    "screen: 0.1 + 0.2 - 0.02, 0.2 + 0.4 - 0.08, 0.3 + 0.6 - 0.18": (
        grade_one_stop(blend_mode="screen"),
        [0.28, 0.52, 0.72],
    ),
    "difference: |0.1 - 0.2|, |0.2 - 0.4|, |0.3 - 0.6|": (grade_one_stop(blend_mode="difference"), [0.1, 0.2, 0.3]),
    "overlay on a dark backdrop multiplies by twice the backdrop": (
        grade_one_stop(blend_mode="overlay"),
        [0.04, 0.16, 0.36],  # source x 2 x backdrop: 0.2 x 0.2, 0.4 x 0.4, 0.6 x 0.6
    ),
    "opacity 0.5 is halfway": (grade_one_stop(opacity=0.5), [0.15, 0.3, 0.45]),
    "opacity 0.25 is a quarter of the way": (grade_one_stop(opacity=0.25), [0.125, 0.25, 0.375]),
}


@pytest.mark.parametrize(("recipe", "pixel"), HAND.values(), ids=HAND.keys())
def test_a_pixel_is_what_working_it_by_hand_gives(recipe, pixel):
    np.testing.assert_allclose(core_render(PIXEL, recipe)[0, 0], pixel, atol=1e-6)


def test_a_mask_of_one_half_is_halfway_by_hand():
    half = np.full((1, 1), 0.5, np.float32)
    made = core_render(PIXEL, grade_one_stop(mask="subject"), maps={"subject": half})
    np.testing.assert_allclose(made[0, 0], [0.15, 0.3, 0.45], atol=1e-6)


# --- design pixels: the same look at any size ----------------------------------------------------------------------


def applied(effect: str, photo: np.ndarray, **working: object) -> np.ndarray:
    """The effect on `photo` with these values taken as working pixels, as the core's own `apply` runs it."""
    return _core.apply(effect, photo, _core.check(effect, dict(working)))


SCALING = {
    "a whole number of pixels is rounded": ("pixel_grid", {"cell": 12}, {"cell": 3}),  # 12 x 128 / 480 = 3.2
    "a whole number of pixels stops at its minimum": ("pixel_grid", {"cell": 8}, {"cell": 3}),  # 2.13, minimum 3
    "a pixel value is multiplied by the ratio": ("pixel_grid", {"cell": 30}, {"cell": 8}),  # 30 x 128 / 480
    "a decimal number of pixels is multiplied": (
        "pattern",
        {"cell": 48, "feather": 3},
        {"cell": 48 * 128 / 480, "feather": 3 * 128 / 480},  # 12.8 and 0.8
    ),
}


@pytest.mark.parametrize(("effect", "design", "working"), SCALING.values(), ids=SCALING.keys())
def test_pixel_values_are_written_for_480_and_scaled_to_the_photo(photo, effect, design, working):
    assert max(photo.shape[:2]) != 480  # the photo is not the design size, so a missing scaling shows
    recipe = Recipe([Step(effect, params=design, opacity=1)])
    expected = applied(effect, photo, **working)
    np.testing.assert_allclose(core_render(photo, recipe), expected, atol=1e-6)
    np.testing.assert_allclose(core_render(photo, recipe), runner.render(photo, recipe), atol=1e-5)


def test_the_ratio_is_the_callers_to_set():
    recipe = {"steps": [{"effect": "pixel_grid", "params": {"cell": 4}, "opacity": 1.0}]}
    photo = np.random.default_rng(3).random((40, 60, 3), dtype=np.float32)
    for ratio, cell in [(1.0, 4), (2.0, 8), (3.0, 12)]:
        np.testing.assert_allclose(
            native.render(photo, recipe, size_ratio=ratio), applied("pixel_grid", photo, cell=cell), atol=1e-6
        )
    assert design_scale((96, 128)) == 128 / 480  # what the facade passes for the photos of these tests


@pytest.mark.parametrize("ratio", [0, -1.0, float("nan"), float("inf")])
def test_a_ratio_that_is_no_positive_number_is_refused(ratio):
    with pytest.raises(ValueError, match="size_ratio"):
        _core.render(PIXEL, {"steps": [{"effect": "grade"}]}, size_ratio=ratio)


# --- presets ---------------------------------------------------------------------------------------------------------

HOT = "[hot]\ndescription = 'One stop up.'\nexposure = 1\n"


def preset_step(**step: object) -> dict:
    return {"steps": [{"effect": "grade", "preset": "hot", **step}]}


def test_a_step_gets_the_parameters_of_its_preset(photo):
    made = native.render(photo, preset_step(), presets={"grade": HOT})
    np.testing.assert_allclose(made, doubled(photo), atol=1e-6)


def test_the_steps_own_value_wins_over_its_preset(photo):
    made = native.render(photo, preset_step(params={"exposure": -1}), presets={"grade": HOT})
    np.testing.assert_allclose(made, np.clip(photo * 0.5, 0, 1), atol=1e-6)


def test_a_shipped_preset_renders_as_the_python_engine_renders_its_step(photo):
    name = next(iter(registry.presets("grade")))
    shipped = (SHIPPED / "grade.toml").read_text(encoding="utf-8")
    made = native.render(photo, {"steps": [{"effect": "grade", "preset": name}]}, presets={"grade": shipped})
    expected = runner.render(photo, Recipe([Step("grade", preset=name)]))
    np.testing.assert_allclose(made, expected, atol=1e-5)
    assert not np.allclose(made, photo)  # the preset is a look, not the identity


PRESET_FAULTS = {
    "a preset the effect lacks": (preset_step(preset="cold"), {"grade": HOT}, "hot"),
    "an effect with no presets given": (preset_step(), {}, "no presets"),
    "a preset with no description": (preset_step(), {"grade": "[hot]\nexposure = 1\n"}, "description"),
    "a preset that breaks a rule of the effect": (
        preset_step(),
        {"grade": "[hot]\ndescription = 'Too bright.'\nexposure = 11\n"},
        "exposure",
    ),
    "presets that are not TOML": (preset_step(), {"grade": "[hot\n"}, r"\w"),
}


@pytest.mark.parametrize(("recipe", "presets", "said"), PRESET_FAULTS.values(), ids=PRESET_FAULTS.keys())
def test_a_preset_that_is_missing_or_wrong_is_a_recipe_error(photo, recipe, presets, said):
    with pytest.raises(RecipeError, match=said):
        native.render(photo, recipe, presets=presets)


# --- maps: handed in, or found once by a finder --------------------------------------------------------------------


class Finds:
    """A finder that answers `answer` and keeps the photos it was asked about."""

    def __init__(self, answer: Any) -> None:
        self.answer: Any = answer
        self.asked: list[np.ndarray] = []

    def find(self, photo: np.ndarray) -> Any:
        self.asked.append(photo.copy())
        return self.answer


def test_a_map_found_by_a_finder_is_the_map_handed_in_and_the_finder_is_asked_once(photo, subject):
    recipe = Recipe([Step("grade", params={"exposure": 1}, mask="subject"), Step("grade", mask="background")])
    finder = Finds(subject)
    found = core_render(photo, recipe, finders={"subject": finder})
    np.testing.assert_array_equal(found, core_render(photo, recipe, maps={"subject": subject}))
    assert len(finder.asked) == 1  # two steps use the subject, one of them through the background
    np.testing.assert_array_equal(finder.asked[0], photo)  # the photo as given


def test_a_finder_is_not_asked_for_a_map_that_was_handed_in_nor_for_one_nothing_uses(photo, subject):
    asked = Finds(subject)
    core_render(photo, grade_one_stop(mask="subject"), maps={"subject": subject}, finders={"subject": asked})
    core_render(photo, grade_one_stop(), finders={"subject": asked})
    assert asked.asked == []


def test_a_finder_may_answer_in_any_float_type(photo, subject):
    recipe = grade_one_stop(mask="subject")
    wide = core_render(photo, recipe, finders={"subject": Finds(subject.astype(np.float64))})
    np.testing.assert_allclose(wide, core_render(photo, recipe, maps={"subject": subject}), atol=1e-6)


def test_the_core_finds_the_depth_and_a_given_region_by_their_finders_too(photo, subject):
    recipe = Recipe(
        [
            Step("grade", params={"exposure": 1}, mask="sky"),
            Step("grade", params={"exposure": Drive(by="depth", start=0, end=1)}),
        ],
        regions={"sky": Given()},
    )
    got = core_render(photo, recipe, finders={"sky": Finds(subject), "depth": Finds(ramp(photo))})
    expected = runner.render(photo, recipe, maps={"sky": subject, "depth": ramp(photo)})
    np.testing.assert_allclose(got, expected, atol=1e-5)


MISSING = {
    "the subject": (grade_one_stop(mask="subject"), SubjectNotFoundError, "subject"),
    "the subject, through the background": (grade_one_stop(mask="background"), SubjectNotFoundError, "subject"),
    "the depth": (
        Recipe([Step("grade", params={"exposure": Drive(by="depth", start=0, end=1)})]),
        DepthNotFoundError,
        "depth",
    ),
    "a given region": (
        Recipe([Step("grade", mask="sky")], regions={"sky": Given()}),
        MapNotFoundError,
        "sky",
    ),
}


@pytest.mark.parametrize(("recipe", "error", "name"), MISSING.values(), ids=MISSING.keys())
def test_a_map_nobody_gave_is_the_error_of_that_map(photo, recipe, error, name):
    with pytest.raises(error, match=name) as caught:
        core_render(photo, recipe)
    assert type(caught.value) is error
    with pytest.raises(error):  # and so does Python's engine
        runner.render(photo, recipe)


def bad_maps(photo: np.ndarray) -> dict[str, np.ndarray]:
    height, width = photo.shape[:2]
    return {
        "of another size": np.zeros((height, width + 1), np.float32),
        "above 1": np.full((height, width), 1.5, np.float32),
        "below 0": np.full((height, width), -0.5, np.float32),
        "not a number": np.full((height, width), np.nan, np.float32),
    }


@pytest.mark.parametrize("problem", ["of another size", "above 1", "below 0", "not a number"])
@pytest.mark.parametrize(
    ("name", "error"), [("subject", SubjectNotFoundError), ("depth", DepthNotFoundError), ("sky", MapNotFoundError)]
)
def test_a_map_that_does_not_fit_is_the_error_of_that_map_whether_handed_in_or_found(photo, name, error, problem):
    recipe = Recipe([Step("grade", mask=name if name != "depth" else "depth")], regions={"sky": Given()})
    map_ = bad_maps(photo)[problem]
    with pytest.raises(error, match=name):
        core_render(photo, recipe, maps={name: map_})
    with pytest.raises(error, match=name):
        core_render(photo, recipe, finders={name: Finds(map_)})


def test_a_map_handed_in_that_nothing_uses_is_still_held_to_the_rules(photo, subject):
    with pytest.raises(SubjectNotFoundError, match="subject"):
        core_render(photo, grade_one_stop(), maps={"subject": bad_maps(photo)["above 1"]})


@pytest.mark.parametrize("where", ["maps", "finders"])
def test_a_name_that_is_no_map_of_the_recipe_is_refused_listing_the_maps_there_are(photo, subject, where):
    given = {"sky": subject if where == "maps" else Finds(subject)}
    with pytest.raises(RecipeError, match=r"sky.*depth.*subject|sky.*subject"):
        core_render(photo, grade_one_stop(), **{where: given})


def test_a_finder_that_answers_none_is_the_error_of_its_map_saying_what_it_must_do(photo):
    with pytest.raises(DepthNotFoundError, match="None") as caught:
        core_render(photo, grade_one_stop(mask="depth"), finders={"depth": Finds(None)})
    assert "raise" in str(caught.value)


def test_a_finder_that_answers_something_that_is_no_map_is_the_error_of_its_map(photo):
    with pytest.raises(SubjectNotFoundError, match="subject"):
        core_render(photo, grade_one_stop(mask="subject"), finders={"subject": Finds("a subject")})


def test_a_finder_that_raises_has_its_own_exception_come_back_as_it_was(photo):
    raised = SubjectNotFoundError("nobody is in this picture")

    class Fails:
        def find(self, photo: np.ndarray) -> np.ndarray:
            raise raised

    with pytest.raises(SubjectNotFoundError) as caught:
        core_render(photo, grade_one_stop(mask="subject"), finders={"subject": Fails()})
    assert caught.value is raised


def test_a_finder_that_fails_in_some_other_way_is_not_turned_into_a_missing_map(photo):
    class Breaks:
        def find(self, photo: np.ndarray) -> np.ndarray:
            raise ZeroDivisionError("the model fell over")

    with pytest.raises(ZeroDivisionError, match="fell over"):
        core_render(photo, grade_one_stop(mask="subject"), finders={"subject": Breaks()})


def test_a_finder_must_have_a_find(photo):
    recipe = {"steps": [{"effect": "grade", "mask": "subject"}]}
    with pytest.raises(ValueError, match="find"):
        _core.render(photo, recipe, finders={"subject": object()})  # type: ignore[dict-item]  # the fault under test


# --- trace ----------------------------------------------------------------------------------------------------------


def kind_of(label: str) -> str:
    """The kind of a stage of the Python engine's trace, from its label."""
    if label.startswith("input"):
        return "input"
    if label.startswith("subject"):
        return "subject"
    if label.startswith("depth"):
        return "depth"
    assert label[0].isdigit(), label
    return "step"


def test_a_trace_shows_the_photo_the_maps_used_and_the_picture_after_each_step(photo, subject):
    recipe = Recipe(
        [
            Step("grade", params={"exposure": 1}, mask="subject"),
            Step("grade", params={"exposure": Drive(by="depth", start=0, end=1)}, blend_mode="screen"),
        ]
    )
    maps = {"subject": subject, "depth": ramp(photo)}
    stages = bridge.trace(photo, recipe, maps=maps)
    python = runner.trace(photo, recipe, maps=maps)
    assert (
        [kind for kind, _, _ in stages]
        == [kind_of(stage.label) for stage in python]
        == [
            "input",
            "subject",
            "depth",
            "step",
            "step",
        ]
    )
    assert [number for _, number, _ in stages] == [None, None, None, 1, 2]
    for (kind, _, array), stage in zip(stages, python, strict=True):
        assert array.dtype == np.float32
        shown = stage.image[..., 0] if kind in {"subject", "depth"} else stage.image  # Python shows a map as grey
        np.testing.assert_allclose(array, shown, atol=1e-5)
    assert stages[0][2].shape == photo.shape
    assert stages[1][2].shape == photo.shape[:2]  # a map is (h, w)


def test_the_last_stage_of_a_trace_is_exactly_what_render_returns(photo, subject):
    recipe = Recipe([Step("grade", params={"exposure": 1}), Step("grade", mask="subject", blend_mode="multiply")])
    stages = bridge.trace(photo, recipe, maps={"subject": subject})
    np.testing.assert_array_equal(stages[-1][2], core_render(photo, recipe, maps={"subject": subject}))


def test_a_trace_fails_where_a_render_does(photo):
    with pytest.raises(SubjectNotFoundError):
        bridge.trace(photo, grade_one_stop(mask="subject"))


# --- the recipe as the core reads it -------------------------------------------------------------------------------


def test_the_recipe_the_core_runs_is_the_one_read_from_a_file(photo):
    text = "[[step]]\neffect = 'grade'\nexposure = 1\nopacity = 0.5\nblend_mode = 'screen'\n"
    read = _core.read_recipe(text, "toml")
    made = native.render(photo, read)
    np.testing.assert_allclose(made, over(photo, screen(photo, doubled(photo)), 0.5), atol=2e-6)


RECIPE_FAULTS = {
    "an unknown effect": ({"steps": [{"effect": "no_such_effect"}]}, "no_such_effect"),
    "a value out of range": ({"steps": [{"effect": "grade", "params": {"exposure": 11}}]}, "exposure"),
    "no steps": ({"steps": []}, "step"),
    "a mask no region declares": ({"steps": [{"effect": "grade", "mask": "sky"}]}, "sky"),
    "a key no recipe has": ({"steps": [{"effect": "grade"}], "colour": "red"}, "colour"),
    "a framing, which the core does not run yet": ({"steps": [{"effect": "grade"}], "frame": {"flip": "x"}}, "frame"),
}


@pytest.mark.parametrize(("recipe", "said"), RECIPE_FAULTS.values(), ids=RECIPE_FAULTS.keys())
def test_a_recipe_that_breaks_a_rule_is_a_recipe_error_before_anything_runs(photo, recipe, said):
    with pytest.raises(RecipeError, match=said):
        native.render(photo, recipe)
    with pytest.raises(RecipeError, match=said):
        native.trace(photo, recipe)


PICTURE_FAULTS = {
    "a list": [[0.1, 0.2, 0.3]],
    "float64": np.zeros((2, 2, 3)),
    "two channels": np.zeros((2, 2, 2), np.float32),
    "not contiguous": np.zeros((2, 2, 3), np.float32)[:, ::2],
}


@pytest.mark.parametrize("rgb", PICTURE_FAULTS.values(), ids=PICTURE_FAULTS.keys())
def test_a_picture_that_is_not_a_contiguous_float32_array_of_rgb_is_refused(rgb):
    from bullterrier_image.errors import InvalidImageError

    with pytest.raises(InvalidImageError, match="float32"):
        native.render(rgb, {"steps": [{"effect": "grade"}]})


# --- effects written in Python: the registry mirrors them into the core, which runs them like its own --------------


class ReachParams(EffectParams):
    """What the reach probes encode."""

    reach: float = param(30.0, unit="px", min=0, max=500, doc="Written into the picture, as the core hands it over.")


@pytest.fixture
def reach_probes(throwaway_effects):
    """`reach_probe`, which declares `size_ratio`, and `plain_probe`, which does not. Both write their `reach` (in the
    working px they are handed: 0.01 per px) into the picture, `reach_probe` also its `size_ratio` (0.5 per unit)."""

    def reach_probe(rgb: np.ndarray, params: ReachParams, *, size_ratio: float = 1.0) -> np.ndarray:
        """Writes its reach and the size ratio it was given into the picture."""
        out = np.full(rgb.shape, 0.5, np.float32)
        out[..., 0], out[..., 1] = params.reach / 100, size_ratio / 2
        return out

    def plain_probe(rgb: np.ndarray, params: ReachParams) -> np.ndarray:
        """Writes its reach into the picture."""
        return np.full(rgb.shape, params.reach / 100, np.float32)

    effect(category="utilities", params=ReachParams)(reach_probe)
    effect(category="light", params=ReachParams, opacity=0.5)(plain_probe)


def reach_recipe(effect_name: str, reach: float = 45) -> Recipe:
    return Recipe([Step(effect_name, params={"reach": reach}, opacity=1)])


def test_a_python_effect_is_scaled_by_its_wrapper_and_told_the_ratio_only_if_it_declares_it(photo, reach_probes):
    ratio = 128 / 480
    made = core_render(photo, reach_recipe("reach_probe"))
    np.testing.assert_allclose(made[..., 0], 0.12, atol=1e-7)  # 45 x 128 / 480 = 12 px working, by hand
    np.testing.assert_allclose(made[..., 1], ratio / 2, atol=1e-7)  # the ratio the effect declared
    np.testing.assert_allclose(made[..., 2], 0.5, atol=1e-7)
    plain = core_render(photo, reach_recipe("plain_probe"))  # a function with no size_ratio would fail if given one
    np.testing.assert_allclose(plain, 0.12, atol=1e-7)
    for name in ("reach_probe", "plain_probe"):
        np.testing.assert_allclose(core_render(photo, reach_recipe(name)), runner.render(photo, reach_recipe(name)))


def test_the_ratio_a_python_effect_sees_is_the_callers_to_set(reach_probes):
    recipe = {"steps": [{"effect": "reach_probe", "params": {"reach": 45}, "opacity": 1.0}]}
    photo = np.zeros((4, 6, 3), np.float32)
    made = native.render(photo, recipe, size_ratio=2.0)
    np.testing.assert_allclose(made[0, 0], [0.9, 1.0, 0.5], atol=1e-7)  # 45 x 2 = 90 px, ratio 2 / 2


def test_a_python_effect_has_the_default_opacity_it_was_registered_with(photo, reach_probes):
    recipe = Recipe([Step("plain_probe", params={"reach": 45})])  # opacity 0.5 by registration
    made = core_render(photo, recipe)
    np.testing.assert_allclose(made, photo + 0.5 * (0.12 - photo), atol=1e-6)


def test_a_python_effect_given_a_misspelt_parameter_in_a_recipe_file_is_a_recipe_error(photo, reach_probes):
    recipe = {"steps": [{"effect": "reach_probe", "params": {"reach": 501}}]}
    with pytest.raises(RecipeError, match="reach"):
        native.render(photo, recipe)


FAULTS = {
    "a recipe error": RecipeError("the effect's own complaint"),
    "a subject that is not there": SubjectNotFoundError("nobody is in this picture"),
    "a plain error": ZeroDivisionError("the effect divided by nothing"),
}


@pytest.mark.parametrize("raised", FAULTS.values(), ids=FAULTS.keys())
def test_an_exception_a_python_effect_raises_comes_back_as_it_was(photo, throwaway_effects, raised):
    def failing(rgb: np.ndarray, params: ReachParams) -> np.ndarray:
        """Raises."""
        raise raised

    effect(category="utilities", params=ReachParams)(failing)
    with pytest.raises(type(raised)) as caught:
        core_render(photo, reach_recipe("failing"))
    assert caught.value is raised


class DepthParams(ReachParams):
    """The reach, and a field to paint."""

    field: str = param("depth", doc="The field painted.")


FIELDS = {
    "the depth, by default": ({}, lambda photo, subject, depth: depth),
    "the light, by name": (
        {"field": "light"},
        lambda photo, subject, depth: photo @ np.array([0.2126, 0.7152, 0.0722]),
    ),
    "the subject, by name": ({"field": "subject"}, lambda photo, subject, depth: subject),
    "the background, by name": ({"field": "background"}, lambda photo, subject, depth: 1 - subject),
}


@pytest.mark.parametrize(("params", "painted"), FIELDS.values(), ids=FIELDS.keys())
def test_a_python_effect_with_a_field_receives_the_map_its_step_names_or_its_default(
    photo, subject, depth_probe, params, painted
):
    depth = ramp(photo)
    recipe = Recipe([Step("depth_probe", params=params, opacity=1)])
    maps = {"subject": subject, "depth": depth}
    made = core_render(photo, recipe, maps=maps)
    np.testing.assert_allclose(made, np.repeat(painted(photo, subject, depth)[..., None], 3, axis=2), atol=1e-6)
    np.testing.assert_allclose(made, runner.render(photo, recipe, maps=maps), atol=1e-6)


def test_a_python_effect_that_works_from_the_depth_asks_for_the_depth(photo, depth_probe):
    with pytest.raises(DepthNotFoundError):
        core_render(photo, Recipe([Step("depth_probe")]))


def test_a_python_effect_that_takes_a_partition_is_known_to_the_core_with_it(partition_probe, depth_probe):
    assert "partition" in _core.effect("partition_probe")["inputs"]
    assert "partition" not in _core.effect("depth_probe")["inputs"]
    assert _core.effect("depth_probe")["fields"] == {"field": "depth"}


def test_an_effect_that_takes_a_partition_is_refused_until_the_core_runs_the_regions_that_make_one(
    photo, partition_probe
):
    recipe = Recipe([Step("partition_probe", partition="panes")], regions={"panes": Grid()})
    with pytest.raises(ValueError, match="partition"):  # the core has no partition to give it yet
        core_render(photo, recipe)


# --- a Picture or a Lut cannot be written in a recipe: the call holds it -------------------------------------------


def test_a_picture_in_a_step_reaches_its_effect(photo, layer_probe):
    recipe = Recipe([Step("layer_probe", params={"image": Picture.of_color("#336699")}, opacity=1)])
    made = core_render(photo, recipe)
    np.testing.assert_allclose(made, np.broadcast_to([0.2, 0.4, 0.6], photo.shape), atol=1e-6)  # 0x33, 0x66, 0x99 / 255


def test_two_pictures_in_two_steps_reach_their_own_steps(photo, layer_probe):
    first = Step("layer_probe", params={"image": Picture.of_color("#336699")}, opacity=1)
    second = Step("layer_probe", params={"image": Picture.of_color("#ff0000")}, opacity=0.5)
    made = core_render(photo, Recipe([first, second]))
    # (0.2, 0.4, 0.6) with (1, 0, 0) laid over it at one half
    np.testing.assert_allclose(made, np.broadcast_to([0.6, 0.2, 0.3], photo.shape), atol=1e-6)
    np.testing.assert_allclose(made, runner.render(photo, Recipe([first, second])), atol=1e-6)


def test_a_lut_in_a_step_reaches_its_effect(photo):
    inverting = Lut(1.0 - Lut.identity(2).table, label="inverting")
    made = core_render(photo, Recipe([Step("lut", params={"file": inverting}, opacity=1)]))
    np.testing.assert_allclose(made, 1.0 - photo, atol=1e-5)  # a table of 1 - colour, interpolated trilinearly
    identity = core_render(photo, Recipe([Step("lut", params={"file": Lut.identity(2)}, opacity=1)]))
    np.testing.assert_allclose(identity, photo, atol=1e-5)


# --- real effects written in Python --------------------------------------------------------------------------------


def test_the_gradient_runs_from_its_first_colour_to_its_last_across_the_picture(photo):
    recipe = Recipe(
        [Step("gradient", params={"kind": "linear", "colors": ["#000000", "#ffffff"], "angle": 0}, opacity=1)]
    )
    made = core_render(photo, recipe)
    np.testing.assert_allclose(made[:, :1], 0.0, atol=0.02)  # the left edge is the first colour, the right the last
    np.testing.assert_allclose(made[:, -1:], 1.0, atol=0.02)
    assert (np.diff(made[0, :, 0]) >= -1e-6).all()  # and it never goes back
    np.testing.assert_allclose(made, made[:1].repeat(photo.shape[0], axis=0), atol=1e-6)  # the same on every row
    np.testing.assert_allclose(made, runner.render(photo, recipe), atol=1e-5)


def test_relighting_a_flat_field_changes_nothing_and_a_slope_does(photo):
    flat = {"depth": np.full(photo.shape[:2], 0.5, np.float32)}
    recipe = Recipe([Step("relight", opacity=1)])
    np.testing.assert_allclose(core_render(photo, recipe, maps=flat), photo, atol=1e-6)
    slope = {"depth": ramp(photo)}
    lit = core_render(photo, recipe, maps=slope)
    assert np.abs(lit - photo).max() > 0.05
    np.testing.assert_allclose(lit, runner.render(photo, recipe, maps=slope), atol=1e-5)


# --- the two registries are one list -------------------------------------------------------------------------------


def test_the_core_knows_exactly_the_effects_the_registry_knows():
    assert set(_core.effects()) == set(registry.all_effects())


def test_an_effect_registered_or_removed_in_python_is_registered_or_removed_in_the_core(reach_probes):
    assert _core.effect("reach_probe")["category"] == "utilities"
    plain = _core.effect("plain_probe")
    assert (plain["category"], plain["opacity"]) == ("light", 0.5)
    unregister("plain_probe")
    assert "plain_probe" not in _core.effects()
    assert "reach_probe" in _core.effects()
    with pytest.raises(UnknownEffectError, match="plain_probe"):  # and the registry has forgotten it too
        registry.get("plain_probe")


def test_registering_an_effect_again_with_another_category_and_opacity_replaces_it_in_the_core(reach_probes):
    effect(category="light", params=ReachParams, opacity=0.25)(registry.get("reach_probe"))
    changed = _core.effect("reach_probe")
    assert (changed["category"], changed["opacity"]) == ("light", 0.25)


def test_emptying_the_registry_empties_the_core_of_python_effects_but_not_of_its_own(registry_emptied):
    own = {name for name in _core.effects() if name in {"grade", "pattern", "pixel_grid"}}
    assert own == {"grade", "pattern", "pixel_grid"}
    everything = set(registry.all_effects())
    with registry_emptied():  # unregisters every effect, and registers them again afterwards
        assert set(_core.effects()) == own  # a core effect cannot be removed from the core, only from the registry
    assert set(_core.effects()) == everything == set(registry.all_effects())
