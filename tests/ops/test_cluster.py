"""Tests of `ops/cluster.py`: k-means centres and the nearest-centre assignment."""

from itertools import pairwise
from typing import Literal

import numpy as np
import pytest

from photo_fx.ops.cluster import kmeans, nearest

Init = Literal["k-means++", "random"]
# Three tight blobs, far apart in a 3-d space; their centres are known by construction.
CENTRES = np.array([[0.1, 0.1, 0.1], [0.9, 0.1, 0.1], [0.5, 0.9, 0.8]])


def blobs(n_blobs: int, per_blob: int = 200, spread: float = 0.01, seed: int = 3) -> np.ndarray:
    rng = np.random.default_rng(seed)
    return np.concatenate([c + rng.normal(0, spread, (per_blob, 3)) for c in CENTRES[:n_blobs]]).astype(np.float32)


def inertia(points: np.ndarray, centres: np.ndarray) -> float:
    """Sum over points of the squared distance to the closest centre, by brute force."""
    gaps = points[:, None, :].astype(np.float64) - centres[None, :, :].astype(np.float64)
    return float(((gaps**2).sum(axis=2)).min(axis=1).sum())


@pytest.mark.parametrize(
    ("init", "n_blobs"),
    [
        pytest.param("k-means++", 3, id="k-means++ spreads its starts over three blobs"),
        pytest.param("random", 2, id="random starts recover two blobs even when both start in one"),
    ],
)
@pytest.mark.parametrize("seed", [0, 1, 7, 12345])
def test_well_separated_blobs_are_found_whatever_the_seed(init: Init, n_blobs: int, seed: int) -> None:
    points = blobs(n_blobs)
    centres = kmeans(points, n_blobs, init=init, iterations=50, tolerance=1e-6, seed=seed)
    found = centres[np.argsort(centres[:, 0])]
    means = np.stack([points[i * 200 : (i + 1) * 200].mean(axis=0) for i in range(n_blobs)])  # each blob's own mean
    np.testing.assert_allclose(found, means[np.argsort(means[:, 0])], atol=1e-5)


def test_the_same_seed_gives_the_same_centres_and_k_centres_come_back() -> None:
    points = np.random.default_rng(5).random((500, 3)).astype(np.float32)
    first = kmeans(points, 6, init="k-means++", iterations=10, tolerance=1e-4, seed=9)
    again = kmeans(points, 6, init="k-means++", iterations=10, tolerance=1e-4, seed=9)
    np.testing.assert_array_equal(first, again)
    assert first.shape == (6, 3)


@pytest.mark.parametrize(
    ("n_points", "k", "n_centres"),
    [
        pytest.param(10, 4, 4, id="more points than centres gives k centres"),
        pytest.param(4, 4, 4, id="as many points as centres gives k centres"),
        pytest.param(3, 5, 3, id="fewer points than centres gives one centre per point"),
        pytest.param(1, 2, 1, id="a single point gives a single centre"),
    ],
)
def test_there_are_k_centres_unless_there_are_fewer_points(n_points: int, k: int, n_centres: int) -> None:
    points = np.random.default_rng(2).random((n_points, 3)).astype(np.float32)
    assert kmeans(points, k, init="random", iterations=5, tolerance=1e-4, seed=0).shape == (n_centres, 3)


@pytest.mark.parametrize("init", ["k-means++", "random"])
def test_identical_points_give_centres_on_that_point_without_nan(init: Init) -> None:
    points = np.full((50, 3), 0.25, np.float32)
    centres = kmeans(points, 3, init=init, iterations=5, tolerance=1e-4, seed=0)
    np.testing.assert_allclose(centres, 0.25)


@pytest.mark.parametrize("seed", range(6))
def test_a_centre_started_on_a_duplicate_point_is_not_wasted(seed: int) -> None:
    """Random starts often pick two points with the same coordinates here; the spare centre must still find the
    location nobody started on."""
    places = np.array([[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], np.float32)
    points = np.repeat(places, 50, axis=0)
    centres = kmeans(points, 4, init="random", iterations=20, tolerance=0.0, seed=seed)
    np.testing.assert_allclose(centres[np.lexsort(centres.T)], places[np.lexsort(places.T)], atol=1e-6)


def test_different_seeds_start_from_different_points() -> None:
    points = np.random.default_rng(5).random((100, 3)).astype(np.float32)
    runs = [kmeans(points, 5, init="random", iterations=1, tolerance=0.0, seed=seed) for seed in range(4)]
    assert any(not np.array_equal(runs[0], other) for other in runs[1:])


def test_more_iterations_never_raise_the_total_squared_distance() -> None:
    rng = np.random.default_rng(11)
    points = rng.random((400, 3)).astype(np.float32)  # no clusters at all: Lloyd needs many rounds
    costs = [
        inertia(points, kmeans(points, 5, init="random", iterations=n, tolerance=0.0, seed=4)) for n in (1, 2, 4, 8, 30)
    ]
    assert all(later <= earlier + 1e-9 for earlier, later in pairwise(costs))
    assert costs[-1] < costs[0]  # and the extra rounds did improve it


def test_a_tolerance_larger_than_any_move_stops_after_the_first_round() -> None:
    points = np.random.default_rng(11).random((400, 3)).astype(np.float32)
    one_round = kmeans(points, 5, init="random", iterations=1, tolerance=0.0, seed=4)
    stopped = kmeans(points, 5, init="random", iterations=50, tolerance=10.0, seed=4)
    np.testing.assert_array_equal(stopped, one_round)


@pytest.mark.parametrize("n_points", [7, 30_000], ids=["a handful of points", "far more than any chunk"])
def test_nearest_is_the_index_of_the_closest_centre_by_brute_force(n_points: int) -> None:
    rng = np.random.default_rng(8)
    points, centres = rng.random((n_points, 3)).astype(np.float32), rng.random((5, 3)).astype(np.float32)
    expected = [
        min(range(5), key=lambda c: sum((float(p[a]) - float(centres[c, a])) ** 2 for a in range(3))) for p in points
    ]
    np.testing.assert_array_equal(nearest(points, centres), expected)
