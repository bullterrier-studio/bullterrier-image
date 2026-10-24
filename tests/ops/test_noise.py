"""Tests of `ops/noise.py`: seeded fractal value noise, normalised to zero mean and unit variance."""

from typing import Any

import numpy as np
import pytest

from photo_fx.ops.noise import fractal, standardise

SETTINGS: dict[str, Any] = {"scale": 16.0, "octaves": 3, "persistence": 0.5, "lacunarity": 2.0, "seed": 4}


def neighbour_correlation(field: np.ndarray) -> float:
    """Correlation of each value with the one to its right (0 = independent pixels, 1 = smooth)."""
    return float(np.corrcoef(field[:, :-1].ravel(), field[:, 1:].ravel())[0, 1])


@pytest.mark.parametrize(
    "shape", [(64, 64), (50, 90), (1, 40), (40, 1)], ids=["square", "wide", "one row", "one column"]
)
def test_the_field_has_the_shape_asked_for_zero_mean_and_unit_variance(shape: tuple[int, int]) -> None:
    field = fractal(shape, **SETTINGS)
    assert field.shape == shape
    assert field.dtype == np.float32
    assert field.mean() == pytest.approx(0.0, abs=1e-4)
    assert field.std() == pytest.approx(1.0, abs=1e-4)


def test_the_field_is_a_pure_function_of_shape_settings_and_seed() -> None:
    np.testing.assert_array_equal(fractal((64, 64), **SETTINGS), fractal((64, 64), **SETTINGS))
    other = fractal((64, 64), **{**SETTINGS, "seed": 5})
    assert np.abs(fractal((64, 64), **SETTINGS) - other).mean() > 0.1


@pytest.mark.parametrize(
    ("scale", "smoother_than"),
    [(4.0, 0.8), (16.0, 0.97), (64.0, 0.995)],
    ids=["4 px", "16 px", "64 px"],
)
def test_a_larger_scale_gives_a_smoother_field(scale: float, smoother_than: float) -> None:
    """One octave: neighbours are correlated by how many pixels one lattice cell spans (limits worked out by eye from
    the cubic spline through independent lattice values: ~0.9, ~0.99, ~0.999)."""
    assert neighbour_correlation(fractal((256, 256), **{**SETTINGS, "octaves": 1, "scale": scale})) > smoother_than


def test_zero_persistence_leaves_only_the_first_octave() -> None:
    """Each octave is weighted persistence**i, so with 0 only the coarse octave is left."""
    only_first = fractal((64, 64), **{**SETTINGS, "octaves": 1})
    np.testing.assert_allclose(fractal((64, 64), **{**SETTINGS, "persistence": 0.0}), only_first, atol=1e-5)


def test_more_persistence_adds_more_fine_detail() -> None:
    rough = fractal((128, 128), **{**SETTINGS, "persistence": 0.9})
    smooth = fractal((128, 128), **{**SETTINGS, "persistence": 0.1})
    assert neighbour_correlation(rough) < neighbour_correlation(smooth)


def test_octaves_finer_than_a_pixel_are_left_out() -> None:
    """Lattice spacings 2, 1, 0.5, ...: from the third on they are finer than the pixel grid and add nothing."""
    two = fractal((64, 64), **{**SETTINGS, "scale": 2.0, "octaves": 2})
    np.testing.assert_array_equal(fractal((64, 64), **{**SETTINGS, "scale": 2.0, "octaves": 8}), two)


@pytest.mark.parametrize(
    ("values", "spread"),
    [
        (np.array([[1.0, 3.0], [5.0, 7.0]]), np.std([1.0, 3.0, 5.0, 7.0])),
        (np.array([[10.0, 10.5]]), 0.25),
        (np.array([[2.0]]), 1.0),  # a single value has no variation to scale: it becomes 0
    ],
    ids=["four values", "two close values", "one value"],
)
def test_standardise_centres_on_zero_and_scales_to_the_standard_deviation(values: np.ndarray, spread: float) -> None:
    out = standardise(values.astype(np.float32))
    np.testing.assert_allclose(out, (values - values.mean()) / spread, atol=1e-5)
