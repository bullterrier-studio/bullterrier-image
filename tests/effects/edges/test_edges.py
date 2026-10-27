"""Behaviour of the `edges` effect."""

import numpy as np
import pytest
from scipy.special import ndtr

from bullterrier_image.core import registry

NAME = "edges"
METHODS = ["sobel", "canny", "dog"]


def edge_map(picture: np.ndarray, **settings: object) -> np.ndarray:
    """The effect's picture as one channel, for settings that draw white on black."""
    return registry.apply(NAME, np.repeat(picture[..., None], 3, axis=-1), **settings)[..., 0]


def step_picture(width: int = 64, at: int = 32) -> np.ndarray:
    """Black on the left, white from column `at` on."""
    picture = np.zeros((9, width), np.float32)
    picture[:, at:] = 1.0
    return picture


@pytest.mark.parametrize("method", METHODS)
def test_a_flat_picture_is_all_background(method: str) -> None:
    flat = np.full((20, 24, 3), 0.4, np.float32)
    out = registry.apply(NAME, flat, method=method, color="#ff0000", background="#001020", threshold=0.01)
    np.testing.assert_allclose(out, np.broadcast_to([0, 16 / 255, 32 / 255], out.shape), atol=1e-6)


@pytest.mark.parametrize("radius", [0.5, 3.0, 12.0], ids=["radius 0.5", "radius 3", "radius 12"])
@pytest.mark.parametrize("method", ["sobel", "canny"])
def test_a_black_to_white_step_is_drawn_in_full_at_the_step_and_not_at_all_far_from_it(
    method: str, radius: float
) -> None:
    """Whatever the method and the smoothing, the steepest edge there can be reads 1 (fully drawn); the Gaussian reaches
    4 x radius px and the gradient one more, so beyond that there is nothing."""
    width = 400
    out = edge_map(step_picture(width, at=200), method=method, radius=radius)[4]
    assert out[198:202].max() == pytest.approx(1.0, abs=1e-5)
    assert out[198:202].max() == out.max()
    reach = int(4 * radius) + 3
    np.testing.assert_array_equal(out[: 200 - reach], 0.0)
    np.testing.assert_array_equal(out[200 + reach :], 0.0)


@pytest.mark.parametrize("radius", [0.5, 1.5, 3.0], ids=["radius 0.5", "radius 1.5", "radius 3"])
def test_dog_draws_a_black_to_white_step_as_one_ink_line_on_its_dark_side(radius: float) -> None:
    """One run of drawn pixels, fully drawn somewhere, entirely on the black side (columns before the step at 200) and
    no farther from the step than the Gaussians reach (4 x 1.6 x radius px); nothing on the white side or beyond."""
    out = edge_map(step_picture(400, at=200), method="dog", radius=radius, threshold=0.3)[4]
    drawn = np.flatnonzero(out)
    assert out.max() == pytest.approx(1.0, abs=1e-5)
    np.testing.assert_array_equal(np.diff(drawn), 1)  # one line, not two
    assert drawn.max() < 200
    assert drawn.min() >= 200 - (int(4 * radius * 1.6) + 3)


def test_dog_across_a_step_follows_the_difference_of_two_gaussians_of_radius_and_1_6_times_radius() -> None:
    """Oracle: a step blurred by a Gaussian of sigma is the normal CDF, so on the black side at distance d from the step
    the response is cdf(d / sigma) - cdf(d / 1.6 sigma), here drawn linearly (`threshold` 1, `softness` 1: half the
    response relative to its peak). The ratio 1.6 is Marr and Hildreth's."""
    sigma = 4.0
    out = edge_map(step_picture(400, at=200), method="dog", radius=sigma, threshold=1.0, softness=1.0)[4]
    distance = 199.5 - np.arange(200 - 30, 200)  # pixel centres to the step between columns 199 and 200
    response = ndtr(distance / sigma) - ndtr(distance / (1.6 * sigma))
    far = np.linspace(0, 40, 4001)
    peak = (ndtr(far / sigma) - ndtr(far / (1.6 * sigma))).max()
    np.testing.assert_allclose(out[200 - 30 : 200], response / peak / 2, atol=0.01)


@pytest.mark.parametrize("radius", [0.5, 1.5, 3.0], ids=["radius 0.5", "radius 1.5", "radius 3"])
def test_dog_draws_a_dark_line_on_light_but_not_a_light_line_on_dark(radius: float) -> None:
    """Ink goes where the picture is darker than its surroundings: a dark stroke is drawn at its own pixel, a light
    stroke is not (only the dark ground beside it is)."""
    dark_stroke = np.ones((9, 400), np.float32)
    dark_stroke[:, 200] = 0.0
    assert edge_map(dark_stroke, method="dog", radius=radius)[4, 200] == pytest.approx(1.0, abs=1e-5)
    assert edge_map(1.0 - dark_stroke, method="dog", radius=radius)[4, 200] == 0.0


def test_edges_are_drawn_in_color_on_background() -> None:
    picture = step_picture()
    rgb = np.repeat(picture[..., None], 3, axis=-1)
    out = registry.apply(NAME, rgb, method="sobel", radius=1.0, color="#ff8000", background="#000040")
    np.testing.assert_allclose(out[4, 32], [1.0, 128 / 255, 0.0], atol=1e-6)
    np.testing.assert_allclose(out[4, 0], [0.0, 0.0, 64 / 255], atol=1e-6)


def test_sobel_inside_a_ramp_is_the_same_everywhere_and_in_proportion_to_the_slope() -> None:
    """With `threshold` 1 and `softness` 1 the drawing is linear in the steepness (0 at 0, 1 at 2), so a ramp twice as
    steep is drawn twice as bright, and wherever the ramp runs freely it is drawn the same."""

    def inside(slope: float) -> np.ndarray:
        ramp = np.tile(0.2 + slope * np.arange(60, dtype=np.float32), (30, 1))
        return edge_map(ramp, method="sobel", radius=1.0, threshold=1.0, softness=1.0)[5:-5, 10:-10]

    gentle, steep = inside(0.01), inside(0.02)
    np.testing.assert_allclose(gentle, gentle[0, 0], atol=1e-5)
    assert gentle[0, 0] > 0
    np.testing.assert_allclose(steep, 2 * gentle[0, 0], rtol=1e-4)


@pytest.mark.parametrize("method", ["sobel", "dog"])
def test_a_turned_picture_has_the_turned_edge_map(method: str, photo: np.ndarray) -> None:
    """The gradient has no preferred direction: turning the picture a quarter turn turns its edges the same way."""
    grey = photo[..., 0]
    out = edge_map(grey, method=method, radius=1.5)
    turned = edge_map(np.rot90(grey).copy(), method=method, radius=1.5)
    # 'dog' subtracts two float32 blurs summed in another order after a turn; the ramp from threshold to full (0.1 of
    # the range at the defaults) magnifies that rounding about a hundredfold
    np.testing.assert_allclose(turned, np.rot90(out), atol=1e-4)


@pytest.mark.parametrize("radius", [0.5, 2.0, 6.0])
def test_canny_lines_are_at_most_two_pixels_thick(radius: float) -> None:
    out = edge_map(step_picture(), method="canny", radius=radius)
    assert 1 <= (out[4] > 0).sum() <= 2
    assert (out[4, 31:33] > 0).any()


def test_canny_is_much_thinner_than_sobel_on_a_photo(photo: np.ndarray) -> None:
    grey = photo[..., 0]
    settings = {"radius": 1.5, "threshold": 0.1}
    assert (edge_map(grey, method="canny", **settings) > 0).sum() < 0.6 * (
        edge_map(grey, method="sobel", **settings) > 0
    ).sum()


def fading_line(contrast: float, rows: int = 100) -> np.ndarray:
    """A vertical step at column 16 whose height falls slowly from 1 at the top to `contrast` at the bottom."""
    height = np.linspace(1.0, contrast, rows, dtype=np.float32)
    picture = np.zeros((rows, 40), np.float32)
    picture[:, 16:] = height[:, None]
    return picture


def test_canny_keeps_a_weak_line_that_joins_a_strong_one_and_drops_one_that_does_not() -> None:
    """The line's steepness falls from 1 to 0.12 between the thresholds 0.05 and 0.15 (`threshold` 0.1, `softness`
    0.5). Joined to its strong top, its weak end stays; alone, at 0.12 all the way, none of it does."""
    settings = {"radius": 1.0, "threshold": 0.1, "softness": 0.5}
    weak = np.tile(0.12 * (np.arange(40) >= 16), (100, 1)).astype(np.float32)
    assert edge_map(fading_line(0.12), method="canny", **settings)[95:, 15:17].max() > 0
    assert edge_map(weak, method="canny", **settings).max() == 0
    assert edge_map(weak, method="sobel", **settings).max() > 0  # the line is not too faint to draw, only unjoined


def test_canny_does_not_join_a_weak_line_to_a_strong_one_across_a_stretch_that_is_below_the_lower_threshold() -> None:
    """The line is strong at the top, falls to a steepness of 0.02 (under the lower threshold 0.05) and rises to 0.12
    (weak) again: the weak part has no strong line to join, only a gap."""
    rows = np.arange(600)
    height = np.interp(rows, [0, 100, 300, 400, 500, 599], [1.0, 1.0, 0.02, 0.02, 0.12, 0.12]).astype(np.float32)
    picture = np.zeros((600, 40), np.float32)
    picture[:, 16:] = height[:, None]
    out = edge_map(picture, method="canny", radius=1.0, threshold=0.1, softness=0.5)
    assert out[5:95, 15:17].max() > 0
    assert out[500:595].max() == 0


@pytest.mark.parametrize("method", METHODS)
def test_softness_0_draws_each_pixel_fully_or_not_at_all(method: str) -> None:
    """On a line whose steepness falls through the thresholds, from 1 down to 0.12."""
    hard = edge_map(fading_line(0.12), method=method, radius=1.0, threshold=0.1, softness=0.0)
    soft = edge_map(fading_line(0.12), method=method, radius=1.0, threshold=0.1, softness=0.5)
    assert set(np.unique(hard)) <= {0.0, 1.0}
    assert ((soft > 0) & (soft < 1)).any()


@pytest.mark.parametrize("method", METHODS)
def test_raising_the_threshold_never_draws_more(method: str, photo: np.ndarray) -> None:
    grey = photo[..., 0]
    low = edge_map(grey, method=method, threshold=0.05)
    high = edge_map(grey, method=method, threshold=0.2)
    assert (high <= low + 1e-6).all()
    assert high.sum() < low.sum()


def test_smoothing_lowers_a_fine_lines_steepness_in_inverse_proportion_to_the_radius() -> None:
    """Hand computation: a thin line smoothed by a Gaussian of sigma has its steepest slope 1/sqrt(e) / sigma times the
    step's (the derivative of a Gaussian peaks at sigma, where it is e^-1/2 of g'(0)'s scale 1/(sigma sqrt(2 pi)))."""
    line = np.zeros((20, 200), np.float32)
    line[:, 100] = 1.0
    peaks = [
        edge_map(line, method="sobel", radius=sigma, threshold=1.0, softness=1.0).max() * 2  # 2: the drawing is s / 2
        for sigma in (4.0, 8.0)
    ]
    assert peaks[0] == pytest.approx(np.exp(-0.5) / 4.0, rel=0.1)
    assert peaks[1] == pytest.approx(np.exp(-0.5) / 8.0, rel=0.1)
