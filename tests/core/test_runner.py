"""Tests of rendering a recipe through `bullterrier_image.recipe`: the subject and the depth are given as a map or as a
`Finder`, and every step, region, drive, framing and size behaves as the recipe says."""

import math
from contextlib import nullcontext

import numpy as np
import pytest
from PIL import Image

import bullterrier_image
from bullterrier_image import (
    DepthNotFoundError,
    InvalidImageError,
    MapNotFoundError,
    NonFiniteImageError,
    RecipeError,
    SubjectNotFoundError,
)
from bullterrier_image import recipe as facade
from bullterrier_image.core import bridge
from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.pieces import Drive
from bullterrier_image.core.registry import effect
from bullterrier_image.core.steps import Each, Recipe, Step
from bullterrier_image.imageio import resize
from bullterrier_image.recipe import Band, Finder, Framing, Given, Grid, Pick, Shards, Subject

MASKED = Recipe((Step("gradient", params={"colors": ["#000000"]}, mask="background"),), name="masked", size=None)
PLAIN = Recipe((Step("grade", params={"exposure": 0.5}),), name="plain", size=None)


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
    "render": lambda photo, recipe, **k: facade.render(photo, recipe, **k),
    "trace": lambda photo, recipe, **k: facade.trace(photo, recipe, **k),
    "render_all": lambda photo, recipe, **k: facade.render_all(photo, [PLAIN, recipe], **k),
}


@pytest.mark.parametrize("run", RUNS.values(), ids=RUNS.keys())
def test_the_finder_is_called_exactly_once_per_render(run, photo, subject):
    finder = CountingFinder(subject)
    run(photo, MASKED, finders={"subject": Finder("stand-in", finder)})
    assert len(finder.seen) == 1  # once for a render, a trace, or all the recipes of render_all


def test_render_all_asks_once_however_many_recipes_need_the_subject(photo, subject):
    finder = CountingFinder(subject)
    assert len(facade.render_all(photo, [MASKED, PLAIN, MASKED], finders={"subject": Finder("stand-in", finder)})) == 3
    assert len(finder.seen) == 1


def test_the_finder_is_not_called_when_nothing_needs_the_subject_or_the_matte_is_given(photo, subject):
    finder = CountingFinder(subject)
    facade.render(photo, PLAIN, finders={"subject": Finder("stand-in", finder)})
    facade.render_all(photo, [PLAIN, PLAIN], finders={"subject": Finder("stand-in", finder)})
    facade.render(photo, MASKED, maps={"subject": subject}, finders={"subject": Finder("stand-in", finder)})
    assert finder.seen == []


def test_the_finders_matte_is_the_one_the_recipe_uses(photo, subject):
    found = facade.render(photo, MASKED, finders={"subject": Finder("stand-in", CountingFinder(subject))})
    np.testing.assert_array_equal(found, facade.render(photo, MASKED, maps={"subject": subject}))
    assert not np.array_equal(found, facade.render(photo, MASKED, maps={"subject": 1.0 - subject}))


def test_the_finder_sees_the_photo_as_given_before_it_is_framed_or_scaled(photo, subject):
    finder = CountingFinder(subject)
    small = Recipe(MASKED.steps, name="small", size=64, frame=Framing(aspect=1.0))
    assert facade.render(photo, small, finders={"subject": Finder("stand-in", finder)}).shape == (64, 64, 3)
    assert finder.seen == [(96, 128, 3)]  # not the 64 px square it is rendered as


WITH_SUBJECT = {
    "a given map": lambda subject: {"maps": {"subject": subject}},
    "a finder": lambda subject: {"finders": {"subject": Finder("stand-in", CountingFinder(subject))}},
}


@pytest.mark.parametrize("given", WITH_SUBJECT.values(), ids=WITH_SUBJECT.keys())
def test_a_matte_of_another_size_than_the_photo_is_refused_naming_both_sizes(photo, subject, given):
    with pytest.raises(SubjectNotFoundError, match=r"subject is \(48, 64\).*\(96, 128\)"):
        facade.render(photo, MASKED, **given(subject[::2, ::2]))


@pytest.mark.parametrize("run", RUNS.values(), ids=RUNS.keys())
def test_a_finder_that_finds_nothing_is_an_error_naming_it(run, photo):
    """A finder returning None (instead of raising) would otherwise make `trace` drop the subject stage silently."""
    broken = Finder("stand-in", lambda rgb: None)  # type: ignore[arg-type,return-value]  # broken on purpose
    with pytest.raises(SubjectNotFoundError, match="finder of 'subject' returned None"):
        run(photo, MASKED, finders={"subject": broken})


# --- the photo: checked once, where it enters ------------------------------------------------------------------------

PUBLIC_RUNS = {
    "render": lambda photo, recipe: bullterrier_image.render(photo, recipe),
    "trace": lambda photo, recipe: bullterrier_image.trace(photo, recipe)[-1].image,
    "render_all": lambda photo, recipe: bullterrier_image.render_all(photo, [recipe])[0],
}


@pytest.mark.parametrize("run", PUBLIC_RUNS.values(), ids=PUBLIC_RUNS.keys())
@pytest.mark.parametrize(
    ("dtype", "top"),
    [
        pytest.param(np.uint8, 255, id="8-bit levels are divided by 255"),
        pytest.param(np.uint16, 65535, id="16-bit levels are divided by 65535"),
    ],
)
def test_a_photo_of_whole_levels_renders_as_its_levels_over_the_types_largest(run, dtype, top):
    levels = np.arange(4 * 6 * 3).reshape(4, 6, 3).astype(dtype) * (top // 71)  # 0 .. 71 x top // 71, spread out
    as_floats = (levels / np.float64(top)).astype(np.float32)  # the oracle: the level over the type's largest
    np.testing.assert_array_equal(run(levels, PLAIN), run(as_floats, PLAIN))


@pytest.mark.parametrize("run", PUBLIC_RUNS.values(), ids=PUBLIC_RUNS.keys())
def test_a_photo_of_float64_renders_as_the_same_values_in_float32(run, photo):
    np.testing.assert_array_equal(run(photo.astype(np.float64), PLAIN), run(photo, PLAIN))


def _with(value: float, at: tuple[int, int, int] = (2, 3, 1)) -> np.ndarray:
    """A grey 4 x 6 float32 photo with one value set."""
    photo = np.full((4, 6, 3), 0.5, np.float32)
    photo[at] = value
    return photo


@pytest.mark.parametrize(
    ("photo", "error", "complaint"),
    [
        pytest.param(_with(np.nan), NonFiniteImageError, "row 2, column 3", id="NaN names the pixel"),
        pytest.param(_with(np.inf), NonFiniteImageError, "row 2, column 3", id="infinity names the pixel"),
        pytest.param(_with(1.5), InvalidImageError, r"max 1\.5", id="above 1 names the largest"),
        pytest.param(_with(-0.25), InvalidImageError, r"min -0\.25", id="below 0 names the least"),
        pytest.param(
            _with(1.0000001),
            InvalidImageError,
            r"max 1\.0000001192\d*: 1\.0000001192\d* is 1\.19\d*e-07 above 1",
            id="a float32 a hair above 1 shows the value and by how much",
        ),
        pytest.param(
            np.full((4, 6, 3), 1 + 1e-7),
            InvalidImageError,
            r"max 1\.0000001192\d*.*e-07 above 1",
            id="a float64 a hair above 1: as the float32 the effects would see",
        ),
        pytest.param(
            _with(-1e-7),
            InvalidImageError,
            r"min -1\.0\d*e-07, max 0\.5: -1\.0\d*e-07 is 1\.0\d*e-07 below 0",
            id="a hair below 0",
        ),
        pytest.param(np.zeros((4, 6), np.float32), InvalidImageError, r"np\.dstack", id="greyscale names the fix"),
        pytest.param(np.zeros((4, 6, 4), np.float32), InvalidImageError, "alpha", id="RGBA names the fix"),
        pytest.param(np.zeros((4, 6, 2), np.float32), InvalidImageError, r"\(4, 6, 2\)", id="two channels: the shape"),
        pytest.param(np.zeros((0, 6, 3), np.float32), InvalidImageError, "at least one pixel", id="no rows"),
        pytest.param(np.zeros((4, 0, 3), np.float32), InvalidImageError, "at least one pixel", id="no columns"),
        pytest.param(np.zeros((4, 6, 1), np.float32), InvalidImageError, r"one channel.*np\.repeat", id="one channel"),
        pytest.param(np.zeros((1, 4, 6, 3), np.float32), InvalidImageError, r"batch.*picture\[0\]", id="a batch"),
        pytest.param(np.zeros((4, 6, 3), np.uint32), InvalidImageError, "uint32", id="uint32 is refused, not divided"),
        pytest.param(np.zeros((4, 6, 3), np.uint64), InvalidImageError, "uint64", id="uint64 is refused, not divided"),
        pytest.param(np.zeros((4, 6, 3), bool), InvalidImageError, "bool", id="bool is no colour level"),
        pytest.param(np.zeros((4, 6, 3), np.int16), InvalidImageError, "int16", id="a signed type is refused"),
        pytest.param(Image.new("RGB", (6, 4)), InvalidImageError, r"np\.asarray", id="a Pillow image names the fix"),
        pytest.param([[[0.5] * 3] * 6] * 4, InvalidImageError, r"np\.asarray", id="a list names the fix"),
    ],
)
def test_a_photo_that_is_no_picture_is_refused_naming_what_was_found_and_the_fix(photo, error, complaint):
    with pytest.raises(error, match=complaint):
        bullterrier_image.render(photo, PLAIN)


@pytest.mark.parametrize("run", PUBLIC_RUNS.values(), ids=PUBLIC_RUNS.keys())
def test_every_way_of_running_a_recipe_refuses_a_photo_that_is_no_picture(run):
    with pytest.raises(NonFiniteImageError, match="row 2, column 3"):
        run(_with(np.nan), PLAIN)


@pytest.mark.parametrize("shape", [(4, 6, 1), (1, 4, 6, 3)], ids=["one channel", "a batch"])
def test_the_shape_hint_fits_the_shape_it_is_not_always_greyscale(shape):
    with pytest.raises(InvalidImageError) as raised:
        bullterrier_image.render(np.zeros(shape, np.float32), PLAIN)
    assert "dstack" not in str(raised.value)


KEPT_AS_IS = {
    "a float32 photo": lambda photo: photo,
    "an 8-bit photo": lambda photo: (photo * 255).astype(np.uint8),
    "a float64 photo": lambda photo: photo.astype(np.float64),
}
UNCHANGED = {
    "a step that mixes nothing in": Recipe([Step("grade", params={"exposure": 0.5}, opacity=0.0)], size=None),
}


@pytest.mark.parametrize("run", PUBLIC_RUNS.values(), ids=PUBLIC_RUNS.keys())
@pytest.mark.parametrize("recipe", UNCHANGED.values(), ids=UNCHANGED.keys())
@pytest.mark.parametrize("make", KEPT_AS_IS.values(), ids=KEPT_AS_IS.keys())
def test_a_render_never_modifies_the_callers_photo_and_the_result_shares_no_memory_with_it(photo, make, recipe, run):
    given = make(photo)
    before = given.copy()
    result = run(given, recipe)
    np.testing.assert_array_equal(given, before)
    assert not np.shares_memory(result, given)


@pytest.mark.parametrize("run", PUBLIC_RUNS.values(), ids=PUBLIC_RUNS.keys())
def test_a_misaligned_photo_renders_as_the_aligned_one_does(photo, misaligned, run):
    expected = run(photo, PLAIN)
    np.testing.assert_array_equal(run(misaligned(photo), PLAIN), expected)
    assert not np.array_equal(expected, photo)  # the effect did something, so the comparison is not vacuous


def test_a_photo_at_both_ends_of_the_range_is_a_picture():
    photo = _with(1.0)
    photo[0, 0, 0] = 0.0
    assert bullterrier_image.render(photo, PLAIN).shape == (4, 6, 3)


# --- depth: a second map, given or found like the subject ------------------------------------------------------------

RAMP = np.arange(96 * 128, dtype=np.float32).reshape(96, 128) / (96 * 128 - 1)  # every pixel different: a move shows
WITH_DEPTH = {
    "a given map": lambda depth: {"maps": {"depth": depth}},
    "a finder": lambda depth: {"finders": {"depth": Finder("stand-in depth", CountingFinder(depth))}},
}


def deep(**keys) -> Recipe:
    """A recipe of one step of `depth_probe`, which paints the depth map it is given (needs the `depth_probe`)."""
    return Recipe([Step("depth_probe")], name="deep", **{"size": None, **keys})


FRONT = Recipe(  # paints red only the front quarter of the depth scale
    [Step("gradient", params={"colors": ["#ff0000"]}, mask="near")],
    name="front",
    size=None,
    regions={"near": Band(of="depth", low=0.75, feather=0)},
)
NEEDING_DEPTH = {  # a recipe that needs depth, and what the error for no depth names
    "an effect that takes it": (deep, "depth_probe"),
    "a region made from it": (lambda: FRONT, "'near'"),
}


@pytest.mark.parametrize("given", WITH_DEPTH.values(), ids=WITH_DEPTH.keys())
def test_an_effect_that_takes_depth_is_given_the_depth_map(photo, depth_probe, given):
    np.testing.assert_array_equal(facade.render(photo, deep(), **given(RAMP))[..., 0], RAMP)


@pytest.mark.parametrize("given", WITH_DEPTH.values(), ids=WITH_DEPTH.keys())
@pytest.mark.parametrize(
    ("wrong", "size"),
    [pytest.param(RAMP[::2, ::2], r"\(48, 64\)", id="half the height and width")],
)
@pytest.mark.parametrize("recipe", [needing for needing, _ in NEEDING_DEPTH.values()], ids=NEEDING_DEPTH.keys())
def test_a_depth_map_of_another_size_than_the_photo_is_refused_naming_both_sizes(
    photo, depth_probe, given, wrong, size, recipe
):
    with pytest.raises(DepthNotFoundError, match=size + r".*\(96, 128\)"):
        facade.render(photo, recipe(), **given(wrong))


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
    "the depth, found": (FRONT, WITH_DEPTH["a finder"], DepthNotFoundError),
}


@pytest.mark.parametrize(("layer", "complaint"), MAP_VALUES)
@pytest.mark.parametrize(("recipe", "given", "error"), EITHER_MAP.values(), ids=EITHER_MAP.keys())
def test_a_map_must_be_finite_and_within_0_and_1(photo, layer, complaint, recipe, given, error):
    if complaint is None:
        assert facade.render(photo, recipe, **given(layer)).shape == photo.shape
        return
    with pytest.raises(error, match=r"^(subject|depth) has .*" + complaint):
        facade.render(photo, recipe, **given(layer))


@pytest.mark.parametrize("run", RUNS.values(), ids=RUNS.keys())
def test_a_depth_finder_that_finds_nothing_is_an_error_naming_it(run, photo, depth_probe):
    broken = Finder("stand-in depth", lambda rgb: None)  # type: ignore[arg-type,return-value]  # broken on purpose
    with pytest.raises(DepthNotFoundError, match="finder of 'depth' returned None"):
        run(photo, deep(), finders={"depth": broken})


# --- a given map: any name a recipe declares, handed in or found like the subject and the depth -----------------------

STAR_RED = Recipe(  # paints red where the `star` map is
    [Step("gradient", params={"colors": ["#ff0000"]}, mask="star")],
    name="star-red",
    size=None,
    regions={"star": Given()},
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
    np.testing.assert_array_equal(facade.render(photo, STAR_RED, **given(STAR)), red_where(STAR, photo))


@pytest.mark.parametrize("run", RUNS.values(), ids=RUNS.keys())
def test_a_recipe_that_uses_a_given_map_that_nobody_gave_is_refused_naming_the_map_and_both_ways(run, photo):
    with pytest.raises(MapNotFoundError, match="'star'") as raised:
        run(photo, STAR_RED)
    message = str(raised.value)
    assert f"'{STAR_RED.name}'" in message
    assert "maps={'star'" in message
    assert "finders={'star'" in message


@pytest.mark.parametrize("keyword", ["maps", "finders"])
@pytest.mark.parametrize("run", RUNS.values(), ids=RUNS.keys())
def test_a_name_that_is_no_map_of_the_recipe_is_refused_listing_the_maps_it_has(run, photo, keyword):
    value = STAR if keyword == "maps" else Finder("stand-in", CountingFinder(STAR))
    with pytest.raises(RecipeError, match=rf"`{keyword}` names 'strar', which is no map") as raised:
        run(photo, STAR_RED, **{keyword: {"strar": value}})
    assert all(name in str(raised.value) for name in ("subject", "depth", "star"))


def test_a_map_of_the_wrong_size_is_refused_naming_it_even_when_no_step_uses_it(photo):
    with pytest.raises(MapNotFoundError, match=r"star is \(48, 64\).*\(96, 128\)"):
        facade.render(photo, STAR_RED, maps={"star": STAR[::2, ::2]})
    with pytest.raises(DepthNotFoundError, match=r"depth is \(48, 64\).*\(96, 128\)"):
        facade.render(photo, PLAIN, maps={"depth": RAMP[::2, ::2]})


def test_a_finder_of_a_map_that_answers_none_is_an_error_naming_the_map(photo):
    broken = Finder("stand-in star", lambda rgb: None)  # type: ignore[arg-type,return-value]  # broken on purpose
    with pytest.raises(MapNotFoundError, match="finder of 'star' returned None"):
        facade.render(photo, STAR_RED, finders={"star": broken})


# --- light: a field made from the picture the steps work on ----------------------------------------------------------

LUMA = np.asarray([0.2126, 0.7152, 0.0722])  # Rec. 709, written here again so that the test does not use the code's
DARK = Recipe(  # paints red where the picture is no brighter than half
    [Step("gradient", params={"colors": ["#ff0000"]}, mask="dark")],
    name="dark",
    size=None,
    regions={"dark": Band(of="light", low=0, high=0.5, feather=0)},
)


def red_where_dark(picture: np.ndarray) -> np.ndarray:
    """`picture` with red where its brightness (by the weights above, in float64) is at most a half."""
    return np.where((picture @ LUMA <= 0.5)[..., None], PAINTED["fill"], picture)


def test_a_recipe_that_uses_light_needs_no_map_from_anyone(photo):
    assert bridge.maps_needed(DARK) == frozenset()
    np.testing.assert_array_equal(facade.render(photo, DARK), red_where_dark(photo))  # no maps, no finders


def test_nobody_can_hand_in_light_it_is_made_here(photo):
    with pytest.raises(RecipeError, match="`maps` names 'light'"):
        facade.render(photo, DARK, maps={"light": np.zeros(photo.shape[:2], np.float32)})


# --- a driven parameter: a field varies it pixel by pixel ------------------------------------------------------------


class BrightenParams(EffectParams):
    """The probe that is driven: a gain and a bias, both smooth."""

    gain: float = param(1.0, unit="ratio", min=0, max=4, drivable=True, doc="Multiplies every value.")
    bias: float = param(0.0, unit="fraction", min=-1, max=1, drivable=True, doc="Added after the gain.")
    steps: int = param(1, unit="count", min=1, max=9, doc="Not drivable: a whole number.")


@pytest.fixture
def brighten(throwaway_effects):
    """Registers `brighten_probe`: clip(rgb x gain + bias)."""

    @effect(category="light", params=BrightenParams)
    def brighten_probe(rgb, params):
        """Multiplies by a gain and adds a bias."""
        return np.clip(rgb * params.gain + params.bias, 0, 1).astype(np.float32)


def driven(by: str = "f", levels: int = 5, **drives: tuple[float, float]) -> Recipe:
    """A recipe of one `brighten_probe` step whose parameters vary by the given map `f` (or a built-in field)."""
    values = {name: Drive(by=by, start=start, end=end, levels=levels) for name, (start, end) in drives.items()}
    return Recipe([Step("brighten_probe", params=values)], name="driven", size=None, regions={"f": Given()})


def plain(**values: float) -> Recipe:
    return Recipe([Step("brighten_probe", params=values)], name="plain", size=None)


def field_of(photo: np.ndarray, value: float) -> np.ndarray:
    return np.full(photo.shape[:2], value, np.float32)


@pytest.mark.parametrize("level", range(5))
def test_a_constant_field_gives_the_plain_effect_at_that_levels_value(photo, brighten, level):
    # 0.5 + 2 x level / 4 by hand: the levels run 0.5, 1, 1.5, 2, 2.5
    expected = facade.render(photo, plain(gain=0.5 + 2.0 * level / 4))
    got = facade.render(photo, driven(gain=(0.5, 2.5)), maps={"f": field_of(photo, level / 4)})
    np.testing.assert_allclose(got, expected, atol=1e-6, rtol=0)


def test_a_field_between_two_levels_mixes_their_renders_by_the_distance_to_each(photo, brighten):
    # levels 0, 0.5, 1 of the gain; a field of 0.25 is half way from the first to the second
    first, second = facade.render(photo, plain(gain=0.0)), facade.render(photo, plain(gain=2.0))
    got = facade.render(photo, driven(levels=3, gain=(0.0, 4.0)), maps={"f": field_of(photo, 0.25)})
    np.testing.assert_allclose(got, 0.5 * first + 0.5 * second, atol=1e-6, rtol=0)


def test_a_driven_step_is_traced_by_name_and_its_last_stage_is_the_render(photo, brighten):
    recipe = driven(gain=(0.5, 2.5))
    stages = facade.trace(photo, recipe, maps={"f": field_of(photo, 0.5)})
    assert stages[-1].label == "1. brighten_probe(gain=by f 0.5..2.5)"
    np.testing.assert_array_equal(stages[-1].image, facade.render(photo, recipe, maps={"f": field_of(photo, 0.5)}))


def test_a_drive_by_a_map_nobody_gave_is_refused_like_any_use_of_it(photo, brighten):
    with pytest.raises(MapNotFoundError, match="'f'"):
        facade.render(photo, driven(gain=(0.5, 2.5)))


# --- framing: the photo and its subject are framed together, before the steps ------------------------------------


def test_render_all_finds_the_subject_once_for_framed_and_plain_recipes(photo, subject):
    finder = CountingFinder(subject)
    framed = Recipe(PLAIN.steps, name="framed", size=None, frame=Framing(anchor="subject"))
    results = facade.render_all(photo, [PLAIN, framed], finders={"subject": Finder("stand-in", finder)})
    assert [r.shape[:2] for r in results] == [(96, 128), (59, 59)]
    assert len(finder.seen) == 1


# --- transparency: the picture is cut out after the last step -------------------------------------------------------


def test_render_all_cuts_only_the_recipes_that_ask_for_it(photo):
    round_ = Recipe(PLAIN.steps, name="round", size=None, frame=Framing(transparent="circle"))
    results = facade.render_all(photo, [PLAIN, round_])
    assert [r.shape[2] for r in results] == [3, 4]


class ScratchParams(EffectParams):
    """The settings of the probe effects that need one parameter."""

    copies: int = param(4, unit="count", min=1, max=8, doc="Unused: a probe ignores it.")


# --- regions: `each` gives every piece of a partition its look; `mask` limits a step to a selection -----------------

RED, GREEN, BLUE, WHITE = "#ff0000", "#00ff00", "#0000ff", "#ffffff"
RGB = {RED: (1.0, 0.0, 0.0), GREEN: (0.0, 1.0, 0.0), BLUE: (0.0, 0.0, 1.0), WHITE: (1.0, 1.0, 1.0)}


def fill(color: str, **keys) -> Step:
    return Step("gradient", params={"colors": [color]}, **keys)


def columns(count: int) -> dict[str, Grid | Pick | Shards]:
    return {"cols": Grid(columns=count, rows=1)}


def run(photo, steps, regions, **keys):
    return facade.render(photo, Recipe(steps, name="regions", size=None, regions=regions), **keys)


def test_a_look_takes_its_effects_default_opacity_unless_it_gives_one(photo, throwaway_effects):
    @effect(category="utilities", params=ScratchParams, opacity=0.4)
    def faint_white(rgb, params):
        """Paints white, shown at 0.4 unless a step says otherwise."""
        return np.ones_like(rgb)

    out = run(
        photo, [Each("cols", looks=[Step("faint_white"), Step("faint_white", opacity=1.0)], feather=0)], columns(2)
    )
    np.testing.assert_allclose(out[:, :64], photo[:, :64] * 0.6 + 0.4, atol=1e-6)
    assert (out[:, 64:] == 1).all()


# --- regions and the subject: found at most once, and only when a region in use needs it ------------------------------


def test_a_declared_region_nobody_uses_costs_nothing_and_does_not_look_for_the_subject(photo, subject):
    finder = CountingFinder(subject)
    unused = {"glass": Shards(fit="subject"), "head": Pick(of="glass")}
    out = run(
        photo,
        [fill(RED, mask="head")] if False else [Step("grade", params={"exposure": 0.5})],
        unused,
        finders={"subject": Finder("stand-in", finder)},
    )
    assert finder.seen == []
    np.testing.assert_array_equal(out, run(photo, [Step("grade", params={"exposure": 0.5})], {}))


def test_a_region_in_use_asks_for_the_subject_once_however_many_steps_use_it(photo, subject):
    finder = CountingFinder(subject)
    regions = {"glass": Shards(count=5, fit="subject"), "head": Pick(of="glass", pieces=[0])}
    steps = [fill(RED, mask="head"), Each("glass", looks=[fill(GREEN), None]), fill(BLUE, mask="subject")]
    run(photo, steps, regions, finders={"subject": Finder("stand-in", finder)})
    assert finder.seen == [(96, 128, 3)]  # the photo as given


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
    recipe = Recipe([Step("grade", params={"exposure": 0.2}, mask="head")], name="traced", size=None, regions=TRACED)
    return facade.trace(photo, recipe, **keys)


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


def test_trace_labels_an_each_step_with_its_partition_and_looks(photo):
    each = Each("panes", looks=[fill(RED), None], mask="head")
    labels = [stage.label for stage in facade.trace(photo, Recipe([each], name="t", size=None, regions=TRACED))]
    assert labels[-1].startswith("1. each panes [gradient(colors=['#ff0000']), none]")


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
    [fill(RED, mask="found")],
    name="grid-first",
    size=64,
    regions={"cells": Grid(columns=2, rows=2), "found": Subject(per="cells", context=0)},
)


FINAL_PICTURE = {
    "render": lambda photo, recipe, **k: facade.render(photo, recipe, **k),
    "trace": lambda photo, recipe, **k: facade.trace(photo, recipe, **k)[-1].image,
    "render_all": lambda photo, recipe, **k: facade.render_all(photo, [PLAIN, recipe], **k)[1],
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


def test_a_subject_found_piece_by_piece_that_no_step_uses_asks_nobody():
    finder = PieceFinder()
    photo = np.zeros((20, 30, 3), np.float32)
    recipe = Recipe([Step("grade", params={"exposure": 0.5})], name="unused", size=None, regions=GRID_FIRST.regions)
    facade.render(photo, recipe, finders={"subject": Finder("stand-in", finder)})
    assert finder.crops == []


# --- one look at any size: a recipe's pixel values are design pixels, scaled to the size it is rendered at ------------

PICTURE_OF = {
    "render": lambda photo, recipe, **k: facade.render(photo, recipe, **k),
    "trace": lambda photo, recipe, **k: facade.trace(photo, recipe, **k)[-1].image,
    "render_all": lambda photo, recipe, **k: facade.render_all(photo, [recipe], **k)[0],
}


def blur(radius, size=64, **recipe) -> Recipe:
    return Recipe([Step("gaussian_blur", params={"radius": radius})], name="blur", size=size, **recipe)


@pytest.mark.parametrize("picture_of", PICTURE_OF.values(), ids=PICTURE_OF.keys())
@pytest.mark.parametrize("size", [0, -64, 2.5, True, "64"], ids=["zero", "negative", "fraction", "bool", "text"])
def test_a_size_that_is_not_a_whole_number_of_pixels_is_refused_naming_it(photo, picture_of, size):
    with pytest.raises(RecipeError, match="size"):
        picture_of(photo, blur(4.0), size=size)


def ratio_looks() -> dict[str, Recipe]:
    """Recipes of the probe: plain, driven by a field, and as the looks of an `Each`."""
    drive = Drive(by="f", start=0.0, end=1.0)
    return {
        "a plain step": Recipe([Step("ratio_probe", params={})], name="plain", size=64),
        "a driven step": Recipe(
            [Step("ratio_probe", params={"level": drive})], name="driven", size=64, regions={"f": Given()}
        ),
        "the looks of an each": Recipe(
            [Each("g", looks=[Step("ratio_probe", params={}), Step("ratio_probe", params={})], feather=0)],
            name="each",
            size=64,
            regions={"g": Grid(columns=2)},
        ),
    }


@pytest.fixture
def ratio_probe(throwaway_effects):
    """`ratio_probe`: a picture of the `size_ratio` it was given, as that over 10 in every value."""

    class RatioParams(EffectParams):
        """The probe that reads the size ratio."""

        level: float = param(0.5, unit="fraction", min=0, max=1, drivable=True, doc="Drivable, and not used.")

    @effect(category="light", params=RatioParams)
    def ratio_probe(rgb, params, *, size_ratio=1.0):
        """Fills the picture with its size ratio over 10."""
        return np.full_like(rgb, size_ratio / 10)


@pytest.mark.parametrize("look", ["a plain step", "a driven step", "the looks of an each"])
@pytest.mark.parametrize(("size", "ratio"), [(480, 1.0), (960, 2.0), (240, 0.5)], ids=["as designed", "twice", "half"])
def test_an_effect_that_declares_size_ratio_gets_working_px_per_design_px(photo, ratio_probe, look, size, ratio):
    recipe = ratio_looks()[look]
    maps = {"f": np.full(photo.shape[:2], 0.3, np.float32)} if "f" in recipe.regions else None
    out = facade.render(photo, recipe, size=size, maps=maps)
    np.testing.assert_allclose(out, ratio / 10, atol=1e-6)  # every Drive level and every look gets the same ratio


BANDED = Recipe(
    [Step("grade", params={"exposure": 0.4}), Step("gaussian_blur", params={"radius": 6.0}), Step("vignette")],
    name="banded",
    size=None,
)


def test_a_band_taller_than_the_picture_is_the_whole_render(photo):
    whole = facade.render(photo, BANDED)
    np.testing.assert_array_equal(facade.render(photo, BANDED, band=2**63), whole)


def test_a_banded_render_gives_the_picture_of_the_whole_render(photo):
    whole = facade.render(photo, BANDED)
    np.testing.assert_array_equal(facade.render(photo, BANDED, band=16), whole)
    np.testing.assert_array_equal(facade.render_all(photo, [BANDED], band=16)[0], whole)


@pytest.mark.parametrize(
    "band",
    [0, -3, 2.5, True, "8", 2**64, 2**70],
    ids=["zero", "negative", "fraction", "bool", "text", "past the largest row count", "far past it"],
)
def test_a_band_that_is_no_number_of_rows_is_refused_naming_it(photo, band):
    with pytest.raises(RecipeError, match="band"):
        facade.render(photo, BANDED, band=band)
