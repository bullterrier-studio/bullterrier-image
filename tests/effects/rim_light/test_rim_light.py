"""Behaviour of the `rim_light` effect."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "rim_light"
BASE = 0.2
WHITE = "#ffffff"


def grey(size: int = 40) -> np.ndarray:
    return np.full((size, size, 3), BASE, np.float32)


def block() -> np.ndarray:
    """A subject of a 20 x 20 square, rows and columns 10..29 of a 40 x 40 picture."""
    subject = np.zeros((40, 40), np.float32)
    subject[10:30, 10:30] = 1.0
    return subject


def screened(light: float) -> float:
    """The grey with `light` (0..1) of white screened over it: 1 - (1 - 0.2)(1 - light), worked out by hand."""
    return 1.0 - (1.0 - BASE) * (1.0 - light)


def lit_at(out: np.ndarray, row: int, col: int) -> float:
    """How much white light reached a pixel: inverts `screened`."""
    return float(1.0 - (1.0 - out[row, col, 0]) / (1.0 - BASE))


def test_amount_0_lights_nothing(photo, subject):
    np.testing.assert_array_equal(registry.apply(NAME, photo, subject=subject, amount=0), photo)


def test_without_a_subject_there_is_nothing_to_light(photo):
    np.testing.assert_array_equal(registry.apply(NAME, photo), photo)


@pytest.mark.parametrize("fill", [0.0, 1.0], ids=["an empty subject", "a subject covering everything"])
def test_a_matte_without_an_edge_lights_nothing(fill, photo):
    subject = np.full(photo.shape[:2], fill, np.float32)
    np.testing.assert_array_equal(registry.apply(NAME, photo, subject=subject), photo)


@pytest.mark.parametrize(
    ("angle", "lit_sides"),
    [
        (0, {"right"}),
        (90, {"top"}),
        (180, {"left"}),
        (-90, {"bottom"}),
    ],
    ids=["from the right", "from above", "from the left", "from below"],
)
def test_only_the_side_facing_the_light_is_lit(angle, lit_sides):
    out = registry.apply(NAME, grey(), subject=block(), angle=angle, width=3, falloff=1, color=WHITE)
    # the middle of each side of the square, on its outermost pixel
    side_pixels = {"right": (20, 29), "top": (10, 20), "left": (20, 10), "bottom": (29, 20)}
    for side, (row, col) in side_pixels.items():
        assert lit_at(out, row, col) == pytest.approx(1.0 if side in lit_sides else 0.0, abs=0.02), side


def test_a_diagonal_light_lights_two_sides_by_the_cosine_of_the_angle_to_their_normal():
    """From the upper right (45 degrees) the top and right normals are 45 degrees off the light: cos 45 = 0.7071."""
    out = registry.apply(NAME, grey(), subject=block(), angle=45, width=3, falloff=1, color=WHITE)
    assert lit_at(out, 10, 20) == pytest.approx(0.7071, abs=0.02)
    assert lit_at(out, 20, 29) == pytest.approx(0.7071, abs=0.02)
    assert lit_at(out, 20, 10) == pytest.approx(0.0, abs=0.02)
    assert lit_at(out, 29, 20) == pytest.approx(0.0, abs=0.02)


@pytest.mark.parametrize(
    ("falloff", "expected"),
    [(1, [1.0, 2 / 3, 1 / 3, 0.0, 0.0]), (2, [1.0, 4 / 9, 1 / 9, 0.0, 0.0])],
    ids=["linear", "squared: concentrated at the edge"],
)
def test_the_light_fades_across_the_band_of_width_px_inside_the_edge(falloff, expected):
    """Width 3 from the right: the edge pixel is lit fully, then (1 - 1/3), (1 - 2/3), then nothing; falloff is the
    exponent of that ramp."""
    out = registry.apply(NAME, grey(), subject=block(), angle=0, width=3, falloff=falloff, color=WHITE)
    lit = [lit_at(out, 20, col) for col in (29, 28, 27, 26, 15)]  # the last is deep inside
    np.testing.assert_allclose(lit, expected, atol=0.02)


def test_outside_the_subject_stays_dark(subject):
    photo = np.full((96, 128, 3), BASE, np.float32)
    out = registry.apply(NAME, photo, subject=subject, angle=30, width=8, amount=3.0)
    np.testing.assert_array_equal(out[subject == 0], photo[subject == 0])
    assert (out != photo).any()


def test_light_has_the_colour_given_and_amount_scales_it():
    colour = [1.0, 0.5, 0.0]
    out = registry.apply(NAME, grey(), subject=block(), angle=0, width=3, color=colour, amount=0.5)
    # half of each channel's light, screened over 0.2: 1 - 0.8 (1 - 0.5 c)
    np.testing.assert_allclose(out[20, 29], [screened(0.5), screened(0.25), screened(0.0)], atol=0.02)


def test_the_picture_border_is_not_an_edge():
    """A subject running off the right border has no rim there, however the light comes."""
    subject = np.zeros((40, 40), np.float32)
    subject[10:30, 10:] = 1.0
    out = registry.apply(NAME, grey(), subject=subject, angle=0, width=3, color=WHITE)
    assert lit_at(out, 20, 39) == pytest.approx(0.0, abs=0.02)
    out_top = registry.apply(NAME, grey(), subject=subject, angle=90, width=3, color=WHITE)
    assert lit_at(out_top, 10, 25) == pytest.approx(1.0, abs=0.02)  # its real top edge is


def test_on_a_round_subject_the_light_follows_the_edge_normal():
    """A disc of radius 15 lit from the right: full at its rightmost pixel, none a quarter turn away or opposite."""
    y, x = np.mgrid[0:40, 0:40]
    disc = (np.hypot(x - 19.5, y - 19.5) <= 15).astype(np.float32)
    out = registry.apply(NAME, grey(), subject=disc, angle=0, width=3, color=WHITE)
    assert lit_at(out, 20, 34) == pytest.approx(1.0, abs=0.05)
    assert lit_at(out, 5, 20) < 0.1
    assert lit_at(out, 20, 5) == pytest.approx(0.0, abs=0.01)


def test_a_soft_matte_edge_carries_the_light_in_proportion_to_the_matte():
    subject = block()
    subject[20, 29] = 0.6  # still the subject (>= 0.5), but only 60 % of it
    subject[20, 30] = 0.4  # not the subject (< 0.5): no light, though 40 % of it is
    out = registry.apply(NAME, grey(), subject=subject, angle=0, width=3, color=WHITE)
    assert lit_at(out, 20, 29) == pytest.approx(0.6, abs=0.03)
    assert lit_at(out, 20, 30) == pytest.approx(0.0, abs=1e-6)  # float32 rounding of `lit_at`'s inversion is below that


@pytest.mark.parametrize(
    ("threshold", "lit"), [(0.3, 1.0), (0.5, 0.0)], ids=["0.4 is subject at 0.3", "0.4 is background at 0.5"]
)
def test_subject_threshold_is_the_subject_value_from_which_a_pixel_belongs_to_the_subject(threshold, lit):
    subject = block()
    subject[20, 30] = 0.4  # beside the right edge: it is the outermost subject pixel when counted in
    out = registry.apply(NAME, grey(), subject=subject, angle=0, width=3, color=WHITE, subject_threshold=threshold)
    assert lit_at(out, 20, 30) == pytest.approx(lit * 0.4, abs=0.03)


@pytest.mark.parametrize(
    ("smoothing", "steady"),
    [(3.0, True), (0.0, False)],
    ids=["smoothed edge direction is steady", "raw edge direction jumps about"],
)
def test_smoothing_steadies_the_direction_along_a_staircase_edge(smoothing, steady):
    """The subject is below the line y = 20 + x/3, whose outward normal (0.316, -0.949) is the direction of a light
    at atan2(0.949, 0.316) = 71.57 degrees, so every pixel of the edge should be lit head-on. The pixel staircase the
    line is drawn with gives raw normals that vary a lot from step to step."""
    y, x = np.mgrid[0:60, 0:60]
    subject = y > 20 + x / 3
    above = np.zeros_like(subject)
    above[1:] = subject[:-1]  # the pixel above is the subject
    edge = subject & ~above & (x > 15) & (x < 45)  # the staircase's top pixels, away from the picture's borders
    out = registry.apply(
        NAME, grey(60), subject=subject.astype(np.float32), angle=71.57, width=1, color=WHITE, smoothing=smoothing
    )
    lit = 1.0 - (1.0 - out[..., 0]) / (1.0 - BASE)
    assert (np.abs(lit[edge] - 1.0).max() < 0.01) == steady


def test_edge_sets_what_the_smoothing_sees_beyond_the_picture_border():
    """A thin subject running off the right border, lit from the right: mirrored, the border stays no edge and its
    last column is dark; wrapped round, the background at the left border seems to lie just beyond it."""
    subject = np.zeros((40, 40), np.float32)
    subject[17:24, 10:] = 1.0
    kwargs = {"subject": subject, "angle": 0, "width": 6, "color": WHITE, "smoothing": 3}
    assert lit_at(registry.apply(NAME, grey(), edge="reflect", **kwargs), 20, 39) == pytest.approx(0.0, abs=0.02)
    assert lit_at(registry.apply(NAME, grey(), edge="wrap", **kwargs), 20, 39) > 0.2


def test_truncate_sets_how_far_the_smoothing_reaches():
    """The same wrapped-round scene: column 30 is 9 px from the border, within the 12 px a sigma-3 blur cut off at 4
    sigma reaches (it sees the background beyond the border), but not the 3 px of one cut off at 1 sigma."""
    subject = np.zeros((40, 40), np.float32)
    subject[17:24, 10:] = 1.0
    kwargs = {"subject": subject, "angle": 0, "width": 6, "color": WHITE, "smoothing": 3, "edge": "wrap"}
    assert lit_at(registry.apply(NAME, grey(), truncate=4, **kwargs), 20, 30) > 0.2
    assert lit_at(registry.apply(NAME, grey(), truncate=1, **kwargs), 20, 30) == pytest.approx(0.0, abs=0.02)
