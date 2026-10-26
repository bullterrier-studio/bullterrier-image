"""Tests of `core/framing.py`: framing a photo and its maps (the subject, the depth, any map handed in) together, and
cutting the finished picture out.

The oracles are made by hand with numpy: flips, `rot90`, slices and `pad` composed in the documented order.
"""

import numpy as np
import pytest

from bullterrier_image import RecipeError, SubjectNotFoundError
from bullterrier_image.core.framing import Framing

BLUE = (0.0, 0.0, 1.0)


@pytest.fixture
def small() -> tuple[np.ndarray, dict[str, np.ndarray]]:
    """A 4 rows by 6 columns photo and three maps (a subject, a depth and a `star` handed in under a name of its own),
    every pixel of each different and the maps unlike each other, so that one taking another's place shows."""
    values = np.arange(24, dtype=np.float32).reshape(4, 6) / 24
    maps = {"subject": (values + 0.01) / 1.1, "depth": 1 - values, "star": values[::-1] * 0.9}
    return np.stack([values, 1 - values, values / 2], axis=2), maps


@pytest.mark.parametrize(
    ("values", "field"),
    [
        pytest.param({"aspect": 0}, "aspect", id="an aspect of 0"),
        pytest.param({"aspect": 11}, "aspect", id="an aspect past 10"),
        pytest.param({"angle": 361}, "angle", id="an angle past a full turn"),
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


def test_the_default_framing_changes_nothing_and_returns_the_photo_and_maps_themselves(small):
    photo, maps = small
    framed_photo, framed = Framing().apply(photo, maps)
    assert framed_photo is photo
    assert framed.keys() == maps.keys()
    assert all(framed[name] is maps[name] for name in maps)


@pytest.mark.parametrize(
    ("values", "by_hand"),
    [
        pytest.param({"flip": "x"}, lambda a: a[:, ::-1], id="flip x"),
        pytest.param({"flip": "y"}, lambda a: a[::-1], id="flip y"),
        pytest.param({"flip": "both"}, lambda a: a[::-1, ::-1], id="flip both"),
        pytest.param({"angle": 90, "fit": "expand"}, lambda a: np.rot90(a), id="a quarter turn"),
        pytest.param({"angle": -90, "fit": "expand"}, lambda a: np.rot90(a, 3), id="a quarter turn clockwise"),
        pytest.param({"aspect": 1.0}, lambda a: a[:, 1:5], id="a square crop in the middle"),
        pytest.param({"aspect": 1.0, "anchor": "left"}, lambda a: a[:, :4], id="a square crop at the left"),
        pytest.param({"aspect": 1.0, "anchor": "right"}, lambda a: a[:, 2:], id="a square crop at the right"),
        pytest.param({"aspect": 3.0}, lambda a: a[1:3], id="a 3:1 crop in the middle"),
    ],
)
def test_the_photo_and_its_maps_are_framed_as_numpy_frames_them_by_hand(small, values, by_hand):
    photo, maps = small
    framed_photo, framed = Framing(**values).apply(photo, maps)
    np.testing.assert_array_equal(framed_photo, by_hand(photo))
    assert framed.keys() == maps.keys()
    for name, layer in maps.items():
        np.testing.assert_array_equal(framed[name], by_hand(layer), err_msg=name)
    alone_photo, alone = Framing(**values).apply(photo, {"star": maps["star"]})  # a map needs no other beside it
    np.testing.assert_array_equal(alone_photo, by_hand(photo))
    np.testing.assert_array_equal(alone["star"], by_hand(maps["star"]))


def test_a_border_is_a_share_of_the_shorter_side_in_the_fill_and_is_zero_in_every_map(small):
    photo, maps = small  # the shorter side is 4 px: 0.5 is 2 px, 0.25 is 1 px
    framed_photo, framed = Framing(pad_top=0.5, pad_left=0.25, fill="#0000ff").apply(photo, maps)
    assert framed_photo.shape == (6, 7, 3)
    np.testing.assert_array_equal(framed_photo[2:, 1:], photo)
    np.testing.assert_array_equal(framed_photo[:2], np.tile(np.array(BLUE, np.float32), (2, 7, 1)))
    np.testing.assert_array_equal(framed_photo[:, :1], np.tile(np.array(BLUE, np.float32), (6, 1, 1)))
    for name, layer in maps.items():
        np.testing.assert_array_equal(framed[name][2:, 1:], layer, err_msg=name)
        assert framed[name][:2].max() == 0, name
        assert framed[name][:, :1].max() == 0, name


def test_the_steps_run_in_the_order_flip_turn_crop_pad(small):
    photo, maps = small
    framing = Framing(flip="x", angle=90, fit="expand", aspect=1.0, anchor="top", pad_bottom=0.5, fill="#0000ff")
    framed_photo, framed = framing.apply(photo, maps)
    turned = np.rot90(photo[:, ::-1])  # 6 rows by 4 columns
    cropped = turned[:4]  # a square at the top
    by_hand = np.concatenate([cropped, np.tile(np.array(BLUE, np.float32), (2, 4, 1))])  # half of 4 px below
    np.testing.assert_array_equal(framed_photo, by_hand)
    for name, layer in maps.items():
        expected = np.concatenate([np.rot90(layer[:, ::-1])[:4], np.zeros((2, 4), np.float32)])
        np.testing.assert_array_equal(framed[name], expected, err_msg=name)


def test_a_turn_by_an_angle_carries_a_map_with_the_photo(photo, subject):
    framed_photo, framed = Framing(angle=30, fit="expand").apply(photo, {"subject": subject})
    assert framed["subject"].shape == framed_photo.shape[:2]
    # the disc is turned about the middle of the photo, so it stays in the middle of the larger picture
    rows, columns = np.nonzero(framed["subject"] >= 0.5)
    assert (rows.mean(), columns.mean()) == pytest.approx(
        ((framed["subject"].shape[0] - 1) / 2, (framed["subject"].shape[1] - 1) / 2), abs=1
    )
    assert framed["subject"].sum() == pytest.approx(subject.sum(), rel=0.02)


def test_a_turn_that_expands_the_picture_leaves_its_corners_zero_in_every_map(photo):
    ones = np.ones(photo.shape[:2], np.float32)
    _, framed = Framing(angle=30, fit="expand").apply(photo, {"subject": ones, "depth": ones, "star": ones})
    for name, layer in framed.items():
        assert (layer[0, 0], layer[0, -1], layer[-1, 0], layer[-1, -1]) == (0, 0, 0, 0), name
        assert layer[74, 79] == pytest.approx(1), name  # the middle of the 148 by 159 picture is the photo's


def test_a_turn_by_an_angle_crops_to_the_largest_rectangle_of_the_photos_aspect_unless_it_expands(photo):
    # 128 by 96 turned by 30 degrees: the scale is 1/((128/96) sin 30 + cos 30) = 0.6525, so 83 by 62 px
    assert Framing(angle=30).apply(photo, {})[0].shape == (62, 83, 3)
    # expanded: 128 cos 30 + 96 sin 30 = 158.9 columns, 128 sin 30 + 96 cos 30 = 147.1 rows
    assert Framing(angle=30, fit="expand").apply(photo, {})[0].shape == (148, 159, 3)


def test_framing_without_maps_frames_the_photo_alone(small):
    photo, _ = small
    framed_photo, framed = Framing(aspect=1.0, flip="y").apply(photo, {})
    np.testing.assert_array_equal(framed_photo, photo[::-1, 1:5])
    assert framed == {}


def test_framing_never_modifies_its_inputs_or_returns_a_view_of_them(small):
    photo, maps = small
    given = {"photo": photo, **maps}
    before = {name: array.copy() for name, array in given.items()}
    framed_photo, framed = Framing(flip="x", aspect=1.0).apply(photo, maps)
    for name, out in {"photo": framed_photo, **framed}.items():
        np.testing.assert_array_equal(given[name], before[name], err_msg=name)
        assert not np.shares_memory(out, given[name]), name


# --- a crop around the subject --------------------------------------------------------------------------------------


def test_a_crop_around_the_subject_holds_it_with_the_margin_all_round(photo, subject):
    # the disc's pixels at one half or more are 59 rows by 59 columns (radius 29.5 about the pixel (48, 64)): a tenth of
    # 59 is 6 px of margin, so the square crop is 71 px with the subject 6 px from every edge
    framed_photo, framed = Framing(anchor="subject", aspect=1.0, margin=0.1).apply(photo, {"subject": subject})
    assert framed_photo.shape[:2] == (71, 71)
    rows, columns = np.nonzero(framed["subject"] >= 0.5)
    assert [int(rows.min()), int(columns.min()), int(70 - rows.max()), int(70 - columns.max())] == [6, 6, 6, 6]
    assert framed["subject"].sum() == pytest.approx(subject.sum())  # none of the subject is cut off
    np.testing.assert_array_equal(framed_photo, photo[13:84, 29:100])


def test_a_crop_around_the_subject_without_an_aspect_is_its_box(photo, subject):
    framed_photo, framed = Framing(anchor="subject").apply(photo, {"subject": subject})
    assert framed_photo.shape[:2] == (59, 59)
    np.testing.assert_array_equal(framed_photo, photo[19:78, 35:94])
    assert framed["subject"].max() == 1.0


def test_a_crop_around_a_subject_that_is_not_there_is_an_error(photo):
    with pytest.raises(SubjectNotFoundError, match="no subject"):
        Framing(anchor="subject", aspect=1.0).apply(photo, {"subject": np.zeros(photo.shape[:2], np.float32)})


def test_a_crop_around_the_subject_follows_the_subject_map_never_another(photo, subject):
    nearest_top_left = np.zeros(photo.shape[:2], np.float32)
    nearest_top_left[:10, :10] = 1
    maps = {"depth": nearest_top_left, "star": nearest_top_left, "subject": subject}  # the subject is not first
    framed_photo, framed = Framing(anchor="subject").apply(photo, maps)
    np.testing.assert_array_equal(framed_photo, photo[19:78, 35:94])
    np.testing.assert_array_equal(framed["star"], nearest_top_left[19:78, 35:94])
    with pytest.raises(SubjectNotFoundError, match="subject map"):
        Framing(anchor="subject").apply(photo, {"depth": nearest_top_left})


# --- the cut --------------------------------------------------------------------------------------------------------


@pytest.mark.parametrize(
    ("values", "needs_subject"),
    [
        pytest.param({}, False, id="nothing"),
        pytest.param({"aspect": 1.0, "anchor": "top"}, False, id="a crop on a side"),
        pytest.param({"anchor": "subject"}, True, id="a crop around the subject"),
        pytest.param({"transparent": "background"}, True, id="the subject as the alpha"),
        pytest.param({"transparent": "circle"}, False, id="a circle"),
        pytest.param({"transparent": "rounded"}, False, id="rounded corners"),
    ],
)
def test_which_framings_need_the_subject(values, needs_subject):
    assert Framing(**values).needs_subject is needs_subject


def test_a_circle_cut_is_opaque_inside_the_inscribed_circle_and_clear_outside(small):
    photo = np.tile(np.array([0.2, 0.4, 0.6], np.float32), (20, 30, 1))
    cut = Framing(transparent="circle").cut(photo, None)
    assert cut.shape == (20, 30, 4)
    np.testing.assert_array_equal(cut[..., :3], photo)
    ys, xs = np.mgrid[0:20, 0:30]
    distance = np.hypot(ys + 0.5 - 10, xs + 0.5 - 15)  # from the middle, the circle's radius being 10
    assert (cut[..., 3][distance < 9] == 1).all()
    assert (cut[..., 3][distance > 11] == 0).all()


def test_a_background_cut_has_the_subject_map_as_its_alpha(photo, subject):
    cut = Framing(transparent="background").cut(photo, subject)
    np.testing.assert_array_equal(cut[..., 3], subject)
    np.testing.assert_array_equal(cut[..., :3], photo)


def test_rounded_corners_are_clear_at_the_corners_and_opaque_in_the_middle_of_each_side():
    photo = np.full((40, 60, 3), 0.5, np.float32)
    cut = Framing(transparent="rounded", corner_radius=0.25).cut(photo, None)  # 0.25 of the 40 px side: 10 px
    alpha = cut[..., 3]
    assert [alpha[0, 0], alpha[0, -1], alpha[-1, 0], alpha[-1, -1]] == [0, 0, 0, 0]
    assert [alpha[0, 30], alpha[-1, 30], alpha[20, 0], alpha[20, -1]] == [1, 1, 1, 1]
    assert alpha[10, 10] == 1  # where the arc of the corner starts
    assert alpha[3, 3] == 1  # inside the arc: a corner 15 px round would have cut it


def test_a_framing_that_does_not_cut_has_nothing_to_cut(small):
    with pytest.raises(ValueError, match="not `transparent`"):
        Framing().cut(small[0], None)
