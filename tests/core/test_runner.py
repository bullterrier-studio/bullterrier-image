"""Tests of `core/runner.py`: the runner is given the subject (a subject, or a `Finder` that finds it); it never looks
for the subject itself."""

import math
import tracemalloc
from contextlib import nullcontext

import numpy as np
import pytest

from bullterrier_image import DepthNotFoundError, MapNotFoundError, RecipeError, SubjectNotFoundError
from bullterrier_image.core import runner
from bullterrier_image.core.framing import Framing
from bullterrier_image.core.params import Drive, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.core.steps import Each, Recipe, Step
from bullterrier_image.ops.resample import resize
from bullterrier_image.recipe import Band, Bands, Combine, Finder, Given, Grid, Pick, Shards, Subject

MASKED = Recipe("masked", (Step("gradient", {"colors": ["#000000"]}, "background"),), size=None)
PLAIN = Recipe("plain", (Step("grade", {"exposure": 0.5}),), size=None)


class CountingFinder:
    """A stand-in subject finder: records the photos it was asked about and returns a fixed subject."""

    def __init__(self, subject: np.ndarray):
        self.subject = subject
        self.seen: list[tuple[int, ...]] = []

    def __call__(self, rgb: np.ndarray) -> np.ndarray:
        self.seen.append(rgb.shape)
        return self.subject


@pytest.mark.parametrize(
    ("identity", "find", "outcome"),
    [
        pytest.param("stand-in v1", np.mean, nullcontext(), id="a named function is a finder"),
        pytest.param("", np.mean, pytest.raises(RecipeError, match="identity"), id="no identity is refused"),
        pytest.param(" \t", np.mean, pytest.raises(RecipeError, match="identity"), id="a blank identity is refused"),
        pytest.param(None, np.mean, pytest.raises(RecipeError, match="identity"), id="an identity not text is refused"),
        pytest.param(
            "stand-in v1", 3, pytest.raises(RecipeError, match="find must"), id="a find not callable is refused"
        ),
    ],
)
def test_a_finder_needs_an_identity_and_a_function(identity, find, outcome):
    with outcome:
        Finder(identity, find)


RUNS = {
    "render": lambda photo, recipe, **k: runner.render(photo, recipe, **k),
    "trace": lambda photo, recipe, **k: runner.trace(photo, recipe, **k),
    "render_all": lambda photo, recipe, **k: runner.render_all(photo, [PLAIN, recipe], **k),
}


@pytest.mark.parametrize("run", RUNS.values(), ids=RUNS.keys())
def test_a_masked_recipe_without_the_subject_map_or_a_finder_is_refused_naming_both_ways(run, photo):
    with pytest.raises(SubjectNotFoundError, match="needs the subject") as raised:
        run(photo, MASKED)
    message = str(raised.value)  # names the recipe and both ways to give the subject
    assert f"'{MASKED.name}'" in message
    assert "maps={'subject'" in message
    assert "finders={'subject'" in message


@pytest.mark.parametrize("run", RUNS.values(), ids=RUNS.keys())
def test_the_finder_is_called_exactly_once_per_render(run, photo, subject):
    finder = CountingFinder(subject)
    run(photo, MASKED, finders={"subject": Finder("stand-in", finder)})
    assert len(finder.seen) == 1  # once for a render, a trace, or all the recipes of render_all


def test_render_all_asks_once_however_many_recipes_need_the_subject(photo, subject):
    finder = CountingFinder(subject)
    assert len(runner.render_all(photo, [MASKED, PLAIN, MASKED], finders={"subject": Finder("stand-in", finder)})) == 3
    assert len(finder.seen) == 1


def test_the_finder_is_not_called_when_nothing_needs_the_subject_or_the_matte_is_given(photo, subject):
    finder = CountingFinder(subject)
    runner.render(photo, PLAIN, finders={"subject": Finder("stand-in", finder)})
    runner.render_all(photo, [PLAIN, PLAIN], finders={"subject": Finder("stand-in", finder)})
    runner.render(photo, MASKED, maps={"subject": subject}, finders={"subject": Finder("stand-in", finder)})
    assert finder.seen == []


def test_the_finders_matte_is_the_one_the_recipe_uses(photo, subject):
    found = runner.render(photo, MASKED, finders={"subject": Finder("stand-in", CountingFinder(subject))})
    np.testing.assert_array_equal(found, runner.render(photo, MASKED, maps={"subject": subject}))
    assert not np.array_equal(found, runner.render(photo, MASKED, maps={"subject": 1.0 - subject}))


def test_the_finder_sees_the_photo_as_given_before_it_is_framed_or_scaled(photo, subject):
    finder = CountingFinder(subject)
    small = Recipe("small", MASKED.steps, size=64, frame=Framing(aspect=1.0))
    assert runner.render(photo, small, finders={"subject": Finder("stand-in", finder)}).shape == (64, 64, 3)
    assert finder.seen == [(96, 128, 3)]  # not the 64 px square it is rendered as


WITH_SUBJECT = {
    "a given map": lambda subject: {"maps": {"subject": subject}},
    "a finder": lambda subject: {"finders": {"subject": Finder("stand-in", CountingFinder(subject))}},
}


@pytest.mark.parametrize("given", WITH_SUBJECT.values(), ids=WITH_SUBJECT.keys())
def test_a_matte_of_another_size_than_the_photo_is_refused_naming_both_sizes(photo, subject, given):
    with pytest.raises(SubjectNotFoundError, match=r"subject is \(48, 64\).*\(96, 128\)"):
        runner.render(photo, MASKED, **given(subject[::2, ::2]))


@pytest.mark.parametrize("run", RUNS.values(), ids=RUNS.keys())
def test_a_finder_that_finds_nothing_is_an_error_naming_it(run, photo):
    """A finder returning None (instead of raising) would otherwise make `trace` drop the subject stage silently."""
    broken = Finder("stand-in", lambda rgb: None)  # type: ignore[arg-type,return-value]  # broken on purpose
    with pytest.raises(SubjectNotFoundError, match="finder of 'subject' returned None"):
        run(photo, MASKED, finders={"subject": broken})


# --- depth: a second map, given or found like the subject ------------------------------------------------------------

RAMP = np.arange(96 * 128, dtype=np.float32).reshape(96, 128) / (96 * 128 - 1)  # every pixel different: a move shows
WITH_DEPTH = {
    "a given map": lambda depth: {"maps": {"depth": depth}},
    "a finder": lambda depth: {"finders": {"depth": Finder("stand-in depth", CountingFinder(depth))}},
}


def deep(**keys) -> Recipe:
    """A recipe of one step of `depth_probe`, which paints the depth map it is given (needs the `depth_probe`)."""
    return Recipe("deep", [Step("depth_probe")], **{"size": None, **keys})


FRONT = Recipe(  # paints red only the front quarter of the depth scale
    "front",
    [Step("gradient", {"colors": ["#ff0000"]}, mask="near")],
    size=None,
    regions={"near": Band(of="depth", low=0.75, feather=0)},
)
# Nearest at the top, farthest at the bottom, falling evenly down the 96 rows of the photo.
DOWN = np.tile(1 - (np.arange(96, dtype=np.float32)[:, None] + 0.5) / 96, (1, 128))
NEEDING_DEPTH = {  # a recipe that needs depth, and what the error for no depth names
    "an effect that takes it": (deep, "depth_probe"),
    "a region made from it": (lambda: FRONT, "'near'"),
}


@pytest.mark.parametrize("given", WITH_DEPTH.values(), ids=WITH_DEPTH.keys())
def test_an_effect_that_takes_depth_is_given_the_depth_map(photo, depth_probe, given):
    np.testing.assert_array_equal(runner.render(photo, deep(), **given(RAMP))[..., 0], RAMP)


@pytest.mark.parametrize(
    "run",
    [*RUNS.values(), lambda photo, recipe, **k: runner.render_all(photo, [recipe, PLAIN, recipe], **k)],
    ids=[*RUNS.keys(), "render_all of two recipes that need it"],
)
def test_the_depth_finder_is_asked_once_with_the_photo_as_given(run, photo, depth_probe):
    finder = CountingFinder(RAMP)
    run(photo, deep(size=64, frame=Framing(aspect=1.0)), finders={"depth": Finder("stand-in depth", finder)})
    assert finder.seen == [(96, 128, 3)]


def test_the_depth_finder_is_not_asked_when_nothing_needs_depth(photo, subject, depth_probe):
    finder = CountingFinder(RAMP)
    asked = Finder("stand-in depth", finder)
    runner.render(photo, PLAIN, finders={"depth": asked})
    runner.trace(photo, MASKED, maps={"subject": subject}, finders={"depth": asked})
    runner.render_all(photo, [PLAIN, MASKED], maps={"subject": subject}, finders={"depth": asked})
    runner.render(photo, deep(), maps={"depth": RAMP}, finders={"depth": asked})
    assert finder.seen == []


@pytest.mark.parametrize(("recipe", "named"), NEEDING_DEPTH.values(), ids=NEEDING_DEPTH.keys())
@pytest.mark.parametrize("run", RUNS.values(), ids=RUNS.keys())
def test_a_recipe_that_needs_depth_without_a_map_or_a_finder_is_refused_naming_the_step_and_both_ways(
    run, photo, depth_probe, recipe, named
):
    with pytest.raises(DepthNotFoundError, match="step 1") as raised:
        run(photo, recipe())
    message = str(raised.value)
    assert named in message
    assert "maps={'depth'" in message
    assert "finders={'depth'" in message


@pytest.mark.parametrize("given", WITH_DEPTH.values(), ids=WITH_DEPTH.keys())
@pytest.mark.parametrize(
    ("wrong", "size"),
    [
        pytest.param(RAMP[::2, ::2], r"\(48, 64\)", id="half the height and width"),
        pytest.param(RAMP[..., None], r"\(96, 128, 1\)", id="the photo's height and width with a channel"),
    ],
)
@pytest.mark.parametrize("recipe", [needing for needing, _ in NEEDING_DEPTH.values()], ids=NEEDING_DEPTH.keys())
def test_a_depth_map_of_another_size_than_the_photo_is_refused_naming_both_sizes(
    photo, depth_probe, given, wrong, size, recipe
):
    with pytest.raises(DepthNotFoundError, match=size + r".*\(96, 128\)"):
        runner.render(photo, recipe(), **given(wrong))


def spoilt(value: float) -> np.ndarray:
    """`RAMP` (exactly 0 to exactly 1) with one pixel set to `value`."""
    layer = RAMP.copy()
    layer[5, 7] = value
    return layer


MAP_VALUES = [
    pytest.param(RAMP, None, id="a ramp from exactly 0 to exactly 1 is fine"),
    pytest.param(spoilt(np.nan), "not finite", id="nan is refused"),
    pytest.param(spoilt(np.inf), "not finite", id="infinity is refused"),
    pytest.param(spoilt(-0.3), r"below 0.*min -0\.3", id="a value below 0 is refused"),
    pytest.param(spoilt(1.2), r"above 1.*max 1\.2", id="a value above 1 is refused"),
]
EITHER_MAP = {  # a recipe needing the map, how it is given or found, and its error
    "the subject, given": (MASKED, WITH_SUBJECT["a given map"], SubjectNotFoundError),
    "the subject, found": (MASKED, WITH_SUBJECT["a finder"], SubjectNotFoundError),
    "the depth, given": (FRONT, WITH_DEPTH["a given map"], DepthNotFoundError),
    "the depth, found": (FRONT, WITH_DEPTH["a finder"], DepthNotFoundError),
}


@pytest.mark.parametrize(("layer", "complaint"), MAP_VALUES)
@pytest.mark.parametrize(("recipe", "given", "error"), EITHER_MAP.values(), ids=EITHER_MAP.keys())
def test_a_map_must_be_finite_and_within_0_and_1(photo, layer, complaint, recipe, given, error):
    if complaint is None:
        assert runner.render(photo, recipe, **given(layer)).shape == photo.shape
        return
    with pytest.raises(error, match=r"^(subject|depth) has .*" + complaint):
        runner.render(photo, recipe, **given(layer))


@pytest.mark.parametrize("run", RUNS.values(), ids=RUNS.keys())
def test_a_depth_finder_that_finds_nothing_is_an_error_naming_it(run, photo, depth_probe):
    broken = Finder("stand-in depth", lambda rgb: None)  # type: ignore[arg-type,return-value]  # broken on purpose
    with pytest.raises(DepthNotFoundError, match="finder of 'depth' returned None"):
        run(photo, deep(), finders={"depth": broken})


@pytest.mark.parametrize(
    ("keys", "by_hand"),
    [
        pytest.param({"frame": Framing(flip="x")}, lambda a: a[:, ::-1], id="a flip"),
        pytest.param({"frame": Framing(aspect=1.0)}, lambda a: a[:, 16:112], id="a square crop"),
        pytest.param({"frame": Framing(angle=90, fit="expand")}, np.rot90, id="a quarter turn"),
        pytest.param(
            {"frame": Framing(pad_top=0.5)},
            lambda a: np.concatenate([np.zeros((48, 128), np.float32), a]),
            id="a border is farthest",
        ),
        pytest.param({"size": 64}, lambda a: resize(a, 64), id="the size"),
    ],
)
def test_the_depth_map_is_framed_and_scaled_with_the_photo(photo, depth_probe, keys, by_hand):
    np.testing.assert_array_equal(runner.render(photo, deep(**keys), maps={"depth": RAMP})[..., 0], by_hand(RAMP))


def test_a_turn_that_expands_the_picture_leaves_the_corners_farthest(photo, depth_probe):
    out = runner.render(
        photo, deep(frame=Framing(angle=30, fit="expand")), maps={"depth": np.ones((96, 128), np.float32)}
    )
    assert out.shape == (148, 159, 3)
    assert (out[0, 0, 0], out[0, -1, 0], out[-1, 0, 0], out[-1, -1, 0]) == (0, 0, 0, 0)
    assert out[74, 79, 0] == pytest.approx(1)


def test_a_recipe_whose_only_need_for_depth_is_a_region_asks_the_finder_and_traces_the_depth(photo):
    finder = CountingFinder(DOWN)
    stages = runner.trace(photo, FRONT, finders={"depth": Finder("stand-in depth", finder)})
    assert finder.seen == [(96, 128, 3)]
    assert "depth (white = nearest)" in [stage.label for stage in stages]


def test_trace_shows_the_depth_after_the_subject_and_ends_in_the_render(photo, subject, depth_probe):
    recipe = deep(frame=Framing(transparent="background"))
    stages = runner.trace(photo, recipe, maps={"subject": subject, "depth": RAMP})
    assert [stage.label for stage in stages[:3]] == [
        "input",
        "subject matte (white = subject)",
        "depth (white = nearest)",
    ]
    np.testing.assert_array_equal(stages[2].image, np.repeat(RAMP[..., None], 3, axis=2))
    np.testing.assert_array_equal(
        stages[-1].image, runner.render(photo, recipe, maps={"subject": subject, "depth": RAMP})
    )
    assert "depth" not in " ".join(
        stage.label for stage in runner.trace(photo, MASKED, maps={"subject": subject, "depth": RAMP})
    )


# --- a given map: any name a recipe declares, handed in or found like the subject and the depth -----------------------

STAR_RED = Recipe(  # paints red where the `star` map is
    "star-red", [Step("gradient", {"colors": ["#ff0000"]}, mask="star")], size=None, regions={"star": Given()}
)
# a hard-edged star-like mask, off-centre and asymmetric, so that a flip, a crop or a turn that missed it shows
STAR = np.zeros((96, 128), np.float32)
STAR[10:40, 20:50] = 1.0
STAR[60:70, 90:100] = 1.0
PAINTED = {"fill": np.array((1.0, 0.0, 0.0), np.float32)}


def red_where(star: np.ndarray, photo: np.ndarray) -> np.ndarray:
    """`photo` with red painted where the hard-edged map `star` is 1: the oracle for `STAR_RED`."""
    return np.where(star[..., None] == 1, PAINTED["fill"], photo)


@pytest.mark.parametrize(
    "given",
    [
        pytest.param(lambda star: {"maps": {"star": star}}, id="a given map"),
        pytest.param(lambda star: {"finders": {"star": Finder("stand-in star", CountingFinder(star))}}, id="a finder"),
    ],
)
def test_a_given_map_limits_a_step_to_where_it_is(photo, given):
    np.testing.assert_array_equal(runner.render(photo, STAR_RED, **given(STAR)), red_where(STAR, photo))


@pytest.mark.parametrize(
    ("frame", "by_hand"),
    [
        pytest.param(Framing(flip="x"), lambda a: a[:, ::-1], id="a flip"),
        pytest.param(Framing(aspect=1.0), lambda a: a[:, 16:112], id="a square crop"),
        pytest.param(Framing(angle=90, fit="expand"), np.rot90, id="a quarter turn"),
    ],
)
def test_a_given_map_moves_with_the_photo_through_the_framing(photo, frame, by_hand):
    framed = Recipe("framed", STAR_RED.steps, size=None, frame=frame, regions=STAR_RED.regions)
    np.testing.assert_array_equal(
        runner.render(photo, framed, maps={"star": STAR}), red_where(by_hand(STAR), by_hand(photo))
    )


def test_a_given_map_is_scaled_with_the_photo_to_the_recipes_size(photo):
    scaled = Recipe("scaled", STAR_RED.steps, size=64, regions=STAR_RED.regions)
    expected = runner.render(resize(photo, 64), STAR_RED, maps={"star": resize(STAR, 64)})
    np.testing.assert_array_equal(runner.render(photo, scaled, maps={"star": STAR}), expected)


@pytest.mark.parametrize("run", RUNS.values(), ids=RUNS.keys())
def test_a_recipe_that_uses_a_given_map_that_nobody_gave_is_refused_naming_the_map_and_both_ways(run, photo):
    with pytest.raises(MapNotFoundError, match="'star'") as raised:
        run(photo, STAR_RED)
    message = str(raised.value)
    assert f"'{STAR_RED.name}'" in message
    assert "maps={'star'" in message
    assert "finders={'star'" in message


def test_a_given_map_that_no_step_uses_is_not_needed(photo):
    unused = Recipe("unused", PLAIN.steps, size=None, regions={"star": Given()})
    np.testing.assert_array_equal(runner.render(photo, unused), runner.render(photo, PLAIN))


@pytest.mark.parametrize("keyword", ["maps", "finders"])
@pytest.mark.parametrize("run", RUNS.values(), ids=RUNS.keys())
def test_a_name_that_is_no_map_of_the_recipe_is_refused_listing_the_maps_it_has(run, photo, keyword):
    value = STAR if keyword == "maps" else Finder("stand-in", CountingFinder(STAR))
    with pytest.raises(RecipeError, match=rf"`{keyword}` names 'strar', which is no map") as raised:
        run(photo, STAR_RED, **{keyword: {"strar": value}})
    assert all(name in str(raised.value) for name in ("subject", "depth", "star"))


def test_a_map_of_the_wrong_size_is_refused_naming_it_even_when_no_step_uses_it(photo):
    with pytest.raises(MapNotFoundError, match=r"star is \(48, 64\).*\(96, 128\)"):
        runner.render(photo, STAR_RED, maps={"star": STAR[::2, ::2]})
    with pytest.raises(DepthNotFoundError, match=r"depth is \(48, 64\).*\(96, 128\)"):
        runner.render(photo, PLAIN, maps={"depth": RAMP[::2, ::2]})


def test_a_given_maps_values_must_be_finite_and_within_0_and_1(photo):
    with pytest.raises(MapNotFoundError, match=r"^star has .*above 1"):
        runner.render(photo, STAR_RED, maps={"star": STAR * 1.5})


def test_a_finder_of_a_given_map_is_asked_once_with_the_photo_as_given_however_many_recipes_use_the_map(photo):
    finder = CountingFinder(STAR)
    small = Recipe("small", STAR_RED.steps, size=64, frame=Framing(aspect=1.0), regions=STAR_RED.regions)
    runner.render_all(photo, [small, PLAIN, STAR_RED], finders={"star": Finder("stand-in star", finder)})
    assert finder.seen == [(96, 128, 3)]


def test_a_finder_of_a_map_that_answers_none_is_an_error_naming_the_map(photo):
    broken = Finder("stand-in star", lambda rgb: None)  # type: ignore[arg-type,return-value]  # broken on purpose
    with pytest.raises(MapNotFoundError, match="finder of 'star' returned None"):
        runner.render(photo, STAR_RED, finders={"star": broken})


@pytest.mark.parametrize("recipe", [MASKED, FRONT], ids=["subject", "depth"])
def test_a_missing_subject_or_depth_is_caught_as_a_map_error(photo, recipe):
    with pytest.raises(MapNotFoundError):
        runner.render(photo, recipe)


# --- light: a field made from the picture the steps work on ----------------------------------------------------------

LUMA = np.asarray([0.2126, 0.7152, 0.0722])  # Rec. 709, written here again so that the test does not use the code's
DARK = Recipe(  # paints red where the picture is no brighter than half
    "dark",
    [Step("gradient", {"colors": ["#ff0000"]}, mask="dark")],
    size=None,
    regions={"dark": Band(of="light", low=0, high=0.5, feather=0)},
)


def red_where_dark(picture: np.ndarray) -> np.ndarray:
    """`picture` with red where its brightness (by the weights above, in float64) is at most a half."""
    return np.where((picture @ LUMA <= 0.5)[..., None], PAINTED["fill"], picture)


def test_a_recipe_that_uses_light_needs_no_map_from_anyone(photo):
    assert DARK.maps_needed == frozenset()
    assert DARK.derived_maps == frozenset({"light"})
    np.testing.assert_array_equal(runner.render(photo, DARK), red_where_dark(photo))  # no maps, no finders


@pytest.mark.parametrize(
    ("frame", "size", "by_hand"),
    [
        pytest.param(Framing(), None, lambda a: a, id="the photo as it is"),
        pytest.param(Framing(flip="x"), None, lambda a: a[:, ::-1], id="a flip"),
        pytest.param(Framing(aspect=1.0), None, lambda a: a[:, 16:112], id="a crop"),
        pytest.param(Framing(), 64, lambda a: resize(a, 64), id="the recipe's size"),
    ],
)
def test_light_is_the_brightness_of_the_picture_the_steps_work_on_after_framing_and_scaling(
    photo, frame, size, by_hand
):
    recipe = Recipe("dark", DARK.steps, size=size, frame=frame, regions=DARK.regions)
    np.testing.assert_array_equal(runner.render(photo, recipe), red_where_dark(by_hand(photo)))


def test_light_does_not_follow_the_steps_it_is_the_photos_before_any_of_them(photo):
    # a first step that blacks the whole picture out must not change where the second one finds the dark: the regions
    # are made once for the picture the recipe receives
    recipe = Recipe(
        "after",
        [Step("gradient", {"colors": ["#000000"]}), *DARK.steps],
        size=None,
        regions=DARK.regions,
    )
    dark = (photo @ LUMA <= 0.5)[..., None]  # of the PHOTO: the black the first step made is no darker than half
    expected = np.where(dark, PAINTED["fill"], 0.0)
    np.testing.assert_array_equal(runner.render(photo, recipe), expected)


def test_the_light_of_the_subject_is_a_combination_of_the_two_fields(photo, subject):
    lit_subject = Recipe(
        "lit-subject",
        [Step("gradient", {"colors": ["#ff0000"]}, mask="lit_subject")],
        size=None,
        regions={
            "bright": Band(of="light", low=0.5, high=1, feather=0),
            "lit_subject": Combine(of=("subject", "bright")),
        },
    )
    assert lit_subject.maps_needed == frozenset({"subject"})
    bright = (photo @ LUMA > 0.5).astype(np.float64)
    weight = (subject * bright)[..., None]
    np.testing.assert_allclose(
        runner.render(photo, lit_subject, maps={"subject": subject}),
        photo * (1 - weight) + PAINTED["fill"] * weight,
        atol=1e-6,
    )


def test_trace_shows_a_region_made_of_light_and_ends_in_the_render(photo):
    stages = runner.trace(photo, DARK)
    assert "region 'dark' (white = chosen)" in [stage.label for stage in stages]
    np.testing.assert_array_equal(stages[-1].image, runner.render(photo, DARK))


def test_nobody_can_hand_in_light_it_is_made_here(photo):
    with pytest.raises(RecipeError, match="`maps` names 'light'"):
        runner.render(photo, DARK, maps={"light": np.zeros(photo.shape[:2], np.float32)})


# --- a driven parameter: a field varies it pixel by pixel ------------------------------------------------------------


class BrightenParams(EffectParams):
    """The probe that is driven: a gain and a bias, both smooth."""

    gain: float = param(1.0, unit="ratio", min=0, max=4, drivable=True, doc="Multiplies every value.")
    bias: float = param(0.0, unit="fraction", min=-1, max=1, drivable=True, doc="Added after the gain.")
    steps: int = param(1, unit="count", min=1, max=9, doc="Not drivable: a whole number.")


@pytest.fixture
def brighten(throwaway_effects):
    """`brighten_probe`: clip(rgb x gain + bias). The set it returns holds the gains it must never be asked to render: a
    test that puts a gain in it shows that a render was skipped by the render finishing, not by counting calls."""
    refused: set[float] = set()

    @effect(category="light", params=BrightenParams)
    def brighten_probe(rgb, params):
        """Multiplies by a gain and adds a bias."""
        if any(params.gain == pytest.approx(gain) for gain in refused):
            raise AssertionError(f"the effect was rendered at the gain {params.gain}, which nothing needed")
        return np.clip(rgb * params.gain + params.bias, 0, 1).astype(np.float32)

    return refused


def driven(by: str = "f", levels: int = 5, **drives: tuple[float, float]) -> Recipe:
    """A recipe of one `brighten_probe` step whose parameters vary by the given map `f` (or a built-in field)."""
    values = {name: Drive(by=by, start=start, end=end, levels=levels) for name, (start, end) in drives.items()}
    return Recipe("driven", [Step("brighten_probe", values)], size=None, regions={"f": Given()})


def plain(**values: float) -> Recipe:
    return Recipe("plain", [Step("brighten_probe", values)], size=None)


def field_of(photo: np.ndarray, value: float) -> np.ndarray:
    return np.full(photo.shape[:2], value, np.float32)


@pytest.mark.parametrize("level", range(5))
def test_a_constant_field_gives_the_plain_effect_at_that_levels_value(photo, brighten, level):
    # 0.5 + 2 x level / 4 by hand: the levels run 0.5, 1, 1.5, 2, 2.5
    expected = runner.render(photo, plain(gain=0.5 + 2.0 * level / 4))
    got = runner.render(photo, driven(gain=(0.5, 2.5)), maps={"f": field_of(photo, level / 4)})
    np.testing.assert_allclose(got, expected, atol=1e-6, rtol=0)


def test_a_field_between_two_levels_mixes_their_renders_by_the_distance_to_each(photo, brighten):
    # levels 0, 0.5, 1 of the gain; a field of 0.25 is half way from the first to the second
    first, second = runner.render(photo, plain(gain=0.0)), runner.render(photo, plain(gain=2.0))
    got = runner.render(photo, driven(levels=3, gain=(0.0, 4.0)), maps={"f": field_of(photo, 0.25)})
    np.testing.assert_allclose(got, 0.5 * first + 0.5 * second, atol=1e-6, rtol=0)


def test_with_two_levels_each_pixel_mixes_the_effect_at_start_and_at_end_by_its_field(photo, brighten):
    ramp = np.tile(np.linspace(0, 1, photo.shape[1], dtype=np.float32), (photo.shape[0], 1))
    at_start, at_end = runner.render(photo, plain(gain=0.5)), runner.render(photo, plain(gain=3.0))
    got = runner.render(photo, driven(levels=2, gain=(0.5, 3.0)), maps={"f": ramp})
    np.testing.assert_allclose(got, (1 - ramp)[..., None] * at_start + ramp[..., None] * at_end, atol=1e-6, rtol=0)


def test_a_field_running_the_other_way_is_the_start_and_end_swapped(photo, brighten):
    ramp = np.tile(np.linspace(0, 1, photo.shape[1], dtype=np.float32), (photo.shape[0], 1))
    forward = runner.render(photo, driven(gain=(0.5, 3.0)), maps={"f": ramp})
    backward = runner.render(photo, driven(gain=(3.0, 0.5)), maps={"f": 1 - ramp})
    np.testing.assert_allclose(backward, forward, atol=1e-6, rtol=0)


@pytest.mark.parametrize(
    ("value", "not_needed"),
    [
        pytest.param(0.0, [1.0, 1.5, 2.0, 2.5], id="a field of 0 needs only the first level"),
        pytest.param(0.5, [0.5, 1.0, 2.0, 2.5], id="a field on a level needs only that level"),
        pytest.param(0.3, [0.5, 2.0, 2.5], id="a field between two levels needs those two"),
        pytest.param(1.0, [0.5, 1.0, 1.5, 2.0], id="a field of 1 needs only the last level"),
    ],
)
def test_a_level_no_pixel_needs_is_never_rendered(photo, brighten, value, not_needed):
    brighten.update(not_needed)  # the effect raises if asked for one of these: the render finishing is the proof
    got = runner.render(photo, driven(gain=(0.5, 2.5)), maps={"f": field_of(photo, value)})
    assert got.shape == photo.shape


def test_two_drives_of_one_step_move_together_along_the_same_field(photo, brighten):
    got = runner.render(photo, driven(levels=3, gain=(0.5, 1.5), bias=(0.0, 0.2)), maps={"f": field_of(photo, 0.5)})
    np.testing.assert_allclose(got, runner.render(photo, plain(gain=1.0, bias=0.1)), atol=1e-6, rtol=0)


def test_a_parameter_that_is_not_driven_keeps_its_written_value(photo, brighten):
    recipe = Recipe(
        "mixed",
        [Step("brighten_probe", {"gain": Drive(by="f", start=0.5, end=2.5), "bias": 0.25})],
        size=None,
        regions={"f": Given()},
    )
    got = runner.render(photo, recipe, maps={"f": field_of(photo, 1.0)})
    np.testing.assert_allclose(got, runner.render(photo, plain(gain=2.5, bias=0.25)), atol=1e-6, rtol=0)


def test_a_driven_step_is_limited_and_mixed_in_like_any_other(photo, brighten, subject):
    both = Recipe(
        "both",
        [Step("brighten_probe", {"gain": Drive(by="f", start=0.5, end=2.5)}, mask="subject", opacity=0.5)],
        size=None,
        regions={"f": Given()},
    )
    got = runner.render(photo, both, maps={"f": field_of(photo, 1.0), "subject": subject})
    brightened = runner.render(photo, plain(gain=2.5))
    weight = (0.5 * subject)[..., None]  # opacity 0.5 where the subject is
    np.testing.assert_allclose(got, photo * (1 - weight) + brightened * weight, atol=1e-6, rtol=0)


def test_the_look_of_an_each_can_be_driven(photo, brighten):
    recipe = Recipe(
        "pieces",
        [Each("g", [Step("brighten_probe", {"gain": Drive(by="f", start=0.5, end=2.5)}), "none"], feather=0)],
        size=None,
        regions={"g": Grid(columns=2), "f": Given()},
    )
    got = runner.render(photo, recipe, maps={"f": field_of(photo, 1.0)})
    brightened = runner.render(photo, plain(gain=2.5))
    np.testing.assert_allclose(got[:, :64], brightened[:, :64], atol=1e-6)  # piece 0 has the driven look
    np.testing.assert_array_equal(got[:, 64:], photo[:, 64:])  # piece 1 stays as it was


def test_a_drive_by_the_depth_map_follows_the_depth(photo, brighten):
    dim = photo * 0.4  # at most 0.4, so a gain up to 2 never clips: the render is then linear in the gain
    depth = np.tile(np.linspace(0, 1, photo.shape[1], dtype=np.float32), (photo.shape[0], 1))
    recipe = Recipe("by-depth", [Step("brighten_probe", {"gain": Drive(by="depth", start=0.0, end=2.0)})], size=None)
    got = runner.render(dim, recipe, maps={"depth": depth})
    # a mix of renders that are linear in the gain is the render at the mixed gain: gain 2 x depth, pixel by pixel
    np.testing.assert_allclose(got, dim * (2.0 * depth)[..., None], atol=1e-6)


def test_a_drive_by_a_band_of_the_depth_is_the_same_as_the_band_given_as_a_map(photo, brighten):
    depth = np.tile(np.linspace(0, 1, photo.shape[1], dtype=np.float32), (photo.shape[0], 1))
    banded = Recipe(
        "banded",
        [Step("brighten_probe", {"gain": Drive(by="near", start=0.5, end=2.5)})],
        size=None,
        regions={"near": Band(of="depth", low=0.6, high=1.0, feather=0.2)},
    )
    by_band = runner.render(photo, banded, maps={"depth": depth})
    near = np.clip(1 - np.maximum(np.maximum(0.6 - depth, depth - 1.0), 0) / 0.2, 0, 1)  # by hand
    np.testing.assert_allclose(by_band, runner.render(photo, driven(gain=(0.5, 2.5)), maps={"f": near}), atol=1e-6)


def test_a_drive_by_light_follows_the_brightness_of_the_picture_the_steps_work_on(photo, brighten):
    recipe = Recipe(
        "by-light", [Step("brighten_probe", {"gain": Drive(by="light", start=0.0, end=1.0, levels=2)})], size=None
    )
    light = (photo @ LUMA)[..., None]
    np.testing.assert_allclose(runner.render(photo, recipe), np.clip(photo * light, 0, 1), atol=1e-5)


def test_a_driven_render_stays_within_0_and_1(photo, brighten):
    field = np.random.default_rng(2).random(photo.shape[:2], dtype=np.float32)
    got = runner.render(photo, driven(gain=(0.0, 4.0), bias=(-1.0, 1.0)), maps={"f": field})
    assert got.min() >= 0
    assert got.max() <= 1


def test_the_mix_of_renders_that_are_all_white_is_white_not_a_hair_above_it(brighten):
    # every level of this drive paints white; the float32 weights of two levels can add up to a hair over 1
    white = np.ones((64, 64, 3), np.float32)
    field = np.random.default_rng(3).random((64, 64), dtype=np.float32)
    got = runner.render(white, driven(levels=16, gain=(2.0, 4.0)), maps={"f": field})
    assert got.max() == 1.0
    assert got.min() == 1.0


def test_a_driven_step_is_traced_by_name_and_its_last_stage_is_the_render(photo, brighten):
    recipe = driven(gain=(0.5, 2.5))
    stages = runner.trace(photo, recipe, maps={"f": field_of(photo, 0.5)})
    assert stages[-1].label == "1. brighten_probe(gain=by f 0.5..2.5)"
    np.testing.assert_array_equal(stages[-1].image, runner.render(photo, recipe, maps={"f": field_of(photo, 0.5)}))


def test_a_drive_by_a_map_nobody_gave_is_refused_like_any_use_of_it(photo, brighten):
    with pytest.raises(MapNotFoundError, match="'f'"):
        runner.render(photo, driven(gain=(0.5, 2.5)))


# --- framing: the photo and its subject are framed together, before the steps ------------------------------------


def test_a_recipe_without_framing_runs_its_steps_on_the_photo_scaled_to_its_size(photo):
    scaled = Recipe("scaled", PLAIN.steps, size=64)
    by_hand = runner.render(resize(photo, 64), PLAIN)
    np.testing.assert_array_equal(runner.render(photo, scaled), by_hand)


@pytest.mark.parametrize(
    ("frame", "by_hand"),
    [
        pytest.param(Framing(aspect=1.0), lambda a: a[:, 16:112], id="a square crop"),
        pytest.param(Framing(aspect=1.0, anchor="left"), lambda a: a[:, :96], id="a square crop at the left"),
        pytest.param(Framing(flip="x"), lambda a: a[:, ::-1], id="a flip"),
        pytest.param(Framing(angle=90, fit="expand"), lambda a: np.rot90(a), id="a quarter turn"),
    ],
)
def test_the_steps_run_on_the_framed_photo_and_a_masked_step_still_finds_the_subject_where_it_moved(
    photo, subject, frame, by_hand
):
    framed = Recipe("framed", MASKED.steps, size=None, frame=frame)
    expected = runner.render(by_hand(photo), MASKED, maps={"subject": by_hand(subject)})  # background black, by hand
    np.testing.assert_array_equal(runner.render(photo, framed, maps={"subject": subject}), expected)
    assert (expected == 0).any()
    assert np.array_equal(
        runner.render(photo, framed, maps={"subject": subject})[by_hand(subject) == 1],
        by_hand(photo)[by_hand(subject) == 1],
    )


def test_framing_comes_before_the_size(photo):
    framed = Recipe("framed", PLAIN.steps, size=48, frame=Framing(aspect=1.0))
    by_hand = runner.render(resize(photo[:, 16:112], 48), PLAIN)  # crop 96 px square, then scale it to 48
    np.testing.assert_array_equal(runner.render(photo, framed), by_hand)


def test_a_crop_around_the_subject_uses_the_matte_that_the_finder_found_on_the_whole_photo(photo, subject):
    framed = Recipe("framed", PLAIN.steps, size=None, frame=Framing(anchor="subject", aspect=1.0, margin=0.1))
    for result in (
        runner.render(photo, framed, maps={"subject": subject}),
        runner.render(photo, framed, finders={"subject": Finder("stand-in", CountingFinder(subject))}),
    ):
        np.testing.assert_array_equal(result, runner.render(photo[13:84, 29:100], PLAIN))  # the disc and 6 px each way


def test_a_subject_crop_needs_the_subject_even_when_no_step_does(photo):
    framed = Recipe("framed", PLAIN.steps, size=None, frame=Framing(anchor="subject"))
    with pytest.raises(SubjectNotFoundError, match="needs the subject"):
        runner.render(photo, framed)


def test_trace_shows_the_framed_photo_and_matte_and_ends_in_the_render(photo, subject):
    framed = Recipe("framed", MASKED.steps, size=None, frame=Framing(aspect=1.0))
    stages = runner.trace(photo, framed, maps={"subject": subject})
    np.testing.assert_array_equal(stages[0].image, photo[:, 16:112])
    np.testing.assert_array_equal(stages[1].image[..., 0], subject[:, 16:112])
    np.testing.assert_array_equal(stages[-1].image, runner.render(photo, framed, maps={"subject": subject}))


def test_render_all_finds_the_subject_once_for_framed_and_plain_recipes(photo, subject):
    finder = CountingFinder(subject)
    framed = Recipe("framed", PLAIN.steps, size=None, frame=Framing(anchor="subject"))
    results = runner.render_all(photo, [PLAIN, framed], finders={"subject": Finder("stand-in", finder)})
    assert [r.shape[:2] for r in results] == [(96, 128), (59, 59)]
    assert len(finder.seen) == 1


# --- transparency: the picture is cut out after the last step -------------------------------------------------------


def test_a_circle_cut_gives_four_channels_the_steps_result_and_the_inscribed_circle_as_alpha(photo):
    recipe = Recipe("round", PLAIN.steps, size=None, frame=Framing(transparent="circle"))
    out = runner.render(photo, recipe)
    assert out.shape == (96, 128, 4)
    np.testing.assert_array_equal(out[..., :3], runner.render(photo, PLAIN))
    ys, xs = np.mgrid[0:96, 0:128]
    distance = np.hypot(ys + 0.5 - 48, xs + 0.5 - 64)  # from the middle; the circle's radius is 48
    assert (out[..., 3][distance < 47] == 1).all()
    assert (out[..., 3][distance > 49] == 0).all()


def test_the_alpha_of_a_background_cut_is_the_subject_matte_that_moved_with_the_photo(photo, subject):
    recipe = Recipe("cut", PLAIN.steps, size=None, frame=Framing(transparent="background", flip="x", aspect=1.0))
    out = runner.render(photo, recipe, maps={"subject": subject})
    np.testing.assert_array_equal(out[..., 3], subject[:, ::-1][:, 16:112])
    np.testing.assert_array_equal(out[..., :3], runner.render(photo[:, ::-1][:, 16:112], PLAIN))


def test_the_alpha_of_rounded_corners_is_clear_in_the_corners_and_opaque_in_the_middle_of_each_side(photo):
    recipe = Recipe("rounded", PLAIN.steps, size=None, frame=Framing(transparent="rounded", corner_radius=0.25))
    alpha = runner.render(photo, recipe)[..., 3]  # a corner radius of a quarter of the 96 px side: 24 px
    assert [alpha[0, 0], alpha[0, -1], alpha[-1, 0], alpha[-1, -1]] == [0, 0, 0, 0]
    assert [alpha[0, 64], alpha[-1, 64], alpha[48, 0], alpha[48, -1]] == [1, 1, 1, 1]


def test_the_cut_is_made_at_the_recipes_size_after_the_steps(photo, subject):
    recipe = Recipe("cut", PLAIN.steps, size=64, frame=Framing(transparent="background"))
    out = runner.render(photo, recipe, maps={"subject": subject})
    assert out.shape == (48, 64, 4)
    np.testing.assert_array_equal(out[..., 3], resize(subject, 64))


def test_a_cut_around_the_subject_needs_the_subject_even_when_no_step_does(photo):
    recipe = Recipe("cut", PLAIN.steps, size=None, frame=Framing(transparent="background"))
    with pytest.raises(SubjectNotFoundError, match="needs the subject"):
        runner.render(photo, recipe)


def test_trace_adds_the_cut_as_its_last_stage_and_it_is_what_render_returns(photo, subject):
    recipe = Recipe("cut", MASKED.steps, size=None, frame=Framing(transparent="circle"))
    stages = runner.trace(photo, recipe, maps={"subject": subject})
    assert [stage.image.shape[2] for stage in stages] == [3, 3, 3, 4]  # input, subject, the step, the cut
    assert stages[-1].label == "2. transparent('circle')"
    np.testing.assert_array_equal(stages[-1].image, runner.render(photo, recipe, maps={"subject": subject}))
    np.testing.assert_array_equal(stages[-1].image[..., :3], stages[-2].image)


def test_render_all_cuts_only_the_recipes_that_ask_for_it(photo):
    round_ = Recipe("round", PLAIN.steps, size=None, frame=Framing(transparent="circle"))
    results = runner.render_all(photo, [PLAIN, round_])
    assert [r.shape[2] for r in results] == [3, 4]


# --- memory: the runner keeps only the image the next step needs ---------------------------------------------------


class ScratchParams(EffectParams):
    """The memory probes' settings."""

    copies: int = param(4, unit="count", min=1, max=8, doc="Picture-sized scratch buffers the step fills as it works.")


SCRATCH_COPIES = 4  # each scratch step holds this many pictures, plus its output, at its peak


def peak_pictures(photo: np.ndarray, recipe: Recipe, subject: np.ndarray | None = None) -> float:
    """The most memory `render` had allocated at once (beyond the photo and subject, which the caller holds), in
    pictures of the working size."""
    maps = {} if subject is None else {"subject": subject}
    picture = runner.render(photo, recipe, maps=maps).nbytes  # also warms up: one-time allocations stay out
    tracemalloc.start()
    try:
        runner.render(photo, recipe, maps=maps)
        return tracemalloc.get_traced_memory()[1] / picture
    finally:
        tracemalloc.stop()


@pytest.fixture
def memory_probes(throwaway_effects):
    """`scratch_probe` works in `copies` picture-sized buffers and returns a new picture; `hoarding_probe` returns a new
    picture but keeps every picture it is given alive, the way a leaky runner would."""
    hoard: list[np.ndarray] = []

    @effect(category="utilities", params=ScratchParams)
    def scratch_probe(rgb, params):
        """Fills scratch pictures, then returns a copy of the input."""
        scratch = np.repeat(rgb[None], params.copies, axis=0)
        return scratch[-1].copy()

    @effect(category="utilities", params=ScratchParams)
    def hoarding_probe(rgb, params):
        """Keeps its input alive and returns a copy of it."""
        hoard.append(rgb)
        return rgb.copy()

    yield
    hoard.clear()


PHOTO = np.random.default_rng(3).random((210, 330, 3), dtype=np.float32)  # scaled to 320 px: a new picture
SUBJECT = np.zeros((210, 330), np.float32)
SUBJECT[50:150, 80:250] = 1.0


@pytest.mark.parametrize(
    ("mask", "subject", "kept"),
    [
        (None, None, 0.0),
        ("subject", SUBJECT, 1 / 3),  # the scaled subject, one channel: a third of a picture, needed by every step
        ("background", SUBJECT, 2 / 3),  # and the background selection made from it, kept for the steps that mask
    ],
    ids=["plain", "subject", "background"],
)
def test_a_linear_render_holds_no_more_than_the_working_step_its_input_and_output(memory_probes, mask, subject, kept):
    """A step's input is released once the next step has run, and neither the scaled photo nor a masked step's blend
    weights are kept for the rest of the chain: the peak is the busiest step's scratch, output and input (and the
    scaled subject), whatever the number of steps."""
    steps = tuple(Step("scratch_probe", {"copies": SCRATCH_COPIES}, mask) for _ in range(6))
    bound = (
        SCRATCH_COPIES + 1 + 1 + kept + 0.25
    )  # scratch, output, input, the selections, a quarter picture for small things
    assert peak_pictures(PHOTO, Recipe("chain", steps, size=320), subject) <= bound
    # known-bad: the same gauge sees a picture that something keeps alive
    leaky = (*steps[:3], Step("hoarding_probe", {}, mask), *steps[3:])
    assert peak_pictures(PHOTO, Recipe("leaky", leaky, size=320), subject) > bound


# --- regions: `each` gives every piece of a partition its look; `mask` limits a step to a selection -----------------

RED, GREEN, BLUE, WHITE = "#ff0000", "#00ff00", "#0000ff", "#ffffff"
RGB = {RED: (1.0, 0.0, 0.0), GREEN: (0.0, 1.0, 0.0), BLUE: (0.0, 0.0, 1.0), WHITE: (1.0, 1.0, 1.0)}


def fill(color: str, **keys) -> Step:
    return Step("gradient", {"colors": [color]}, **keys)


def test_each_over_depth_bands_gives_every_distance_its_look_nearest_first(photo):
    looks = Each("bands", [fill(RED), fill(GREEN), fill(BLUE)], feather=0)
    recipe = Recipe("by-distance", [looks], size=None, regions={"bands": Bands(of="depth", count=3)})
    by_hand = np.empty((96, 128, 3), np.float32)
    by_hand[:32], by_hand[32:64], by_hand[64:] = RGB[RED], RGB[GREEN], RGB[BLUE]  # nearer than 2/3, than 1/3, the rest
    np.testing.assert_allclose(runner.render(photo, recipe, maps={"depth": DOWN}), by_hand, atol=1e-6)


@pytest.mark.parametrize("given", WITH_DEPTH.values(), ids=WITH_DEPTH.keys())
def test_a_step_masked_to_a_depth_selection_changes_only_what_lies_within_it(photo, given):
    out = runner.render(photo, FRONT, **given(DOWN))
    np.testing.assert_allclose(out[:24], np.broadcast_to(RGB[RED], (24, 128, 3)), atol=1e-6)  # depth 0.75 and more
    np.testing.assert_array_equal(out[24:], photo[24:])


def columns(count: int) -> dict[str, Grid | Pick | Shards]:
    return {"cols": Grid(columns=count, rows=1)}


def run(photo, steps, regions, **keys):
    return runner.render(photo, Recipe("regions", steps, size=None, regions=regions), **keys)


@pytest.mark.parametrize("count", [1, 2, 4, 5])
def test_each_piece_shows_its_look_and_the_looks_take_turns(photo, count):
    looks = [fill(RED), fill(GREEN), fill(BLUE)]
    out = run(photo, [Each("cols", looks, feather=0)], columns(count))
    width = photo.shape[1]
    for x in range(width):
        column = int((x + 0.5) * count / width)  # the piece holding the pixel's centre, worked out by hand
        want = RGB[(RED, GREEN, BLUE)[column % 3]]
        assert (out[:, x] == want).all(), (count, x)


@pytest.mark.parametrize("size", [(1, 1), (1, 7), (7, 1), (13, 29), (61, 37)])
@pytest.mark.parametrize("feather", [0, 0.5 / 128, 3 / 128])  # shares of the 128 px long side
@pytest.mark.parametrize("assign", ["cycle", "random"])
def test_the_looks_weights_add_up_to_one_at_every_pixel_whatever_the_size_and_feather(size, feather, assign):
    photo = np.random.default_rng(2).random((*size, 3), dtype=np.float32)
    pure = [
        fill(RED),
        fill(GREEN),
        fill(BLUE),
    ]  # a pure look is its own weight in its channel, so the channels add up to 1
    out = run(photo, [Each("cols", pure, assign=assign, feather=feather)], columns(5))
    assert out.shape == photo.shape
    assert out.min() >= 0
    assert out.max() <= 1
    np.testing.assert_allclose(out.sum(axis=2), 1.0, atol=1e-5)


def test_a_none_look_leaves_its_pieces_exactly_as_they_were(photo):
    out = run(photo, [Each("cols", [fill(RED), "none"], feather=0)], columns(4))
    np.testing.assert_array_equal(out[:, 32:64], photo[:, 32:64])
    np.testing.assert_array_equal(out[:, 96:], photo[:, 96:])
    assert (out[:, 0:32] == RGB[RED]).all()


def test_a_none_look_is_exact_away_from_the_feathered_borders(photo):
    out = run(photo, [Each("cols", [fill(RED), "none"], feather=2 / 128)], columns(4))
    reach = 4 * 2 + 1  # a feather of 2 px, cut off at 4 sigmas
    np.testing.assert_array_equal(out[:, 32 + reach : 64 - reach], photo[:, 32 + reach : 64 - reach])
    assert (out[:, 0 : 32 - reach] == RGB[RED]).all()
    border = out[:, 31, 1]  # the green of the last column of the red piece: red has none, the photo some
    assert (border > 0).all()
    assert (border < photo[:, 31, 1]).all()  # a mix of the two looks, not either one


@pytest.mark.parametrize("feather", [0, 3 / 128])
@pytest.mark.parametrize("assign", ["cycle", "random"])
def test_looks_that_change_nothing_give_back_the_photo_exactly(photo, feather, assign):
    nothing: list[Step | str] = [fill(RED, opacity=0.0), fill(GREEN, opacity=0.0), "none"]
    np.testing.assert_array_equal(
        run(photo, [Each("cols", nothing, assign=assign, feather=feather)], columns(7)), photo
    )


def test_a_look_with_its_own_opacity_and_blend_mode_is_laid_over_the_photo_like_a_step(photo):
    looks = [fill("#808080", blend_mode="multiply"), fill(WHITE, opacity=0.25)]
    out = run(photo, [Each("cols", looks, feather=0)], columns(2))
    np.testing.assert_allclose(out[:, :64], photo[:, :64] * (128 / 255), atol=1e-6)
    np.testing.assert_allclose(out[:, 64:], photo[:, 64:] * 0.75 + 0.25, atol=1e-6)


def test_a_look_takes_its_effects_default_opacity_unless_it_gives_one(photo, throwaway_effects):
    @effect(category="utilities", params=ScratchParams, opacity=0.4)
    def faint_white(rgb, params):
        """Paints white, shown at 0.4 unless a step says otherwise."""
        return np.ones_like(rgb)

    out = run(photo, [Each("cols", [Step("faint_white"), Step("faint_white", opacity=1.0)], feather=0)], columns(2))
    np.testing.assert_allclose(out[:, :64], photo[:, :64] * 0.6 + 0.4, atol=1e-6)
    assert (out[:, 64:] == 1).all()


def test_the_each_steps_own_opacity_and_blend_mode_apply_to_the_blended_looks(photo):
    looks = [fill(RED), fill(GREEN)]
    half = run(photo, [Each("cols", looks, feather=0, opacity=0.5)], columns(2))
    np.testing.assert_allclose(half[:, :64], photo[:, :64] * 0.5 + 0.5 * np.array(RGB[RED], np.float32), atol=1e-6)
    np.testing.assert_allclose(half[:, 64:], photo[:, 64:] * 0.5 + 0.5 * np.array(RGB[GREEN], np.float32), atol=1e-6)
    multiplied = run(photo, [Each("cols", looks, feather=0, blend_mode="multiply")], columns(2))
    np.testing.assert_allclose(multiplied[:, :64], photo[:, :64] * np.array(RGB[RED], np.float32), atol=1e-6)


HEAD: dict[str, Grid | Pick | Shards] = {
    "panes": Grid(columns=2, rows=2),
    "head": Pick(of="panes", pieces=[1]),
}  # the top right quarter


@pytest.mark.parametrize(
    "step",
    [
        pytest.param(fill(RED, mask="head"), id="a step"),
        pytest.param(Each("panes", [fill(RED)], mask="head"), id="an each step"),
    ],
)
def test_a_step_masked_by_a_region_changes_only_the_pixels_the_region_chooses(photo, step):
    out = run(photo, [step], HEAD)
    assert (out[:48, 64:] == RGB[RED]).all()
    np.testing.assert_array_equal(out[:48, :64], photo[:48, :64])
    np.testing.assert_array_equal(out[48:], photo[48:])


def test_a_mask_can_name_a_region_made_from_the_subject(photo, subject):
    regions = {"around": Shards(count=6, fit="subject"), "piece": Pick(of="around", pieces=[0])}
    out = run(photo, [fill(RED, mask="piece")], regions, maps={"subject": subject})
    changed = (out != photo).any(axis=2)
    assert changed.any()
    assert not changed.all()  # one shard of six


class Stamps:
    """The renders so far of the `stamp` effect, which paints its own number into the picture."""

    count = 0


@pytest.fixture
def stamp(throwaway_effects):
    Stamps.count = 0

    @effect(category="utilities", params=ScratchParams)
    def stamp(rgb, params):
        """Paints a flat grey of 0.01 per render so far, counting this one."""
        Stamps.count += 1
        return np.full_like(rgb, Stamps.count / 100)


def test_every_look_is_rendered_once_over_the_whole_picture_however_many_pieces_it_has(photo, stamp):
    twice = Step("stamp", {"copies": 2})
    out = run(photo, [Each("cols", [Step("stamp"), twice, "none"], feather=0), Step("stamp")], columns(30))
    # 30 pieces, 3 looks, one a "none": two renders (0.01 and 0.02) and the step after them is the third
    assert set(np.unique(out)) == {np.float32(0.03)}


def test_a_look_no_piece_has_is_not_rendered(photo, stamp):
    out = run(
        photo, [Each("cols", [Step("stamp"), Step("stamp"), Step("stamp")], feather=0), Step("stamp")], columns(2)
    )
    assert set(np.unique(out)) == {np.float32(0.03)}  # two pieces use two looks; the third would have made it 0.04


def test_the_random_assignment_is_decided_by_the_seed_alone(photo):
    looks = [fill(RED), fill(GREEN), fill(BLUE)]

    def render_with(seed: int) -> np.ndarray:
        return run(photo, [Each("cols", looks, assign="random", seed=seed, feather=0)], columns(40))

    np.testing.assert_array_equal(render_with(3), render_with(3))
    assert not np.array_equal(render_with(3), render_with(4))
    chosen = {tuple(pixel) for pixel in render_with(3).reshape(-1, 3)}
    assert chosen == set(RGB.values()) - {RGB[WHITE]}  # all three looks are used, and nothing else appears
    cycle = run(photo, [Each("cols", looks, feather=0)], columns(40))
    assert not np.array_equal(render_with(3), cycle)


def test_regions_are_made_for_the_working_picture_after_framing_and_scaling(photo):
    recipe = Recipe(
        "framed",
        [Each("halves", [fill(RED), fill(GREEN)], feather=0)],
        size=64,
        frame=Framing(aspect=1.0),
        regions={"halves": Grid(columns=2, rows=1)},
    )
    out = runner.render(photo, recipe)
    assert out.shape == (64, 64, 3)
    assert (out[:, :32] == RGB[RED]).all()
    assert (out[:, 32:] == RGB[GREEN]).all()


# --- regions and the subject: found at most once, and only when a region in use needs it ------------------------------


def test_a_declared_region_nobody_uses_costs_nothing_and_does_not_look_for_the_subject(photo, subject):
    finder = CountingFinder(subject)
    unused = {"glass": Shards(fit="subject"), "head": Pick(of="glass")}
    out = run(
        photo,
        [fill(RED, mask="head")] if False else [Step("grade", {"exposure": 0.5})],
        unused,
        finders={"subject": Finder("stand-in", finder)},
    )
    assert finder.seen == []
    np.testing.assert_array_equal(out, run(photo, [Step("grade", {"exposure": 0.5})], {}))


def test_a_region_in_use_asks_for_the_subject_once_however_many_steps_use_it(photo, subject):
    finder = CountingFinder(subject)
    regions = {"glass": Shards(count=5, fit="subject"), "head": Pick(of="glass", pieces=[0])}
    steps = [fill(RED, mask="head"), Each("glass", [fill(GREEN), "none"]), fill(BLUE, mask="subject")]
    run(photo, steps, regions, finders={"subject": Finder("stand-in", finder)})
    assert finder.seen == [(96, 128, 3)]  # the photo as given


def test_a_region_laid_over_the_subject_follows_the_subject_scaled_to_the_working_picture(photo, subject):
    recipe = Recipe(
        "scaled",
        [Each("sides", [fill(RED), fill(GREEN)], feather=0)],
        size=64,
        regions={"sides": Grid(columns=2, rows=1, fit="subject")},
    )
    out = runner.render(photo, recipe, finders={"subject": Finder("stand-in", CountingFinder(subject))})
    assert out.shape == (48, 64, 3)
    middle = 32  # the disc is in the middle of the picture, so its box is cut in half there
    assert (out[:, :middle] == RGB[RED]).all()
    assert (out[:, middle:] == RGB[GREEN]).all()


@pytest.mark.parametrize("run_it", RUNS.values(), ids=RUNS.keys())
def test_a_region_that_needs_the_subject_without_a_finder_is_refused_like_a_masked_step(run_it, photo):
    recipe = Recipe("fitted", [Each("glass", ["none"])], size=None, regions={"glass": Shards(fit="subject")})
    with pytest.raises(SubjectNotFoundError, match="needs the subject"):
        run_it(photo, recipe)


# --- the partition an effect draws ---------------------------------------------------------------------------------


def test_an_effect_that_takes_a_partition_gets_the_pieces_numbered_in_reading_order(photo, partition_probe):
    out = run(photo, [Step("partition_probe", partition="panes")], {"panes": Grid(columns=3, rows=2)})
    height, width = photo.shape[:2]
    for y in range(height):
        for x in range(width):
            want = int((y + 0.5) * 2 / height) * 3 + int((x + 0.5) * 3 / width)  # row after row, by hand
            assert out[y, x, 0] * 255 == pytest.approx(want), (x, y)


def test_a_selection_is_given_to_an_effect_as_a_partition_of_two_pieces(photo, subject, partition_probe):
    out = run(photo, [Step("partition_probe", partition="subject")], {}, maps={"subject": subject})
    np.testing.assert_allclose(out[..., 0] * 255, subject > 0.5)  # outside the subject first: it holds the first pixel


# --- trace: the regions a recipe uses, with the pieces numbered ----------------------------------------------------

TRACED: dict[str, Grid | Pick | Shards] = {
    "panes": Grid(columns=10, rows=1),
    "head": Pick(of="panes", pieces=[1]),
    "unused": Grid(columns=2, rows=2),
}


def traced(photo, **keys):
    recipe = Recipe("traced", [Step("grade", {"exposure": 0.2}, mask="head")], size=None, regions=TRACED)
    return runner.trace(photo, recipe, **keys)


def test_trace_shows_each_region_the_steps_use_between_the_input_and_the_steps_and_none_they_do_not(photo):
    stages = traced(photo)
    assert [stage.label.split("(")[0].strip() for stage in stages] == [
        "input",
        "region 'panes'",
        "region 'head'",
        "1. grade",
    ]  # in the order declared, with the one `head` is picked from, and not the unused one
    assert all(stage.image.shape == photo.shape for stage in stages)
    np.testing.assert_array_equal(stages[0].image, photo)


def piece_columns(piece: int, margin: int = 0) -> slice:
    """The pixel columns of piece `piece` of 10 equal columns over 128 px (a pixel belongs to the piece holding its
    centre), without `margin` px at each side."""
    first, last = (math.ceil(k * 12.8 - 0.5) for k in (piece, piece + 1))
    return slice(first + margin, last - margin)


def test_the_stage_of_a_selection_is_white_where_it_chooses_and_black_elsewhere(photo):
    head = traced(photo)[2].image
    assert (head[:, piece_columns(1)] == 1).all()
    assert (head[:, : piece_columns(1).start] == 0).all()
    assert (head[:, piece_columns(1).stop :] == 0).all()


def test_the_stage_of_a_partition_draws_the_borders_and_a_number_in_every_piece(photo):
    panes = traced(photo)[1].image
    white = panes.min(axis=2) > 0.5  # the photo is dimmed, so only lines and numbers are this bright
    for piece in range(10):
        assert white[:, piece_columns(piece, 2)].any(), piece  # a number, away from the borders
    assert white[:, 12:14].any()  # a border between two pieces
    assert not np.array_equal(panes, photo)


def test_the_number_drawn_in_a_piece_is_its_own_a_one_has_less_ink_than_an_eight(photo):
    panes = traced(photo)[1].image

    def ink(piece: int) -> int:
        return int((panes[:, piece_columns(piece, 2)].min(axis=2) > 0.5).sum())

    assert ink(1) < ink(8)


def test_trace_still_ends_in_what_render_returns_when_regions_are_drawn(photo):
    recipe = Recipe("traced", [Step("grade", {"exposure": 0.2}, mask="head")], size=None, regions=TRACED)
    np.testing.assert_array_equal(runner.trace(photo, recipe)[-1].image, runner.render(photo, recipe))


def test_trace_labels_an_each_step_with_its_partition_and_looks(photo):
    each = Each("panes", [fill(RED), "none"], mask="head")
    labels = [stage.label for stage in runner.trace(photo, Recipe("t", [each], size=None, regions=TRACED))]
    assert labels[-1].startswith("1. each panes [gradient(colors=['#ff0000']), none]")


def test_a_feather_is_a_share_of_the_long_side_so_the_blend_is_the_same_at_two_resolutions():
    grey = np.full((96, 128, 3), 0.5, np.float32)
    recipe = [Each("cols", [fill(RED), "none"], feather=0.02)]

    def rendered(size: int) -> np.ndarray:
        return runner.render(grey, Recipe("scaled", recipe, size=size, regions=columns(2)))

    small, large = rendered(128), rendered(256)
    halved = large.reshape(96, 2, 128, 2, 3).mean(axis=(1, 3))  # the large one at the small one's resolution
    np.testing.assert_allclose(halved, small, atol=0.03)
    assert 0.1 < small[0, 63, 1] < 0.4  # and it is a real blend at the border, not a hard edge


# --- the subject found piece by piece (grid first) ------------------------------------------------------------------


class PieceFinder:
    """A stand-in subject finder for pieces: records the crops it is asked about and finds a 4 x 4 subject at the top
    left of each crop."""

    def __init__(self) -> None:
        self.crops: list[np.ndarray] = []

    def __call__(self, rgb: np.ndarray) -> np.ndarray:
        self.crops.append(rgb.copy())
        subject = np.zeros(rgb.shape[:2], np.float32)
        subject[2:6, 2:6] = 1
        return subject


GRID_FIRST = Recipe(
    "grid-first",
    [fill(RED, mask="found")],
    size=64,
    regions={"cells": Grid(columns=2, rows=2), "found": Subject(per="cells", context=0)},
)


FINAL_PICTURE = {
    "render": lambda photo, recipe, **k: runner.render(photo, recipe, **k),
    "trace": lambda photo, recipe, **k: runner.trace(photo, recipe, **k)[-1].image,
    "render_all": lambda photo, recipe, **k: runner.render_all(photo, [PLAIN, recipe], **k)[1],
}


@pytest.mark.parametrize("run_it", FINAL_PICTURE.values(), ids=FINAL_PICTURE.keys())
def test_the_finder_is_asked_about_the_crop_of_each_piece_of_the_working_picture_and_the_finds_are_laid_where_found(
    run_it, photo
):
    finder = PieceFinder()
    out = run_it(photo, GRID_FIRST, finders={"subject": Finder("stand-in", finder)})
    working = resize(photo, 64)  # 48 x 64: four pieces of 24 x 32
    assert sorted(crop.tobytes() for crop in finder.crops) == sorted(
        working[y : y + 24, x : x + 32].tobytes() for y in (0, 24) for x in (0, 32)
    )
    expected = working.copy()
    for y in (0, 24):
        for x in (0, 32):
            expected[y + 2 : y + 6, x + 2 : x + 6] = RGB[RED]
    np.testing.assert_array_equal(out, expected)


def test_a_matte_of_the_whole_photo_does_not_stand_in_for_the_finder_of_pieces(photo, subject):
    with pytest.raises(SubjectNotFoundError, match=r"'found'.*cells"):
        runner.render(photo, GRID_FIRST, maps={"subject": subject})
    finder = PieceFinder()
    runner.render(photo, GRID_FIRST, maps={"subject": subject}, finders={"subject": Finder("stand-in", finder)})
    assert [crop.shape for crop in finder.crops] == [(24, 32, 3)] * 4  # the pieces, never the whole photo


@pytest.mark.parametrize("run_it", RUNS.values(), ids=RUNS.keys())
def test_a_subject_found_piece_by_piece_is_refused_without_a_finder(run_it, photo):
    with pytest.raises(SubjectNotFoundError, match=r"'found'.*finder"):
        run_it(photo, GRID_FIRST)


def test_a_subject_found_piece_by_piece_that_no_step_uses_asks_nobody():
    finder = PieceFinder()
    photo = np.zeros((20, 30, 3), np.float32)
    recipe = Recipe("unused", [Step("grade", {"exposure": 0.5})], size=None, regions=GRID_FIRST.regions)
    runner.render(photo, recipe, finders={"subject": Finder("stand-in", finder)})
    assert finder.crops == []


# --- one look at any size: a recipe's pixel values are design pixels, scaled to the size it is rendered at ------------

PICTURE_OF = {
    "render": lambda photo, recipe, **k: runner.render(photo, recipe, **k),
    "trace": lambda photo, recipe, **k: runner.trace(photo, recipe, **k)[-1].image,
    "render_all": lambda photo, recipe, **k: runner.render_all(photo, [recipe], **k)[0],
}


def blur(radius, size=64, **recipe) -> Recipe:
    return Recipe("blur", [Step("gaussian_blur", {"radius": radius})], size=size, **recipe)


@pytest.mark.parametrize("picture_of", PICTURE_OF.values(), ids=PICTURE_OF.keys())
@pytest.mark.parametrize(
    ("designed", "size", "by_hand"),
    [
        pytest.param(blur(4.0), 128, blur(8.0, 128), id="twice the size: twice the radius"),
        pytest.param(blur(4.0), 32, blur(2.0, 32), id="half the size: half the radius"),
        pytest.param(blur(4.0), 64, blur(4.0, 64), id="the recipe's own size: nothing scaled"),
        pytest.param(blur(24.0, None), 64, blur(3.2, 64), id="a recipe without a size is written for 480: 24 x 64/480"),
        pytest.param(
            Recipe("grid", [Step("pixel_grid", {"cell": 4})], size=64),
            128,
            Recipe("grid", [Step("pixel_grid", {"cell": 8})], size=128),
            id="a whole number of pixels",
        ),
        pytest.param(
            Recipe("grid", [Step("pixel_grid", {"cell": 4})], size=64),
            16,
            Recipe("grid", [Step("pixel_grid", {"cell": 3})], size=16),
            id="a whole number stops at its minimum of 3",
        ),
        pytest.param(
            Recipe(
                "driven",
                [Step("gaussian_blur", {"radius": Drive(by="f", start=2.0, end=6.0)})],
                size=64,
                regions={"f": Given()},
            ),
            128,
            Recipe("plain", [Step("gaussian_blur", {"radius": 12.0})], size=128),
            id="both ends of a driven number (a field of 1 takes the end)",
        ),
        pytest.param(
            Recipe(
                "each",
                [Each("g", [Step("gaussian_blur", {"radius": 4.0}), "none"])],
                size=64,
                regions={"g": Grid(columns=2)},
            ),
            128,
            Recipe(
                "each",
                [Each("g", [Step("gaussian_blur", {"radius": 8.0}), "none"])],
                size=128,
                regions={"g": Grid(columns=2)},
            ),
            id="the looks of an each step",
        ),
    ],
)
def test_a_look_at_another_size_has_its_pixel_values_scaled_by_the_ratio_of_the_sizes(
    photo, picture_of, designed, size, by_hand
):
    maps = {"f": np.ones(photo.shape[:2], np.float32)} if "f" in designed.regions else None
    np.testing.assert_array_equal(picture_of(photo, designed, size=size, maps=maps), picture_of(photo, by_hand))


@pytest.mark.parametrize("picture_of", PICTURE_OF.values(), ids=PICTURE_OF.keys())
@pytest.mark.parametrize(
    ("recipe_size", "size"), [(64, 40), (None, 40), (64, 200)], ids=["smaller", "photos own", "larger"]
)
def test_a_render_at_a_size_comes_out_with_that_longest_side(photo, picture_of, recipe_size, size):
    assert max(picture_of(photo, blur(4.0, recipe_size), size=size).shape[:2]) == size


def scene(long_side: int) -> np.ndarray:
    """A smooth, deterministic 3:2 scene of flat blocks and a diagonal gradient, drawn at any resolution."""
    height = long_side * 2 // 3
    y, x = np.mgrid[0:height, 0:long_side].astype(np.float32)
    x, y = x / long_side, y / long_side
    blocks = ((x > 0.3) & (x < 0.6) & (y > 0.15) & (y < 0.5)).astype(np.float32)
    return np.stack([x, blocks, (x + y) / 2], axis=-1).astype(np.float32)


def disc(long_side: int) -> np.ndarray:
    """A hard-edged disc of 0.15 of the long side in radius, left of the centre, as a subject matte."""
    height = long_side * 2 // 3
    y, x = np.mgrid[0:height, 0:long_side].astype(np.float32)
    return (np.hypot(x - 0.4 * long_side, y - 0.33 * long_side) < 0.15 * long_side).astype(np.float32)


@pytest.mark.parametrize("picture_of", PICTURE_OF.values(), ids=PICTURE_OF.keys())
@pytest.mark.parametrize(
    "step",
    [Step("gaussian_blur", {"radius": 16.0}), Step("drop_shadow", {"offset": 34.0, "angle": 45.0, "blur": 12.0})],
    ids=["a blur radius", "a shadow's offset and blur"],
)
def test_a_recipe_without_a_size_gives_the_same_look_on_a_photo_of_any_size(picture_of, step):
    recipe = Recipe("anysize", [step], size=None)

    def render_at(long_side: int) -> np.ndarray:
        return picture_of(scene(long_side), recipe, maps={"subject": disc(long_side)})

    small, large = render_at(480), render_at(1920)
    assert np.abs(resize(large, 480) - small).mean() < 0.002  # the same look, up to resampling
    assert np.abs(small - scene(480)).mean() > 0.01  # and the look is there to compare


def test_a_recipe_with_a_size_keeps_it_as_the_size_its_pixel_values_are_written_for():
    own = Recipe("own", [Step("gaussian_blur", {"radius": 6.0})], size=960)
    unsized = Recipe("unsized", [Step("gaussian_blur", {"radius": 3.0})], size=None)  # 3 x 960/480 = 6
    np.testing.assert_array_equal(runner.render(scene(1920), own), runner.render(resize(scene(1920), 960), unsized))


@pytest.mark.parametrize("effect_name", ["gaussian_blur", "pixel_grid"])
def test_a_recipe_without_a_size_renders_a_photo_far_smaller_than_480(effect_name):
    photo = scene(64)
    out = runner.render(photo, Recipe("tiny", [Step(effect_name)], size=None))
    assert out.shape == photo.shape
    assert np.isfinite(out).all()


@pytest.mark.parametrize("picture_of", PICTURE_OF.values(), ids=PICTURE_OF.keys())
@pytest.mark.parametrize("size", [0, -64, 2.5, True, "64"], ids=["zero", "negative", "fraction", "bool", "text"])
def test_a_size_that_is_not_a_whole_number_of_pixels_is_refused_naming_it(photo, picture_of, size):
    with pytest.raises(RecipeError, match="size"):
        picture_of(photo, blur(4.0), size=size)


@pytest.mark.parametrize(
    "step",
    [
        pytest.param(Step("gaussian_blur", {"radius": 6.0}), id="a blur"),
        pytest.param(Step("bloom", {"radius": 24.0}), id="a bloom"),
        pytest.param(Step("sharpen", {"radius": 30.0}), id="a local contrast"),
        pytest.param(
            Step("drop_shadow", {"offset": 22.0, "angle": 56.0, "blur": 24.0}), id="a shadow offset and blurred"
        ),
        pytest.param(Step("pixel_grid", {"cell": 4}), id="a grid of whole pixels"),
    ],
)
def test_the_same_look_at_twice_the_size_is_the_small_picture_in_more_detail(photo, subject, step):
    """The promise of a size: shrunk back, the large render is the small one (to a small mean difference), and far
    closer to it than the same pixel values left unscaled at the large size."""
    maps = {"subject": subject}
    small = runner.render(photo, Recipe("look", (step,), size=64), maps=maps)
    large = runner.render(photo, Recipe("look", (step,), size=64), maps=maps, size=128)
    unscaled = runner.render(photo, Recipe("look", (step,), size=128), maps=maps)
    scaled_error = np.abs(resize(large, 64) - small).mean()
    unscaled_error = np.abs(resize(unscaled, 64) - small).mean()
    assert scaled_error < 0.02
    assert scaled_error * 3 < unscaled_error


@pytest.mark.parametrize(
    ("mask", "expected"),
    [
        pytest.param(
            "subject", lambda photo: photo, id="the subject of nobody selects nothing: the step changes nothing"
        ),
        pytest.param(
            "background", np.zeros_like, id="the background of nobody is the whole picture: all of it is filled"
        ),
    ],
)
def test_when_nobody_is_in_the_picture_the_subject_selects_nothing_and_the_background_everything(photo, mask, expected):
    recipe = Recipe("masked", (Step("gradient", {"colors": ["#000000"]}, mask),), size=None)
    out = runner.render(photo, recipe, maps={"subject": np.zeros(photo.shape[:2], np.float32)})
    np.testing.assert_array_equal(out, expected(photo))


@pytest.mark.parametrize(
    "marks",
    [
        pytest.param(Step("particles", {"density": 3.0, "length": 12.0, "color": "#ffffff"}), id="streaks"),
        pytest.param(Step("particles", {"density": 3.0, "size": 12.0}), id="dots"),
        pytest.param(
            Step("dust_scratches", {"density": 3.0, "scratches": 0, "dark": "#000000", "light": "#ffffff"}), id="dust"
        ),
    ],
)
def test_a_density_puts_the_same_share_of_marks_on_the_picture_at_any_size(marks):
    """Marks grow with the picture (their sizes are in design px) and their count is per design area, so what they
    cover stays the same share of the picture."""
    recipe = Recipe("marks", (marks,), size=240)
    black = np.zeros((190, 240, 3), np.float32)
    covered = [runner.render(black, recipe, size=size).mean() for size in (240, 960)]
    assert covered[1] == pytest.approx(covered[0], rel=0.25)
