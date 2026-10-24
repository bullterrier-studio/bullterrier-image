"""Tests of `ops/frame.py`: the pixel geometry of framing (flip, rotate, crop box, pad, circle and rounded alpha).

The oracles are hand-made: numpy's own flips and `rot90` for the exact moves, hand arithmetic for sizes and boxes, a
dot or a flat colour for what rotation does to content.
"""

import numpy as np
import pytest

from bullterrier_image import SubjectNotFoundError
from bullterrier_image.ops import frame

RED, BLUE = (1.0, 0.0, 0.0), (0.0, 0.0, 1.0)


def numbered(height: int, width: int, channels: int = 3) -> np.ndarray:
    """A picture whose every pixel is different (and 0..1): the pixel at row r and column c holds (r*width + c)."""
    values = np.arange(height * width, dtype=np.float32).reshape(height, width) / (height * width)
    return np.repeat(values[..., None], channels, axis=2)


def flat(height: int, width: int, colour=RED) -> np.ndarray:
    return np.tile(np.array(colour, np.float32), (height, width, 1))


# --- flip -----------------------------------------------------------------------------------------------------------


@pytest.mark.parametrize(
    ("x", "y", "expected"),
    [
        pytest.param(True, False, lambda a: a[:, ::-1], id="x mirrors left and right"),
        pytest.param(False, True, lambda a: a[::-1], id="y mirrors top and bottom"),
        pytest.param(True, True, lambda a: a[::-1, ::-1], id="both is a half turn"),
        pytest.param(False, False, lambda a: a, id="neither changes nothing"),
    ],
)
def test_flip_is_numpys_flip(x, y, expected):
    picture = numbered(3, 4)
    np.testing.assert_array_equal(frame.flip(picture, x=x, y=y), expected(picture))


# --- rotate ---------------------------------------------------------------------------------------------------------


@pytest.mark.parametrize(
    ("angle", "turns"),
    [(90, 1), (180, 2), (270, 3), (-90, 3), (360, 0), (-270, 1)],
    ids=lambda value: str(value),
)
def test_a_quarter_turn_expanded_moves_every_pixel_exactly_counter_clockwise(angle, turns):
    picture = numbered(2, 3)
    turned = frame.rotate(picture, angle, fit="expand", fill=RED, resample="bicubic")
    np.testing.assert_array_equal(turned, np.rot90(picture, turns))


def test_a_quarter_turn_cropped_keeps_the_original_aspect_around_the_centre():
    # 90 degrees of a 4 wide, 8 tall picture: the turned frame is 8 wide, 4 tall; the largest 1:2 rectangle in it is 2
    # wide and 4 tall, in the middle
    picture = numbered(8, 4)
    turned = frame.rotate(picture, 90, fit="crop", fill=RED, resample="bicubic")
    np.testing.assert_array_equal(turned, np.rot90(picture)[:, 3:5])


@pytest.mark.parametrize(
    ("width", "height", "angle", "fit", "rows", "columns"),
    [
        pytest.param(10, 10, 45, "expand", 15, 15, id="a square on its corner needs side*sqrt(2) = 14.1, rounded up"),
        pytest.param(10, 10, 45, "crop", 7, 7, id="and when cropped keeps side/sqrt(2) = 7.07, rounded down"),
        pytest.param(10, 12, 30, "expand", 16, 15, id="expand: rows w*sin+h*cos = 15.4, columns w*cos+h*sin = 14.7"),
        pytest.param(10, 12, 30, "crop", 8, 6, id="crop keeps the aspect: the scale is 1/(cos+(h/w)sin) = 0.68"),
    ],
)
def test_rotated_sizes(width, height, angle, fit, rows, columns):
    turned = frame.rotate(flat(height, width), angle, fit=fit, fill=BLUE, resample="bilinear")
    assert turned.shape[:2] == (rows, columns)


def test_cropping_after_a_turn_never_shows_the_fill_and_keeps_a_flat_colour():
    turned = frame.rotate(flat(40, 60), 23, fit="crop", fill=BLUE, resample="bicubic")
    np.testing.assert_allclose(turned, flat(*turned.shape[:2]), atol=1e-5)


def test_expanding_shows_the_fill_in_the_corners_and_the_picture_in_the_middle():
    turned = frame.rotate(flat(40, 60), 30, fit="expand", fill=BLUE, resample="bilinear")
    height, width = turned.shape[:2]
    np.testing.assert_allclose(turned[0, 0], BLUE)
    np.testing.assert_allclose(turned[-1, -1], BLUE)
    np.testing.assert_allclose(turned[height // 2, width // 2], RED, atol=1e-5)
    # the picture's pixels still cover the picture's area (the edge pixels are partly covered, so this is approximate)
    covered = float((turned[..., 0] > 0.5).sum())
    assert covered == pytest.approx(40 * 60, rel=0.03)


def test_a_positive_angle_turns_the_picture_counter_clockwise():
    picture = np.zeros((41, 41, 3), np.float32)
    picture[20, 30] = 1.0  # 10 px right of the centre
    turned = frame.rotate(picture, 30, fit="crop", fill=RED, resample="bilinear")
    centre = np.array(turned.shape[:2]) // 2
    row, column = np.unravel_index(np.argmax(turned[..., 0]), turned.shape[:2])
    # a point 10 px to the right moves to 10*cos(30) = 8.66 right and 10*sin(30) = 5 up
    assert (column - centre[1], centre[0] - row) == pytest.approx((8.66, 5.0), abs=1.0)


@pytest.mark.parametrize("resample", ["nearest", "bilinear", "bicubic"])
def test_a_turn_stays_between_0_and_1(resample):
    picture = np.zeros((30, 30, 3), np.float32)
    picture[10:20, 10:20] = 1.0  # a hard edge, which bicubic overshoots
    turned = frame.rotate(picture, 17, fit="expand", fill=BLUE, resample=resample)
    assert turned.min() >= 0.0
    assert turned.max() <= 1.0


def test_a_channel_the_caller_added_gets_its_own_fill():
    stacked = np.concatenate([flat(20, 20), np.ones((20, 20, 1), np.float32)], axis=2)
    turned = frame.rotate(stacked, 30, fit="expand", fill=(*BLUE, 0.0), resample="bilinear")
    np.testing.assert_allclose(turned[0, 0], (*BLUE, 0.0))
    np.testing.assert_allclose(turned[turned.shape[0] // 2, turned.shape[1] // 2], (*RED, 1.0), atol=1e-5)


# --- pad ------------------------------------------------------------------------------------------------------------


def test_pad_adds_each_side_in_the_colour_and_keeps_the_picture_in_place():
    picture = numbered(2, 3)
    padded = frame.pad(picture, top=1, right=2, bottom=3, left=4, fill=BLUE)
    assert padded.shape == (2 + 1 + 3, 3 + 2 + 4, 3)
    np.testing.assert_array_equal(padded[1:3, 4:7], picture)
    outside = np.ones(padded.shape[:2], bool)
    outside[1:3, 4:7] = False
    np.testing.assert_array_equal(padded[outside], np.tile(np.array(BLUE, np.float32), (int(outside.sum()), 1)))


def test_pad_without_a_size_returns_the_picture_as_it_is():
    picture = numbered(2, 3)
    np.testing.assert_array_equal(frame.pad(picture, top=0, right=0, bottom=0, left=0, fill=BLUE), picture)


# --- the subject's box and the crop box -----------------------------------------------------------------------------


def a_subject(height=40, width=60, top=10, left=20, box_height=8, box_width=16) -> np.ndarray:
    subject = np.zeros((height, width), np.float32)
    subject[top : top + box_height, left : left + box_width] = 1.0
    return subject


@pytest.mark.parametrize(
    ("margin", "box"),
    [
        pytest.param(0.0, (10, 20, 8, 16), id="no margin is the bounding box of the pixels at half or more"),
        pytest.param(0.25, (6, 16, 16, 24), id="a margin is a share of the box's longer side (16 px: 4 px each side)"),
        pytest.param(1.0, (0, 4, 34, 48), id="a margin past the top or left edge stops at it"),
    ],
)
def test_subject_box(margin, box):
    assert tuple(frame.subject_box(a_subject(), margin)) == box


def test_subject_box_margin_past_the_bottom_or_right_edge_stops_at_it():
    assert tuple(frame.subject_box(a_subject(top=32, left=44), 1.0)) == (16, 28, 24, 32)


def test_subject_box_ignores_what_is_below_half_coverage():
    subject = a_subject()
    subject[0, 0] = 0.49
    subject[39, 59] = 0.5
    assert tuple(frame.subject_box(subject, 0.0)) == (10, 20, 30, 40)


def test_an_empty_subject_has_no_box():
    with pytest.raises(SubjectNotFoundError, match="subject"):
        frame.subject_box(np.zeros((4, 4), np.float32), 0.0)


@pytest.mark.parametrize(
    ("shape", "aspect", "anchor", "box"),
    [
        pytest.param((48, 64), 1.0, "center", (0, 8, 48, 48), id="a square from a wide picture, centred"),
        pytest.param((48, 64), 1.0, "left", (0, 0, 48, 48), id="left"),
        pytest.param((48, 64), 1.0, "right", (0, 16, 48, 48), id="right"),
        pytest.param((48, 64), 1.0, "top", (0, 8, 48, 48), id="top has no room to move in a wide picture"),
        pytest.param((64, 48), 1.0, "top", (0, 0, 48, 48), id="top of a tall picture"),
        pytest.param((64, 48), 1.0, "bottom", (16, 0, 48, 48), id="bottom of a tall picture"),
        pytest.param((64, 48), 1.0, "center", (8, 0, 48, 48), id="centre of a tall picture"),
        pytest.param((48, 64), 2.0, "center", (8, 0, 32, 64), id="2:1 from 4:3 keeps the width, centred"),
        pytest.param((48, 64), 4 / 3, "center", (0, 0, 48, 64), id="the aspect it has already: all of it"),
    ],
)
def test_crop_box_of_an_aspect_around_an_anchor(shape, aspect, anchor, box):
    assert tuple(frame.crop_box(shape, aspect, anchor)) == box


@pytest.mark.parametrize(
    ("aspect", "box"),
    [
        pytest.param(None, (10, 20, 8, 16), id="no aspect: exactly the subject's box"),
        pytest.param(1.0, (6, 20, 16, 16), id="a square grows the short side, around the subject's centre"),
        pytest.param(4.0, (10, 12, 8, 32), id="a wider one grows the width, 4*8 = 32, around the centre"),
        pytest.param(0.5, (0, 20, 32, 16), id="a taller one grows the height, kept inside the picture"),
    ],
)
def test_crop_box_around_the_subject_holds_the_subject_box(aspect, box):
    subject = frame.subject_box(a_subject(), 0.0)
    got = frame.crop_around((40, 60), subject, aspect)
    top, left, height, width = got
    assert (top, left, height, width) == box
    assert top <= subject.top
    assert top + height >= subject.top + subject.height
    assert left <= subject.left
    assert left + width >= subject.left + subject.width


def test_crop_box_that_would_leave_the_picture_is_moved_inside_it():
    near_the_edge = frame.subject_box(a_subject(top=0, left=0), 0.0)
    top, left, height, width = frame.crop_around((40, 60), near_the_edge, 1.0)
    assert (top, left) == (0, 0)
    assert (height, width) == (16, 16)


def test_crop_box_cannot_hold_a_subject_wider_than_the_aspect_allows_and_takes_the_most_the_picture_gives():
    wide = frame.subject_box(a_subject(box_height=4, box_width=60, left=0), 0.0)
    _, left, height, width = frame.crop_around((40, 60), wide, 0.5)
    assert (height, width) == (40, 20)
    assert left >= 0
    assert left + width <= 60


# --- circle and rounded corners -------------------------------------------------------------------------------------


def test_circle_alpha_is_one_inside_and_zero_outside_the_inscribed_circle():
    alpha = frame.circle_alpha((20, 20))  # centre (10, 10), radius 10, in pixel-corner coordinates
    ys, xs = np.mgrid[0:20, 0:20]
    distance = np.hypot(xs + 0.5 - 10, ys + 0.5 - 10)  # of each pixel's centre from the middle
    np.testing.assert_array_equal(alpha[distance < 9.0], 1.0)
    np.testing.assert_array_equal(alpha[distance > 11.0], 0.0)
    assert 0.0 < alpha[0, 9] < 1.0  # the edge is one pixel of soft ramp: the top middle pixel is just inside


def test_circle_alpha_of_a_wide_picture_is_the_circle_of_its_shorter_side():
    alpha = frame.circle_alpha((20, 40))
    assert alpha[10, 20] == 1.0
    assert alpha[10, 5] == 0.0  # 15 px left of the middle, the radius being 10
    assert alpha[10, 12] == 1.0
    assert alpha[0, 0] == 0.0


def test_rounded_alpha_cuts_the_corners_and_keeps_the_middle_of_each_side():
    alpha = frame.rounded_alpha((32, 48), 8.0)
    assert alpha.shape == (32, 48)
    assert [alpha[0, 0], alpha[0, -1], alpha[-1, 0], alpha[-1, -1]] == [0.0] * 4
    assert [alpha[0, 24], alpha[-1, 24], alpha[16, 0], alpha[16, -1]] == [1.0] * 4
    assert alpha[8, 8] == 1.0  # where the corner's arc starts to curve


def test_rounded_alpha_with_no_radius_keeps_everything():
    np.testing.assert_array_equal(frame.rounded_alpha((6, 9), 0.0), np.ones((6, 9), np.float32))
