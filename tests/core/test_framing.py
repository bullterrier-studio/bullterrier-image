"""Tests of `core/framing.py`: framing a photo and its subject matte together, and cutting the finished picture out.

The oracles are made by hand with numpy: flips, `rot90`, slices and `pad` composed in the documented order.
"""

import numpy as np
import pytest

from bullterrier_image import RecipeError, SubjectNotFoundError
from bullterrier_image.core.framing import Framing

BLUE = (0.0, 0.0, 1.0)


@pytest.fixture
def small() -> tuple[np.ndarray, np.ndarray]:
    """A 4 rows by 6 columns photo and a matte, every pixel of both different."""
    values = np.arange(24, dtype=np.float32).reshape(4, 6) / 24
    return np.stack([values, 1 - values, values / 2], axis=2), (values + 0.01) / 1.1


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


def test_the_default_framing_changes_nothing_and_returns_the_photo_itself(small):
    photo, matte = small
    framed_photo, framed_matte = Framing().apply(photo, matte)
    assert framed_photo is photo
    assert framed_matte is matte


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
def test_the_photo_and_its_matte_are_framed_as_numpy_frames_them_by_hand(small, values, by_hand):
    photo, matte = small
    framed_photo, framed_matte = Framing(**values).apply(photo, matte)
    np.testing.assert_array_equal(framed_photo, by_hand(photo))
    np.testing.assert_array_equal(framed_matte, by_hand(matte))


def test_a_border_is_a_share_of_the_shorter_side_in_the_fill_and_is_background_in_the_matte(small):
    photo, matte = small  # the shorter side is 4 px: 0.5 is 2 px, 0.25 is 1 px
    framed_photo, framed_matte = Framing(pad_top=0.5, pad_left=0.25, fill="#0000ff").apply(photo, matte)
    assert framed_photo.shape == (6, 7, 3)
    np.testing.assert_array_equal(framed_photo[2:, 1:], photo)
    np.testing.assert_array_equal(framed_photo[:2], np.tile(np.array(BLUE, np.float32), (2, 7, 1)))
    np.testing.assert_array_equal(framed_photo[:, :1], np.tile(np.array(BLUE, np.float32), (6, 1, 1)))
    np.testing.assert_array_equal(framed_matte[2:, 1:], matte)
    assert framed_matte[:2].max() == 0
    assert framed_matte[:, :1].max() == 0


def test_the_steps_run_in_the_order_flip_turn_crop_pad(small):
    photo, matte = small
    framing = Framing(flip="x", angle=90, fit="expand", aspect=1.0, anchor="top", pad_bottom=0.5, fill="#0000ff")
    framed_photo, framed_matte = framing.apply(photo, matte)
    turned = np.rot90(photo[:, ::-1])  # 6 rows by 4 columns
    cropped = turned[:4]  # a square at the top
    by_hand = np.concatenate([cropped, np.tile(np.array(BLUE, np.float32), (2, 4, 1))])  # half of 4 px below
    np.testing.assert_array_equal(framed_photo, by_hand)
    expected_matte = np.concatenate([np.rot90(matte[:, ::-1])[:4], np.zeros((2, 4), np.float32)])
    np.testing.assert_array_equal(framed_matte, expected_matte)


def test_a_turn_by_an_angle_carries_the_matte_with_the_photo(photo, matte):
    framed_photo, framed_matte = Framing(angle=30, fit="expand").apply(photo, matte)
    assert framed_matte.shape == framed_photo.shape[:2]
    # the disc is turned about the middle of the photo, so it stays in the middle of the larger picture
    rows, columns = np.nonzero(framed_matte >= 0.5)
    assert (rows.mean(), columns.mean()) == pytest.approx(
        ((framed_matte.shape[0] - 1) / 2, (framed_matte.shape[1] - 1) / 2), abs=1
    )
    assert framed_matte.sum() == pytest.approx(matte.sum(), rel=0.02)


def test_a_turn_by_an_angle_crops_to_the_largest_rectangle_of_the_photos_aspect_unless_it_expands(photo):
    # 128 by 96 turned by 30 degrees: the scale is 1/((128/96) sin 30 + cos 30) = 0.6525, so 83 by 62 px
    assert Framing(angle=30).apply(photo, None)[0].shape == (62, 83, 3)
    # expanded: 128 cos 30 + 96 sin 30 = 158.9 columns, 128 sin 30 + 96 cos 30 = 147.1 rows
    assert Framing(angle=30, fit="expand").apply(photo, None)[0].shape == (148, 159, 3)


def test_framing_without_a_matte_frames_the_photo_alone(small):
    photo, _ = small
    framed_photo, framed_matte = Framing(aspect=1.0, flip="y").apply(photo, None)
    np.testing.assert_array_equal(framed_photo, photo[::-1, 1:5])
    assert framed_matte is None


def test_framing_never_modifies_its_inputs_or_returns_a_view_of_them(small):
    photo, matte = small
    before = photo.copy(), matte.copy()
    framed_photo, framed_matte = Framing(flip="x", aspect=1.0).apply(photo, matte)
    np.testing.assert_array_equal(photo, before[0])
    np.testing.assert_array_equal(matte, before[1])
    assert not np.shares_memory(framed_photo, photo)
    assert not np.shares_memory(framed_matte, matte)


# --- a crop around the subject --------------------------------------------------------------------------------------


def test_a_crop_around_the_subject_holds_it_with_the_margin_all_round(photo, matte):
    # the disc's pixels at one half or more are 59 rows by 59 columns (radius 29.5 about the pixel (48, 64)): a tenth of
    # 59 is 6 px of margin, so the square crop is 71 px with the subject 6 px from every edge
    framed_photo, framed_matte = Framing(anchor="subject", aspect=1.0, margin=0.1).apply(photo, matte)
    assert framed_photo.shape[:2] == (71, 71)
    rows, columns = np.nonzero(framed_matte >= 0.5)
    assert (rows.min(), columns.min(), 70 - rows.max(), 70 - columns.max()) == (6, 6, 6, 6)
    assert framed_matte.sum() == pytest.approx(matte.sum())  # none of the subject is cut off
    np.testing.assert_array_equal(framed_photo, photo[13:84, 29:100])


def test_a_crop_around_the_subject_without_an_aspect_is_its_box(photo, matte):
    framed_photo, framed_matte = Framing(anchor="subject").apply(photo, matte)
    assert framed_photo.shape[:2] == (59, 59)
    np.testing.assert_array_equal(framed_photo, photo[19:78, 35:94])
    assert framed_matte.max() == 1.0


def test_a_crop_around_a_subject_that_is_not_there_is_an_error(photo):
    with pytest.raises(SubjectNotFoundError, match="no subject"):
        Framing(anchor="subject", aspect=1.0).apply(photo, np.zeros(photo.shape[:2], np.float32))


# --- the cut --------------------------------------------------------------------------------------------------------


@pytest.mark.parametrize(
    ("values", "needs_matte"),
    [
        pytest.param({}, False, id="nothing"),
        pytest.param({"aspect": 1.0, "anchor": "top"}, False, id="a crop on a side"),
        pytest.param({"anchor": "subject"}, True, id="a crop around the subject"),
        pytest.param({"transparent": "background"}, True, id="the subject as the alpha"),
        pytest.param({"transparent": "circle"}, False, id="a circle"),
        pytest.param({"transparent": "rounded"}, False, id="rounded corners"),
    ],
)
def test_which_framings_need_the_subject(values, needs_matte):
    assert Framing(**values).needs_matte is needs_matte


def test_a_circle_cut_is_opaque_inside_the_inscribed_circle_and_clear_outside(small):
    photo = np.tile(np.array([0.2, 0.4, 0.6], np.float32), (20, 30, 1))
    cut = Framing(transparent="circle").cut(photo, None)
    assert cut.shape == (20, 30, 4)
    np.testing.assert_array_equal(cut[..., :3], photo)
    ys, xs = np.mgrid[0:20, 0:30]
    distance = np.hypot(ys + 0.5 - 10, xs + 0.5 - 15)  # from the middle, the circle's radius being 10
    assert (cut[..., 3][distance < 9] == 1).all()
    assert (cut[..., 3][distance > 11] == 0).all()


def test_a_background_cut_has_the_subject_matte_as_its_alpha(photo, matte):
    cut = Framing(transparent="background").cut(photo, matte)
    np.testing.assert_array_equal(cut[..., 3], matte)
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
