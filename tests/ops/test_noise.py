"""Tests of `ops/noise.py`: seeded white noise clumped into specks."""

import math
from typing import Any

import numpy as np
import pytest

from bullterrier_image.ops.noise import clumped_noise


def neighbour_correlation(field: np.ndarray) -> float:
    """Correlation of each value with the one to its right (0 = independent pixels, 1 = smooth)."""
    return float(np.corrcoef(field[:, :-1].ravel(), field[:, 1:].ravel())[0, 1])


CLUMPS: dict[str, Any] = {"size": 4.0, "seed": 3, "truncate": 4.0, "edge": "reflect"}


@pytest.mark.parametrize(
    ("size", "neighbour_correlation_is"),
    [(0.0, 0.0), (1.0, 0.0), (1.5, math.exp(-1 / 2.25)), (4.0, math.exp(-1 / 16)), (8.0, math.exp(-1 / 64))],
    ids=["no clumping", "one pixel", "1.5 px specks", "4 px specks", "8 px specks"],
)
def test_clumped_noise_has_unit_deviation_and_neighbours_correlated_by_its_size(
    size: float, neighbour_correlation_is: float
) -> None:
    """White noise blurred by a Gaussian of sigma size / 2 has a Gaussian autocorrelation of sigma size / sqrt 2, so
    neighbours 1 px apart correlate by exp(-1 / (4 sigma^2)): exp(-1/2.25) at 1.5 px, exp(-1/16) at 4 px,
    exp(-1/64) at 8 px."""
    noise = clumped_noise((300, 300), **{**CLUMPS, "size": size})
    assert noise.dtype == np.float32
    assert noise.std() == pytest.approx(1.0, abs=0.02)
    assert neighbour_correlation(noise) == pytest.approx(neighbour_correlation_is, abs=0.015)


@pytest.mark.parametrize("size", [1.0, 4.0], ids=["one pixel", "4 px specks"])
@pytest.mark.parametrize("distribution", ["gaussian", "uniform"])
def test_clumped_noise_has_zero_mean_and_unit_deviation_in_either_distribution(distribution: str, size: float) -> None:
    noise = clumped_noise((300, 300), **{**CLUMPS, "size": size, "distribution": distribution})
    assert noise.mean() == pytest.approx(0.0, abs=0.02)
    assert noise.std() == pytest.approx(1.0, abs=0.02)


def test_unclumped_uniform_noise_fills_its_range_evenly_and_gaussian_noise_does_not() -> None:
    """A uniform distribution of unit deviation spans +-sqrt(3), evenly; a Gaussian's tail goes further."""
    bound = math.sqrt(3)
    uniform = clumped_noise((300, 300), **{**CLUMPS, "size": 1.0, "distribution": "uniform"})
    assert np.abs(uniform).max() <= bound
    assert np.abs(uniform).max() > 0.99 * bound
    assert np.mean(np.abs(uniform) < bound / 2) == pytest.approx(0.5, abs=0.01)  # evenly: half within half the bound
    gaussian = clumped_noise((300, 300), **{**CLUMPS, "size": 1.0, "distribution": "gaussian"})
    assert np.abs(gaussian).max() > 2 * bound


def test_clumped_noise_is_a_pure_function_of_its_arguments() -> None:
    np.testing.assert_array_equal(clumped_noise((40, 50), **CLUMPS), clumped_noise((40, 50), **CLUMPS))
    assert np.abs(clumped_noise((40, 50), **CLUMPS) - clumped_noise((40, 50), **{**CLUMPS, "seed": 4})).mean() > 0.1


def test_clumped_noise_edge_rule_matters_only_near_the_border() -> None:
    """The blur (sigma 2, cut at 2 sigma = 4 px) sees beyond the picture only within 4 px of its sides."""
    reflect = clumped_noise((60, 60), **{**CLUMPS, "truncate": 2.0})
    wrap = clumped_noise((60, 60), **{**CLUMPS, "truncate": 2.0, "edge": "wrap"})
    inner = (slice(6, -6), slice(6, -6))
    assert np.corrcoef(reflect[inner].ravel(), wrap[inner].ravel())[0, 1] > 1 - 1e-6
    assert np.corrcoef(reflect[:, :2].ravel(), wrap[:, :2].ravel())[0, 1] < 0.99
