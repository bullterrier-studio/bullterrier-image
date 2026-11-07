"""The runner seen from Python: the facade's checks of maps, finders and pictures, effects written in Python run by
the core, and the two registries. What the core does with a recipe is tested in Rust (`crates/core/tests/it`)."""

from typing import Any

import numpy as np
import pytest

from bullterrier_image._native import _core  # noqa: PLC2701  architecture: the Rust core's runner is compared with the definitions
from bullterrier_image.core import bridge, native, registry
from bullterrier_image.core.params import Drive, EffectParams, param
from bullterrier_image.core.regions import Given, Grid
from bullterrier_image.core.registry import effect, unregister
from bullterrier_image.core.steps import Recipe, Step
from bullterrier_image.core.values import Lut, Picture
from bullterrier_image.errors import (
    DepthNotFoundError,
    InvalidImageError,
    MapNotFoundError,
    RecipeError,
    SubjectNotFoundError,
    UnknownEffectError,
)


def ramp(photo: np.ndarray) -> np.ndarray:
    """A map that rises from 0 on the left to 1 on the right, one value per pixel of `photo`."""
    height, width = photo.shape[:2]
    return np.tile(np.linspace(0, 1, width, dtype=np.float32), (height, 1))


def grade_one_stop(**step: object) -> Recipe:
    return Recipe([Step("grade", params={"exposure": 1}, **step)])  # type: ignore[arg-type]


def core_render(photo: np.ndarray, recipe: Recipe, **given: object) -> np.ndarray:
    return bridge.render(photo, recipe, **given)  # type: ignore[arg-type]


STILL = {"steps": [{"effect": "grade", "opacity": 0.0}]}  # a step that leaves the picture as it is
MASKED = {"steps": [{"effect": "grade", "opacity": 0.0, "mask": "subject"}]}  # and one that has the core hold a map


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
    expected = core_render(photo, recipe, maps={"sky": subject, "depth": ramp(photo)})
    np.testing.assert_array_equal(got, expected)


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


def test_a_finder_that_finds_nothing_has_the_error_of_its_map_with_its_words_come_back(photo):
    # "found nothing" (the package's not-found errors) is told to the core as such, because a subject found piece by
    # piece treats it as a piece with nothing in it: the error is rebuilt from its kind and text, not the same object
    class Fails:
        def find(self, photo: np.ndarray) -> np.ndarray:
            raise SubjectNotFoundError("nobody is in this picture")

    with pytest.raises(SubjectNotFoundError, match="nobody is in this picture") as caught:
        core_render(photo, grade_one_stop(mask="subject"), finders={"subject": Fails()})
    assert type(caught.value) is SubjectNotFoundError


def test_a_finder_that_raises_another_exception_has_that_very_exception_come_back(photo):
    raised = ZeroDivisionError("the model fell over")

    class Fails:
        def find(self, photo: np.ndarray) -> np.ndarray:
            raise raised

    with pytest.raises(ZeroDivisionError) as caught:
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


EMPTY_RECIPES = {"as it is": STILL, "turned": STILL | {"frame": {"angle": 30, "fit": "expand"}}}


@pytest.mark.parametrize("recipe", EMPTY_RECIPES.values(), ids=EMPTY_RECIPES.keys())
def test_a_picture_with_no_pixel_is_an_invalid_image_not_a_panic(recipe):
    empty = np.zeros((0, 5, 3), np.float32)
    for run in (native.render, native.trace):
        with pytest.raises(InvalidImageError, match="at least one"):
            run(empty, recipe)
    with pytest.raises(ValueError, match="at least one"):  # the extension's own error, a ValueError and no panic
        _core.render(empty, recipe)


PICTURE_FAULTS = {
    "a list": [[0.1, 0.2, 0.3]],
    "float64": np.zeros((2, 2, 3)),
    "two channels": np.zeros((2, 2, 2), np.float32),
    "not contiguous": np.zeros((2, 2, 3), np.float32)[:, ::2],
}


@pytest.mark.parametrize("rgb", PICTURE_FAULTS.values(), ids=PICTURE_FAULTS.keys())
def test_a_picture_that_is_not_a_contiguous_float32_array_of_rgb_is_refused(rgb):
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


def test_the_ratio_a_python_effect_sees_is_that_of_the_picture(reach_probes):
    recipe = {"steps": [{"effect": "reach_probe", "params": {"reach": 45}, "opacity": 1.0}]}
    photo = np.zeros((4, 960, 3), np.float32)  # twice the design size
    made = native.render(photo, recipe)
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


def test_a_python_effect_that_works_from_the_depth_asks_for_the_depth(photo, depth_probe):
    with pytest.raises(DepthNotFoundError):
        core_render(photo, Recipe([Step("depth_probe")]))


def test_a_python_effect_that_takes_a_partition_is_known_to_the_core_with_it(partition_probe, depth_probe):
    assert "partition" in _core.effect("partition_probe")["inputs"]
    assert "partition" not in _core.effect("depth_probe")["inputs"]
    assert _core.effect("depth_probe")["fields"] == {"field": "depth"}


def test_an_effect_that_takes_a_partition_is_given_the_pieces_of_the_region_it_names(photo, partition_probe):
    recipe = Recipe([Step("partition_probe", partition="panes", opacity=1)], regions={"panes": Grid(columns=2, rows=2)})
    made = core_render(photo, recipe)
    height, width = photo.shape[:2]
    # a pixel is in the second column when its centre is at or beyond the middle: a centre on the border is in the later
    row, column = (np.arange(height) + 0.5 >= height / 2), (np.arange(width) + 0.5 >= width / 2)
    piece = 2 * row[:, None].astype(int) + column[None, :].astype(int)
    np.testing.assert_allclose(made, np.repeat((piece / 255)[..., None], 3, axis=2), atol=1e-6)


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


def test_a_lut_in_a_step_reaches_its_effect(photo):
    inverting = Lut(1.0 - Lut.identity(2).table, label="inverting")
    made = core_render(photo, Recipe([Step("lut", params={"file": inverting}, opacity=1)]))
    np.testing.assert_allclose(made, 1.0 - photo, atol=1e-5)  # a table of 1 - colour, interpolated trilinearly
    identity = core_render(photo, Recipe([Step("lut", params={"file": Lut.identity(2)}, opacity=1)]))
    np.testing.assert_allclose(identity, photo, atol=1e-5)


class TableParams(EffectParams):
    """The parameters of an effect written in Python that takes a lookup table."""

    file: Lut = param(Lut.identity(), doc="The table.")


@pytest.fixture
def table_probe(throwaway_effects):
    """Registers `table_probe`, a Python effect with a Lut parameter: it fills the photo with the mean of its table's
    entries, so its output tells which table it received."""

    def table_probe(rgb: np.ndarray, params: TableParams) -> np.ndarray:
        """Fills the photo with the mean of a table."""
        return np.full_like(rgb, params.file.table.mean())

    return effect(category="utilities", params=TableParams)(table_probe)


def test_a_lut_in_a_step_reaches_a_python_effect_as_the_lut_and_each_step_gets_its_own(photo, table_probe):
    dark, light = Lut(np.full((2, 2, 2, 3), 0.25, np.float32)), Lut(np.full((2, 2, 2, 3), 0.75, np.float32))
    one = core_render(photo, Recipe([Step("table_probe", params={"file": dark}, opacity=1)]))
    np.testing.assert_allclose(one, 0.25, atol=1e-6)
    both = Recipe(
        [
            Step("table_probe", params={"file": dark}, opacity=1),
            Step("table_probe", params={"file": light}, opacity=0.5),
        ]
    )
    np.testing.assert_allclose(core_render(photo, both), 0.5, atol=1e-6)  # 0.25 with 0.75 laid over it at one half
    default = core_render(photo, Recipe([Step("table_probe", opacity=1)]))
    np.testing.assert_allclose(default, 0.5, atol=1e-6)  # the identity table's mean


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


def test_relighting_a_flat_field_changes_nothing_and_a_slope_does(photo):
    flat = {"depth": np.full(photo.shape[:2], 0.5, np.float32)}
    recipe = Recipe([Step("relight", opacity=1)])
    np.testing.assert_allclose(core_render(photo, recipe, maps=flat), photo, atol=1e-6)
    slope = {"depth": ramp(photo)}
    lit = core_render(photo, recipe, maps=slope)
    assert np.abs(lit - photo).max() > 0.05


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
    own = set(_core.effects(declared=True))
    assert {"grade", "pattern", "pixel_grid"} <= own
    everything = set(registry.all_effects())
    with registry_emptied():  # unregisters every effect, and registers them again afterwards
        assert set(_core.effects()) == own  # a core effect cannot be removed from the core, only from the registry
    assert set(_core.effects()) == everything == set(registry.all_effects())
