"""Tests of `ops/voronoi.py`: which site is nearest to any coordinates (so a bent edge can classify them), against a
brute-force reference written here."""

import numpy as np
import pytest

from bullterrier_image.ops.voronoi import centres, nearest


def brute_force(sites: np.ndarray, x: np.ndarray, y: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """For every coordinate pair, the distance to the closest site and that site's index, by comparing all of them."""
    x, y = np.broadcast_arrays(x, y)
    away = np.hypot(x[..., None] - sites[:, 0], y[..., None] - sites[:, 1])
    return away.min(axis=-1), away.argmin(axis=-1)


@pytest.mark.parametrize("shape", [(1, 1), (1, 9), (9, 1), (13, 29), (40, 25)], ids=lambda s: f"{s[0]}x{s[1]}")
def test_the_pixel_centres_lie_half_a_pixel_in_from_the_corner(shape):
    x, y = centres(shape)
    height, width = shape
    np.testing.assert_array_equal(np.broadcast_to(x, shape)[0], np.arange(width) + 0.5)
    np.testing.assert_array_equal(np.broadcast_to(y, shape)[:, 0], np.arange(height) + 0.5)
    assert x.dtype == y.dtype == np.float32


def test_every_pixel_belongs_to_its_nearest_site():
    shape = (37, 53)
    sites = np.random.default_rng(1).random((11, 2)) * (shape[1], shape[0])
    distance, owner = nearest(sites, *centres(shape))
    want_distance, want_owner = brute_force(sites, *centres(shape))
    assert owner.shape == distance.shape == shape
    np.testing.assert_array_equal(owner, want_owner)
    np.testing.assert_allclose(distance, want_distance, rtol=1e-6)


def test_any_coordinates_are_classified_not_only_the_pixel_centres():
    """Bent coordinates (as a torn edge makes them) have the picture's shape but are no grid."""
    rng = np.random.default_rng(2)
    sites = rng.random((9, 2)) * 30
    x, y = rng.random((2, 20, 24)).astype(np.float32) * 30
    distance, owner = nearest(sites, x, y)
    want_distance, want_owner = brute_force(sites, x, y)
    np.testing.assert_array_equal(owner, want_owner)
    np.testing.assert_allclose(distance, want_distance, rtol=1e-6)
