"""Behaviour of the `drop_shadow` effect."""

import numpy as np
import pytest

from bullterrier_image.core import registry
from bullterrier_image.core.steps import Step

NAME = "drop_shadow"
GREY = 0.8
RED, BLUE = "#ff0000", "#0000ff"


def grey(height: int = 32, width: int | None = None) -> np.ndarray:
    return np.full((height, width or height, 3), GREY, np.float32)


def one_pixel(row: int = 10, col: int = 12, size: int = 32) -> np.ndarray:
    subject = np.zeros((size, size), np.float32)
    subject[row, col] = 1.0
    return subject


def block(cols: slice, height: int = 12, width: int = 40) -> np.ndarray:
    """A subject 4 rows tall in the middle and the given columns wide."""
    subject = np.zeros((height, width), np.float32)
    subject[4:8, cols] = 1.0
    return subject


def stripe(out: np.ndarray, cols: slice) -> np.ndarray:
    return out[5, cols]


@pytest.mark.parametrize(
    ("offset", "angle", "lands_at"),
    [
        pytest.param(5, 0, [(10, 17)], id="0 degrees is to the right"),
        pytest.param(5, 90, [(15, 12)], id="90 degrees is down"),
        pytest.param(5, 180, [(10, 7)], id="180 degrees is to the left"),
        pytest.param(5, 270, [(5, 12)], id="270 degrees is up"),
        pytest.param(3 * 2**0.5, 45, [(13, 15)], id="45 degrees is the lower right; the offset is along the ray"),
        pytest.param(0.5, 90, [(10, 12), (11, 12)], id="half a pixel shares the shadow between two pixels"),
    ],
)
def test_the_shadow_lands_offset_away_along_the_angle_and_nowhere_else(
    offset: float, angle: float, lands_at: list[tuple[int, int]]
) -> None:
    """With no blur and a black shadow, a shadow pixel is `picture x (1 - coverage)`; the coverage of a pixel moved by
    a whole number of pixels is 1 there, and of one moved by half a pixel is 1/2 on two pixels."""
    out = registry.apply(NAME, grey(), subject=one_pixel(), offset=offset, angle=angle, blur=0.0)
    expected = grey()
    for row, col in lands_at:
        expected[row, col] = GREY * (1 - 1 / len(lands_at))
    expected[10, 12] = GREY  # the pixel the subject itself covers is never changed
    np.testing.assert_allclose(out, expected, atol=1e-6)


def test_the_subject_is_never_changed(photo: np.ndarray, subject: np.ndarray) -> None:
    for kwargs in ({}, {"offset": 3, "blur": 5.0}, {"offset": 0, "color": "#ff0000"}, {"copies": 5, "offset": 6.0}):
        out = registry.apply(NAME, photo, subject=subject, **kwargs)
        np.testing.assert_array_equal(out[subject == 1], photo[subject == 1])


@pytest.mark.parametrize(
    ("kwargs", "expected"),
    [
        pytest.param({}, [0.0, 0.0, 0.0], id="the default shadow is black where it is full"),
        pytest.param({"color": RED}, [1.0, 0.0, 0.0], id="a coloured shadow is that colour"),
        pytest.param({"color": RED, "end_color": BLUE}, [1.0, 0.0, 0.0], id="with one copy only color is used"),
    ],
)
def test_where_the_shadow_is_full_it_replaces_the_picture_with_its_colour(kwargs: dict, expected: list[float]) -> None:
    out = registry.apply(NAME, grey(), subject=one_pixel(), offset=3 * 2**0.5, angle=45, blur=0.0, **kwargs)
    np.testing.assert_allclose(out[13, 15], expected, atol=1e-6)


def test_blur_spreads_the_shadow_as_a_gaussian_of_that_sigma_and_keeps_its_amount() -> None:
    """Over a flat picture a black shadow removes `GREY x coverage`; the coverage of one pixel blurred by sigma is the
    2-D Gaussian, which peaks at 1 / (2 pi sigma^2) and sums to 1."""
    sigma = 2.0
    out = registry.apply(NAME, grey(64), subject=one_pixel(30, 30, 64), offset=6 * 2**0.5, angle=45, blur=sigma)
    removed = GREY - out[..., 0]
    assert removed[36, 36] == pytest.approx(GREY / (2 * np.pi * sigma**2), abs=1e-4)
    assert removed[36, 36 + 2] == pytest.approx(GREY * np.exp(-4 / (2 * sigma**2)) / (2 * np.pi * sigma**2), abs=1e-4)
    assert removed.sum() == pytest.approx(GREY, abs=1e-3)


def test_every_copy_is_blurred_by_blur() -> None:
    """Two copies of one pixel, 6 px apart: each is its own Gaussian, so the picture loses the amount of both."""
    sigma = 1.5
    out = registry.apply(NAME, grey(64), subject=one_pixel(30, 10, 64), copies=2, offset=8.0, angle=0, blur=sigma)
    removed = GREY - out[..., 0]
    assert removed[30, 18] == pytest.approx(GREY / (2 * np.pi * sigma**2), abs=1e-3)
    assert removed[30, 26] == pytest.approx(GREY / (2 * np.pi * sigma**2), abs=1e-3)


def test_each_copy_is_the_silhouette_moved_one_more_offset_and_coloured_from_color_toward_end_color() -> None:
    """A 2 px subject, three copies 4 px apart, to the right: copy k covers the 2 px that start 4k px on. The colours
    step evenly from red (the copy by the subject) to blue (the farthest): red, purple, blue."""
    out = registry.apply(
        NAME,
        grey(12, 40),
        subject=block(slice(5, 7)),
        copies=3,
        offset=4.0,
        angle=0,
        blur=0.0,
        color=RED,
        end_color=BLUE,
    )
    for cols, expected in (
        (slice(0, 5), GREY),  # left of the subject
        (slice(5, 7), GREY),  # the subject itself
        (slice(7, 9), GREY),  # between the subject and the first copy
        (slice(9, 11), [[1.0, 0.0, 0.0]] * 2),
        (slice(11, 13), GREY),
        (slice(13, 15), [[0.5, 0.0, 0.5]] * 2),
        (slice(15, 17), GREY),
        (slice(17, 19), [[0.0, 0.0, 1.0]] * 2),
        (slice(19, 40), GREY),
    ):
        np.testing.assert_allclose(stripe(out, cols), expected, atol=1e-6)


def test_a_copy_nearer_the_subject_is_in_front_of_the_farther_ones() -> None:
    """Copies 2 px apart of a 6 px subject overlap; where they do the nearest shows. The subject is cols 10..15, so
    copy 1 is cols 12..17, copy 2 cols 14..19, copy 3 cols 16..21: 16..17 belong to copy 1, 18..19 to copy 2 and
    20..21 to copy 3."""
    out = registry.apply(
        NAME,
        grey(12, 40),
        subject=block(slice(10, 16)),
        copies=3,
        offset=2.0,
        angle=0,
        blur=0.0,
        color=RED,
        end_color=BLUE,
    )
    np.testing.assert_allclose(stripe(out, slice(16, 18)), [[1.0, 0.0, 0.0]] * 2, atol=1e-6)
    np.testing.assert_allclose(stripe(out, slice(18, 20)), [[0.5, 0.0, 0.5]] * 2, atol=1e-6)
    np.testing.assert_allclose(stripe(out, slice(20, 22)), [[0.0, 0.0, 1.0]] * 2, atol=1e-6)


def test_copies_are_laid_over_each_other_by_their_copy_opacity() -> None:
    """Two copies of copy opacity 0.5 over grey: the farther one first, g + 0.5 (blue - g) = (0.4, 0.4, 0.9) where only
    it shows, and the nearer one over that, + 0.5 (red - that) = (0.7, 0.2, 0.45), where both do."""
    out = registry.apply(
        NAME,
        grey(12, 40),
        subject=block(slice(10, 16)),
        copies=2,
        offset=2.0,
        angle=0,
        blur=0.0,
        color=RED,
        end_color=BLUE,
        copy_opacity=0.5,
    )
    np.testing.assert_allclose(stripe(out, slice(16, 18)), [[0.7, 0.2, 0.45]] * 2, atol=1e-6)
    np.testing.assert_allclose(stripe(out, slice(18, 20)), [[0.4, 0.4, 0.9]] * 2, atol=1e-6)


def test_a_single_copy_at_a_copy_opacity_is_the_shadow_at_that_opacity() -> None:
    """One black copy at 0.25 over grey leaves 0.75 of the picture where it is full."""
    out = registry.apply(NAME, grey(), subject=one_pixel(), offset=3, angle=0, blur=0.0, copy_opacity=0.25)
    np.testing.assert_allclose(out[10, 15], GREY * 0.75, atol=1e-6)


@pytest.mark.parametrize(
    ("angle", "step", "offset"),
    [
        pytest.param(0, (0, 1), 3.0, id="0 degrees is to the right"),
        pytest.param(90, (1, 0), 3.0, id="90 degrees is down"),
        pytest.param(180, (0, -1), 3.0, id="180 degrees is to the left"),
        pytest.param(270, (-1, 0), 3.0, id="270 degrees is up"),
        pytest.param(45, (1, 1), 3 * 2**0.5, id="45 degrees is the lower right; the offset is along the ray"),
    ],
)
def test_the_copies_go_in_the_direction_of_the_angle(angle: float, step: tuple[int, int], offset: float) -> None:
    subject = np.zeros((21, 21), np.float32)
    subject[10, 10] = 1.0
    out = registry.apply(
        NAME,
        grey(21),
        subject=subject,
        copies=2,
        offset=offset,
        angle=angle,
        blur=0.0,
        color=RED,
        end_color=BLUE,
    )
    expected = grey(21)
    expected[10 + 3 * step[0], 10 + 3 * step[1]] = [1.0, 0.0, 0.0]
    expected[10 + 6 * step[0], 10 + 6 * step[1]] = [0.0, 0.0, 1.0]
    np.testing.assert_allclose(out, expected, atol=1e-6)


def test_a_soft_edge_keeps_its_share_of_the_subject_however_many_copies_lie_behind() -> None:
    """Column 16 is half subject; copies 1 px apart cover it all with the red of the copy by the subject, so it is
    half picture, half red, whether there is one copy or four."""
    subject = block(slice(10, 16))
    subject[4:8, 16] = 0.5
    for copies in (1, 4):
        out = registry.apply(
            NAME, grey(12, 40), subject=subject, copies=copies, offset=1.0, angle=0, blur=0.0, color=RED
        )
        np.testing.assert_allclose(out[5, 16], [0.9, 0.4, 0.4], atol=1e-6)


@pytest.mark.parametrize(
    ("copies", "blur"),
    [pytest.param(1, 0.0, id="a shadow"), pytest.param(3, 0.0, id="a stack"), pytest.param(2, 1.5, id="soft copies")],
)
def test_a_subject_cut_by_the_border_casts_its_copies_without_a_gap(copies: int, blur: float) -> None:
    """The subject fills rows 2 to 4 of the six left columns, so it goes on beyond the border. The picture must show
    what a picture 10 px wider on the left, with the subject carried on into it, shows."""
    subject = np.zeros((20, 12), np.float32)
    subject[2:5, :6] = 1.0
    wide = np.zeros((20, 22), np.float32)
    wide[2:5, :16] = 1.0
    kwargs = {"copies": copies, "offset": 4 * 2**0.5, "angle": 45, "blur": blur, "color": RED, "end_color": BLUE}
    out = registry.apply(NAME, grey(20, 12), subject=subject, **kwargs)
    out_wide = registry.apply(NAME, grey(20, 22), subject=wide, **kwargs)
    np.testing.assert_allclose(out, out_wide[:, 10:], atol=1e-6)
    assert (out[8, 0] != GREY).all()  # in the copy of the part out of the picture


def test_copy_opacity_zero_changes_nothing_and_a_stack_changes_the_picture(
    photo: np.ndarray, subject: np.ndarray
) -> None:
    stack = registry.apply(NAME, photo, subject=subject, copies=5, offset=6.0, angle=30)
    assert not np.array_equal(stack, photo)
    np.testing.assert_array_equal(registry.apply(NAME, photo, subject=subject, copy_opacity=0.0), photo)


def test_without_a_subject_there_is_no_shadow(photo: np.ndarray) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo), photo)


@pytest.mark.parametrize(
    ("fade", "expected"),
    [
        pytest.param(0.0, [0.0, 0.0, 0.0], id="no fade: every copy as opaque as the first"),
        pytest.param(0.5, [0.0, 0.2, 0.4], id="half: opacity falls in a straight line to half at the farthest copy"),
        pytest.param(1.0, [0.0, 0.4, GREY], id="full: the farthest copy vanishes"),
    ],
)
def test_fade_lowers_each_copys_opacity_in_a_straight_line_along_the_copies(fade: float, expected: list[float]) -> None:
    """Three black copies of a 2 px subject, 4 px apart, over grey leave `GREY x (1 - opacity)` of the picture in copy
    k: the opacity is 1 at the first copy, `1 - fade / 2` at the second and `1 - fade` at the third."""
    out = registry.apply(
        NAME, grey(12, 40), subject=block(slice(5, 7)), copies=3, offset=4.0, angle=0, blur=0.0, fade=fade
    )
    np.testing.assert_allclose([out[5, 9, 0], out[5, 13, 0], out[5, 17, 0]], expected, atol=1e-6)


def test_a_single_copy_does_not_fade() -> None:
    out = registry.apply(NAME, grey(), subject=one_pixel(), offset=3, angle=0, blur=0.0, copy_opacity=0.5, fade=1.0)
    np.testing.assert_allclose(out[10, 15], GREY * 0.5, atol=1e-6)


def test_the_long_shadow_preset_is_a_solid_shadow_that_fades_out_over_300_px() -> None:
    """A 40 px wide band that fills the picture's height, the preset's shadow turned to the right: dark beside the
    subject, lighter at every pixel farther on, gone 300 px past the subject's edge."""
    subject = np.zeros((6, 380), np.float32)
    subject[:, 10:50] = 1.0
    out = registry.apply(NAME, grey(6, 380), subject=subject, **{**Step(NAME, preset="long_shadow").params, "angle": 0})
    removed = GREY - out[3, :, 0]
    assert removed[50] > 0.9 * GREY  # beside the subject
    assert (np.diff(removed[50:]) <= 1e-6).all()  # never darker farther out
    assert removed[290] > 0.2 * GREY  # still there 240 px out
    assert removed[330] < 0.1 * GREY  # nearly gone 280 px out
    np.testing.assert_allclose(removed[352:], 0.0, atol=1e-6)  # nothing past the last copy
