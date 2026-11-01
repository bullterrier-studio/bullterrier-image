"""Tests of `Framing`: how a recipe frames a photo and its maps (the subject, the depth, any map handed in) together
before its steps, and cuts the finished picture out after them.

The oracles are made by hand with numpy: flips, `rot90`, slices and `pad` composed in the documented order. A photo is
framed by rendering a recipe whose one step changes nothing; a map is framed by rendering a recipe whose one step paints
that map, so what is seen is what the steps would be given.
"""

from collections.abc import Callable

import numpy as np
import pytest

from bullterrier_image import RecipeError, SubjectNotFoundError
from bullterrier_image.recipe import Framing, Given, Recipe, Step, render

BLUE = (0.0, 0.0, 1.0)

Framed = Callable[[Framing, np.ndarray, dict[str, np.ndarray]], tuple[np.ndarray, dict[str, np.ndarray]]]


@pytest.fixture
def framed(throwaway_effects, register_probe, depth_probe) -> Framed:
    """`framed(framing, photo, maps)`: the photo and each of the maps as the steps of a recipe with that framing are
    given them, at the photo's own size."""
    register_probe("unchanged")

    def frame(framing, photo, maps):
        def recipe(step: Step) -> Recipe:
            given = {name: Given() for name in maps if name not in ("subject", "depth")}
            return Recipe([step], name="framed", size=None, frame=framing, regions=given)

        framed_photo = render(photo, recipe(Step("unchanged")), maps=maps)
        painted = {
            name: render(photo, recipe(Step("depth_probe", params={"field": name})), maps=maps)[..., 0] for name in maps
        }
        return framed_photo, painted

    return frame


@pytest.fixture
def small() -> tuple[np.ndarray, dict[str, np.ndarray]]:
    """A 4 rows by 6 columns photo and three maps (a subject, a depth and a `star` handed in under a name of its own),
    every pixel of each different and the maps unlike each other, so that one taking another's place shows."""
    values = np.arange(24, dtype=np.float32).reshape(4, 6) / 24
    maps = {"subject": (values + 0.01) / 1.1, "depth": 1 - values, "star": values[::-1] * 0.9}
    return np.stack([values, 1 - values, values / 2], axis=2), maps


@pytest.fixture
def star(depth_probe) -> Callable[[Framing], Recipe]:
    """`star(framing)`: a recipe with that framing that needs the given map `star` and changes nothing."""
    return lambda framing: Recipe(
        [Step("depth_probe", params={"field": "star"})], size=None, frame=framing, regions={"star": Given()}
    )


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


def test_the_default_framing_changes_nothing(small, framed):
    photo, maps = small
    framed_photo, painted = framed(Framing(), photo, maps)
    np.testing.assert_array_equal(framed_photo, photo)
    for name, layer in maps.items():
        np.testing.assert_array_equal(painted[name], layer, err_msg=name)


@pytest.mark.parametrize(
    ("values", "by_hand"),
    [
        pytest.param({"flip": "x"}, lambda a: a[:, ::-1], id="flip x"),
        pytest.param({"flip": "y"}, lambda a: a[::-1], id="flip y"),
        pytest.param({"flip": "both"}, lambda a: a[::-1, ::-1], id="flip both"),
        pytest.param({"angle": -90, "fit": "expand"}, lambda a: np.rot90(a), id="a quarter turn counter-clockwise"),
        pytest.param({"angle": 90, "fit": "expand"}, lambda a: np.rot90(a, 3), id="a quarter turn clockwise"),
        pytest.param({"aspect": 1.0}, lambda a: a[:, 1:5], id="a square crop in the middle"),
        pytest.param({"aspect": 1.0, "anchor": "left"}, lambda a: a[:, :4], id="a square crop at the left"),
        pytest.param({"aspect": 1.0, "anchor": "right"}, lambda a: a[:, 2:], id="a square crop at the right"),
        pytest.param({"aspect": 3.0}, lambda a: a[1:3], id="a 3:1 crop in the middle"),
    ],
)
def test_the_photo_and_its_maps_are_framed_as_numpy_frames_them_by_hand(small, framed, values, by_hand):
    photo, maps = small
    framed_photo, painted = framed(Framing(**values), photo, maps)
    np.testing.assert_array_equal(framed_photo, by_hand(photo))
    for name, layer in maps.items():
        np.testing.assert_array_equal(painted[name], by_hand(layer), err_msg=name)
    _, alone = framed(Framing(**values), photo, {"star": maps["star"]})  # a map needs no other beside it
    np.testing.assert_array_equal(alone["star"], by_hand(maps["star"]))


def test_a_border_is_a_share_of_the_shorter_side_in_the_fill_and_is_zero_in_every_map(small, framed):
    photo, maps = small  # the shorter side is 4 px: 0.5 is 2 px, 0.25 is 1 px
    framed_photo, painted = framed(Framing(pad_top=0.5, pad_left=0.25, fill="#0000ff"), photo, maps)
    assert framed_photo.shape == (6, 7, 3)
    np.testing.assert_array_equal(framed_photo[2:, 1:], photo)
    np.testing.assert_array_equal(framed_photo[:2], np.tile(np.array(BLUE, np.float32), (2, 7, 1)))
    np.testing.assert_array_equal(framed_photo[:, :1], np.tile(np.array(BLUE, np.float32), (6, 1, 1)))
    for name, layer in maps.items():
        np.testing.assert_array_equal(painted[name][2:, 1:], layer, err_msg=name)
        assert painted[name][:2].max() == 0, name
        assert painted[name][:, :1].max() == 0, name


def test_the_steps_run_in_the_order_flip_turn_crop_pad(small, framed):
    photo, maps = small
    framing = Framing(flip="x", angle=-90, fit="expand", aspect=1.0, anchor="top", pad_bottom=0.5, fill="#0000ff")
    framed_photo, painted = framed(framing, photo, maps)
    turned = np.rot90(photo[:, ::-1])  # 6 rows by 4 columns
    cropped = turned[:4]  # a square at the top
    by_hand = np.concatenate([cropped, np.tile(np.array(BLUE, np.float32), (2, 4, 1))])  # half of 4 px below
    np.testing.assert_array_equal(framed_photo, by_hand)
    for name, layer in maps.items():
        expected = np.concatenate([np.rot90(layer[:, ::-1])[:4], np.zeros((2, 4), np.float32)])
        np.testing.assert_array_equal(painted[name], expected, err_msg=name)


def test_a_turn_by_an_angle_carries_a_map_with_the_photo(photo, subject, framed):
    framed_photo, painted = framed(Framing(angle=30, fit="expand"), photo, {"subject": subject})
    assert painted["subject"].shape == framed_photo.shape[:2]
    # the disc is turned about the middle of the photo, so it stays in the middle of the larger picture
    rows, columns = np.nonzero(painted["subject"] >= 0.5)
    assert (rows.mean(), columns.mean()) == pytest.approx(
        ((painted["subject"].shape[0] - 1) / 2, (painted["subject"].shape[1] - 1) / 2), abs=1
    )
    assert painted["subject"].sum() == pytest.approx(subject.sum(), rel=0.02)


def test_a_turn_that_expands_the_picture_leaves_its_corners_zero_in_every_map(photo, framed):
    ones = np.ones(photo.shape[:2], np.float32)
    _, painted = framed(Framing(angle=30, fit="expand"), photo, {"subject": ones, "depth": ones, "star": ones})
    for name, layer in painted.items():
        assert (layer[0, 0], layer[0, -1], layer[-1, 0], layer[-1, -1]) == (0, 0, 0, 0), name
        assert layer[74, 79] == pytest.approx(1), name  # the middle of the 148 by 159 picture is the photo's


def test_a_turn_by_an_angle_crops_to_the_largest_rectangle_of_the_photos_aspect_unless_it_expands(photo, framed):
    # 128 by 96 turned by 30 degrees: the scale is 1/((128/96) sin 30 + cos 30) = 0.6525, so 83 by 62 px
    assert framed(Framing(angle=30), photo, {})[0].shape == (62, 83, 3)
    # expanded: 128 cos 30 + 96 sin 30 = 158.9 columns, 128 sin 30 + 96 cos 30 = 147.1 rows
    assert framed(Framing(angle=30, fit="expand"), photo, {})[0].shape == (148, 159, 3)


# --- a crop around the subject --------------------------------------------------------------------------------------


def test_a_crop_around_the_subject_holds_it_with_the_margin_all_round(photo, subject, framed):
    # the disc's pixels at one half or more are 59 rows by 59 columns (radius 29.5 about the pixel (48, 64)): a tenth of
    # 59 is 6 px of margin, so the square crop is 71 px with the subject 6 px from every edge
    framed_photo, painted = framed(Framing(anchor="subject", aspect=1.0, margin=0.1), photo, {"subject": subject})
    assert framed_photo.shape[:2] == (71, 71)
    rows, columns = np.nonzero(painted["subject"] >= 0.5)
    assert [int(rows.min()), int(columns.min()), int(70 - rows.max()), int(70 - columns.max())] == [6, 6, 6, 6]
    assert painted["subject"].sum() == pytest.approx(subject.sum())  # none of the subject is cut off
    np.testing.assert_array_equal(framed_photo, photo[13:84, 29:100])


def test_a_crop_around_the_subject_without_an_aspect_is_its_box(photo, subject, framed):
    framed_photo, painted = framed(Framing(anchor="subject"), photo, {"subject": subject})
    assert framed_photo.shape[:2] == (59, 59)
    np.testing.assert_array_equal(framed_photo, photo[19:78, 35:94])
    assert painted["subject"].max() == 1.0


def test_a_crop_around_a_subject_that_is_not_there_is_an_error(photo, framed):
    with pytest.raises(SubjectNotFoundError, match="no subject"):
        framed(Framing(anchor="subject", aspect=1.0), photo, {"subject": np.zeros(photo.shape[:2], np.float32)})


def test_a_crop_around_the_subject_follows_the_subject_map_never_another(photo, subject, framed):
    nearest_top_left = np.zeros(photo.shape[:2], np.float32)
    nearest_top_left[:10, :10] = 1
    maps = {"depth": nearest_top_left, "star": nearest_top_left, "subject": subject}  # the subject is not first
    framed_photo, painted = framed(Framing(anchor="subject"), photo, maps)
    np.testing.assert_array_equal(framed_photo, photo[19:78, 35:94])
    np.testing.assert_array_equal(painted["star"], nearest_top_left[19:78, 35:94])


# --- the cut --------------------------------------------------------------------------------------------------------


def test_a_circle_cut_is_opaque_inside_the_inscribed_circle_and_clear_outside(star):
    photo = np.tile(np.array([0.2, 0.4, 0.6], np.float32), (20, 30, 1))
    gray = np.full((20, 30), 0.2, np.float32)
    cut = render(photo, star(Framing(transparent="circle")), maps={"star": gray})
    assert cut.shape == (20, 30, 4)
    np.testing.assert_allclose(cut[..., :3], 0.2)
    ys, xs = np.mgrid[0:20, 0:30]
    distance = np.hypot(ys + 0.5 - 10, xs + 0.5 - 15)  # from the middle, the circle's radius being 10
    assert (cut[..., 3][distance < 9] == 1).all()
    assert (cut[..., 3][distance > 11] == 0).all()


def test_a_background_cut_has_the_subject_map_as_its_alpha(photo, subject, depth_probe):
    cut = render(
        photo,
        Recipe([Step("depth_probe", params={"field": "subject"})], size=None, frame=Framing(transparent="background")),
        maps={"subject": subject},
    )
    np.testing.assert_array_equal(cut[..., 3], subject)
    np.testing.assert_array_equal(cut[..., 0], subject)  # the step painted the subject: the colour is the steps'


def test_rounded_corners_are_clear_at_the_corners_and_opaque_in_the_middle_of_each_side(star):
    photo = np.full((40, 60, 3), 0.5, np.float32)
    gray = np.full((40, 60), 0.5, np.float32)
    cut = render(photo, star(Framing(transparent="rounded", corner_radius=0.25)), maps={"star": gray})  # 10 px
    alpha = cut[..., 3]
    assert [alpha[0, 0], alpha[0, -1], alpha[-1, 0], alpha[-1, -1]] == [0, 0, 0, 0]
    assert [alpha[0, 30], alpha[-1, 30], alpha[20, 0], alpha[20, -1]] == [1, 1, 1, 1]
    assert alpha[10, 10] == 1  # where the arc of the corner starts
    assert alpha[3, 3] == 1  # inside the arc: a corner 15 px round would have cut it


@pytest.mark.parametrize(
    ("values", "has_alpha"),
    [
        pytest.param({}, False, id="no cut"),
        pytest.param({"aspect": 1.0, "anchor": "top"}, False, id="a crop on a side"),
        pytest.param({"transparent": "circle"}, True, id="a circle"),
        pytest.param({"transparent": "rounded"}, True, id="rounded corners"),
    ],
)
def test_only_a_framing_that_cuts_gives_a_picture_with_alpha(small, star, values, has_alpha):
    photo, maps = small
    assert render(photo, star(Framing(**values)), maps=maps).shape[2] == (4 if has_alpha else 3)
