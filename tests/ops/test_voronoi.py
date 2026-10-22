import numpy as np
import pytest

from photo_fx.ops import voronoi

# ---------------------------------------------------------------- random_points


def test_random_points_are_deterministic_per_seed():
    a = voronoi.random_points((30, 80), 50, seed=3)
    b = voronoi.random_points((30, 80), 50, seed=3)
    assert np.array_equal(a, b)


def test_random_points_differ_across_seeds():
    a = voronoi.random_points((30, 80), 50, seed=3)
    b = voronoi.random_points((30, 80), 50, seed=4)
    assert not np.array_equal(a, b)


def test_random_points_have_shape_dtype_and_stay_inside_a_non_square_picture():
    pts = voronoi.random_points((30, 80), 500, seed=1)  # h=30, w=80
    assert pts.shape == (500, 2) and pts.dtype == np.float32
    assert pts[:, 0].min() >= 0 and pts[:, 0].max() < 80
    assert pts[:, 1].min() >= 0 and pts[:, 1].max() < 30
    # x spans the width and y the height: a swapped (h, w) or (x, y) would break one of these
    assert pts[:, 0].max() > 30 and pts[:, 1].max() > 20


def test_random_points_never_reach_the_far_edge_even_for_the_largest_random_draw(monkeypatch):
    class LargestDraw:
        # the biggest value Generator.random can return; scaled by 3 and rounded to float32 it becomes exactly 3.0
        def random(self, size):
            return np.full(size, np.nextafter(1.0, 0.0))

    monkeypatch.setattr(np.random, "default_rng", lambda seed: LargestDraw())
    pts = voronoi.random_points((5, 3), 4, seed=0)
    assert (pts[:, 0] < 3).all() and (pts[:, 1] < 5).all()


def test_random_points_zero_count_is_empty():
    assert voronoi.random_points((10, 10), 0, seed=0).shape == (0, 2)


def test_random_points_reject_negative_count():
    with pytest.raises(ValueError):
        voronoi.random_points((10, 10), -1, seed=0)
