"""Behaviour of `bullterrier_image.ops.edges`: how fast a picture's brightness changes at each pixel."""

import numpy as np
import pytest

from bullterrier_image.ops.convolve import Edge
from bullterrier_image.ops.edges import connected_to, edge_strength, ridges, sobel_gradient


def strength(grey: np.ndarray, *, sigma: float = 0.0, truncate: float = 4.0, edge: Edge = "nearest") -> np.ndarray:
    return edge_strength(grey, sigma=sigma, truncate=truncate, edge=edge)


def test_a_flat_picture_has_no_edges() -> None:
    np.testing.assert_array_equal(strength(np.full((9, 11), 0.4, np.float32), sigma=2.0), 0.0)


@pytest.mark.parametrize(
    ("slope_x", "slope_y"),
    [(0.05, 0.0), (0.0, 0.05), (0.03, 0.04), (-0.03, 0.04)],
    ids=["rising across", "rising down", "diagonal: the 3-4-5 triangle", "falling across, rising down"],
)
def test_inside_a_ramp_the_strength_is_its_slope_in_brightness_per_pixel(slope_x: float, slope_y: float) -> None:
    """A plane `slope_x x + slope_y y` rises by hypot(slope_x, slope_y) per pixel along its steepest direction."""
    rows, cols = np.mgrid[0:12, 0:14]
    plane = (0.5 + slope_x * cols + slope_y * rows).astype(np.float32)
    inside = strength(plane)[1:-1, 1:-1]
    np.testing.assert_allclose(inside, np.hypot(slope_x, slope_y), atol=1e-6)


@pytest.mark.parametrize("vertical", [True, False], ids=["a vertical step", "a horizontal step"])
def test_a_step_reads_half_its_height_on_both_pixels_beside_it_and_nothing_elsewhere(vertical: bool) -> None:
    """A step of 0.6 between column 5 and column 6: the Sobel kernel's smoothing weights 1, 2, 1 add up to 4 and the
    difference across the step is 0.6, a slope of 4 x 0.6 / 8 = 0.3 (the 8 being 4 times the 2 px a difference spans) on
    each of the two pixels next to it."""
    picture = np.zeros((12, 12), np.float32)
    picture[:, 6:] = 0.6
    if not vertical:
        picture = picture.T
    out = strength(picture)
    beside = np.zeros((12, 12), bool)
    beside[:, 5:7] = True
    if not vertical:
        beside = beside.T
    np.testing.assert_allclose(out[beside], 0.3, atol=1e-6)
    np.testing.assert_array_equal(out[~beside], 0.0)


def test_smoothing_spreads_a_step_and_lowers_its_peak() -> None:
    picture = np.zeros((9, 40), np.float32)
    picture[:, 20:] = 1.0
    sharp, soft = strength(picture, sigma=0.0), strength(picture, sigma=2.0)
    assert soft.max() < sharp.max()
    assert (soft[4, 15:25] > 0.01).all()  # and reaches several pixels either side
    assert (sharp[4, 15:25] > 0.01).sum() == 2


@pytest.mark.parametrize(
    ("edge", "first_column"),
    [("nearest", 0.05), ("reflect", 0.05), ("mirror", 0.0), ("wrap", 0.5)],
    ids=[
        "nearest: the edge pixel is its own neighbour, half the slope",
        "reflect: the same",
        "mirror: the pixel beside the edge on both sides, no slope",
        "wrap: the last column is the neighbour, a drop of 1.0 across the pixel",
    ],
)
def test_edge_decides_the_border_column_and_nothing_inside(edge: Edge, first_column: float) -> None:
    """A ramp of 0.1 per column over 12 columns: inside, the strength is 0.1. In column 0 the difference across the
    pixel spans from its left neighbour to its right one (0.2 in the ramp's own steps): nearest and reflect repeat the
    pixel itself, so 0.1, half the slope (0.05); mirror reads the pixel beside it on both sides, 0; wrap reads the last
    column (1.1) on the left and column 1 (0.1) on the right, a difference of -1.0, 5 times 0.1 x 2, so 0.5."""
    ramp = np.tile(0.1 * np.arange(12, dtype=np.float32), (9, 1))
    out = strength(ramp, edge=edge)
    np.testing.assert_allclose(out[1:-1, 1:-1], 0.1, atol=1e-6)
    assert out[4, 0] == pytest.approx(first_column, abs=1e-6)


def test_truncate_cuts_the_smoothing_off_at_that_many_sigmas() -> None:
    """A step smoothed with sigma 2: 5 px from it the full 4 sigmas (8 px) still give a slope, a cut-off at 1 sigma
    (2 px) none."""
    picture = np.zeros((9, 40), np.float32)
    picture[:, 20:] = 1.0
    assert strength(picture, sigma=2.0, truncate=4.0)[4, 14] > 0.001
    assert strength(picture, sigma=2.0, truncate=1.0)[4, 14] == 0.0


@pytest.mark.parametrize(
    ("slope_x", "slope_y"),
    [(0.05, 0.0), (0.0, -0.05), (-0.03, 0.04)],
    ids=["rising to the right: gx only", "falling downwards: negative gy only", "both, signed"],
)
def test_the_gradient_is_the_signed_slope_across_and_down(slope_x: float, slope_y: float) -> None:
    rows, cols = np.mgrid[0:12, 0:14]
    plane = (0.5 + slope_x * cols + slope_y * rows).astype(np.float32)
    gx, gy = sobel_gradient(plane, edge="nearest")
    np.testing.assert_allclose(gx[1:-1, 1:-1], slope_x, atol=1e-6)
    np.testing.assert_allclose(gy[1:-1, 1:-1], slope_y, atol=1e-6)


def crests(strength: np.ndarray, gx: np.ndarray, gy: np.ndarray) -> list[tuple[int, int]]:
    return [(int(row), int(col)) for row, col in np.argwhere(ridges(strength, gx, gy))]


@pytest.mark.parametrize(
    ("profile", "expected"),
    [
        ([0, 1, 3, 5, 3, 1, 0], [3]),
        ([0, 2, 2, 0, 0, 0, 0], [1]),
        ([5, 3, 1, 0, 0, 0, 0], [0]),
        ([1, 4, 1, 0, 3, 0, 0], [1, 4]),
    ],
    ids=["one hump: its top", "two equal tops: one stays", "the border pixel can be a crest", "two humps: both tops"],
)
def test_ridges_keep_the_crest_of_the_strength_across_a_horizontal_gradient(
    profile: list[int], expected: list[int]
) -> None:
    strength = np.tile(np.asarray(profile, np.float32), (3, 1))
    gradient_x = strength  # a gradient pointing right, as long as the strength
    found = crests(strength, gradient_x, np.zeros_like(strength))
    assert found == [(row, col) for row in range(3) for col in expected]


def test_ridges_look_across_the_gradient_whichever_way_it_points() -> None:
    """The same hump along the rows, with the gradient pointing down (then up): its top row stays, not its sides."""
    strength = np.tile(np.asarray([0, 1, 3, 1, 0], np.float32)[:, None], (1, 3))
    for sign in (1, -1):
        found = crests(strength, np.zeros_like(strength), sign * strength)
        assert found == [(2, col) for col in range(3)]


def test_ridges_look_across_a_diagonal_line_when_the_gradient_points_across_it() -> None:
    """A diagonal line of strength 4 with 2 on each flank and nothing further, its gradient pointing down and to the
    left, across it: every pixel of the line stays and nothing beyond the flanks does. Equal flanks tie, and a tie
    keeps one side only, so the line is no more than 2 px thick."""
    index = np.arange(9)
    across = index[:, None] - index[None, :]  # 0 on the main diagonal, then 1, 2... away from it
    strength = np.select([across == 0, np.abs(across) == 1], [4.0, 2.0], 0.0).astype(np.float32)
    found = crests(strength, -strength / np.sqrt(2), strength / np.sqrt(2))
    assert {(i, i) for i in index} <= set(found)
    assert all(abs(row - col) <= 1 for row, col in found)
    assert all(sum(1 for r, _ in found if r == row) <= 2 for row in index)


def test_connected_to_keeps_the_candidates_that_touch_a_seed_diagonals_included() -> None:
    candidates = np.zeros((6, 8), bool)
    candidates[0, 0:3] = True  # a run with a seed at one end
    candidates[1, 3] = True  # continues diagonally
    candidates[2, 4:6] = True
    candidates[5, 5:8] = True  # a separate run, no seed
    seeds = np.zeros_like(candidates)
    seeds[0, 0] = True
    kept = connected_to(candidates, seeds)
    expected = candidates.copy()
    expected[5] = False
    np.testing.assert_array_equal(kept, expected)
