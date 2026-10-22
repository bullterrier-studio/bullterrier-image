"""Tests of `matte.refine_edge`, the part of finding the subject that runs anywhere (the model itself is Apple Vision).

Expected values are worked out by hand: erosion by a disc on axis-aligned shapes, and the normal distribution for a
blurred straight edge."""

import math

import numpy as np
import pytest

from photo_fx.matte import refine_edge


def square(size: int = 40, start: int = 10, stop: int = 30) -> np.ndarray:
    alpha = np.zeros((size, size), np.float32)
    alpha[start:stop, start:stop] = 1.0
    return alpha


def half_plane(edge: int = 30) -> np.ndarray:
    """1 from column `edge` to the right border, 0 to its left: a straight vertical edge."""
    alpha = np.zeros((20, 60), np.float32)
    alpha[:, edge:] = 1.0
    return alpha


def normal_cdf(x: float) -> float:
    return 0.5 * (1 + math.erf(x / math.sqrt(2)))


@pytest.mark.parametrize(
    "shrink, pulled_in",
    [
        (3.0, 3),  # a disc of radius 3 reaches 3 px along each axis
        (2.5, 2),  # x^2 + y^2 <= 6.25 holds for (2, 1) but not for (3, 0)
        (1.5, 1),
    ],
)
def test_shrink_pulls_the_edge_in_by_a_disc_of_that_radius(shrink, pulled_in):
    expected = square(start=10 + pulled_in, stop=30 - pulled_in)
    np.testing.assert_array_equal(refine_edge(square(), shrink, 0.0), expected)


def test_nothing_to_shrink_or_feather_leaves_the_matte_as_it_is():
    alpha = square()
    np.testing.assert_array_equal(refine_edge(alpha, 0.0, 0.0), alpha)


def test_a_subject_cut_off_by_the_picture_border_is_not_pulled_in_from_the_border():
    alpha = np.zeros((40, 40), np.float32)
    alpha[:20] = 1.0  # a subject filling the top half, cut off by the top, left and right borders
    expected = np.zeros((40, 40), np.float32)
    expected[:17] = 1.0  # only its real edge, at row 20, moves in
    np.testing.assert_array_equal(refine_edge(alpha, 3.0, 0.0), expected)


def test_feather_softens_a_straight_edge_like_a_gaussian_of_that_sigma_and_keeps_the_amount():
    soft = refine_edge(half_plane(edge=30), 0.0, 2.0)
    # The pixel centres beside the edge lie 0.5 px from it: a sigma-2 blur gives them normal_cdf(+-0.25).
    assert soft[10, 30] == pytest.approx(normal_cdf(0.25), abs=0.01)
    assert soft[10, 29] == pytest.approx(normal_cdf(-0.25), abs=0.01)
    assert soft.min() >= 0.0
    assert soft.max() <= 1.0
    assert soft.sum() == pytest.approx(half_plane(edge=30).sum(), rel=1e-4)  # the blur moves the edge, adds nothing


def test_shrink_happens_before_feather():
    soft = refine_edge(half_plane(edge=30), 3.0, 2.0)  # the edge moves in to column 33, then softens with sigma 2
    assert soft[10, 33] == pytest.approx(normal_cdf(0.25), abs=0.01)
    assert soft[10, 32] == pytest.approx(normal_cdf(-0.25), abs=0.01)


def test_the_result_is_float32_and_the_input_is_not_modified():
    alpha = square()
    before = alpha.copy()
    out = refine_edge(alpha, 2.0, 1.0)
    assert out.dtype == np.float32
    assert out.shape == alpha.shape
    np.testing.assert_array_equal(alpha, before)
