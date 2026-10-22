import numpy as np
import pytest

from photo_fx.ops.sprites import Placement, scatter

FIELDS = ("x", "y", "size", "depth", "phase")


# ---------------------------------------------------------------- scatter


def test_scatter_is_deterministic_per_seed_and_differs_across_seeds():
    a, b, other = scatter((40, 90), 200, seed=7), scatter((40, 90), 200, seed=7), scatter((40, 90), 200, seed=8)
    for name in FIELDS:
        np.testing.assert_array_equal(getattr(a, name), getattr(b, name))
    assert not np.array_equal(a.x, other.x)
    assert not np.array_equal(a.y, other.y)


def test_scatter_returns_exactly_count_float32_sprites_of_every_field():
    p = scatter((40, 90), 123, seed=1)
    for name in FIELDS:
        arr = getattr(p, name)
        assert arr.shape == (123,), name
        assert arr.dtype == np.float32, name


def test_scatter_positions_are_inside_the_picture_and_cover_it():
    h, w = 40, 90  # non-square so that swapped axes fail
    p = scatter((h, w), 5000, seed=3)
    assert p.x.min() >= 0 and p.x.max() < w
    assert p.y.min() >= 0 and p.y.max() < h
    # uniform: every quarter of the picture gets roughly a quarter of the sprites
    for left in (True, False):
        for top in (True, False):
            inside = ((p.x < w / 2) == left) & ((p.y < h / 2) == top)
            assert inside.mean() == pytest.approx(0.25, abs=0.03)


def test_scatter_sizes_stay_in_range_and_reach_both_ends():
    p = scatter((50, 50), 600, seed=2, size=(3.0, 11.0), depth_layers=5)
    assert p.size.min() == pytest.approx(3.0)
    assert p.size.max() == pytest.approx(11.0)


def test_scatter_has_exactly_depth_layers_distinct_depths_in_unit_range():
    p = scatter((50, 50), 300, seed=2, depth_layers=4)
    depths = np.unique(p.depth)
    assert len(depths) == 4
    assert depths.min() >= 0 and depths.max() <= 1
    assert depths.min() < depths.max()
    assert len(np.unique(scatter((50, 50), 300, seed=2, depth_layers=1).depth)) == 1
    assert p.phase.min() >= 0 and p.phase.max() <= 1
    assert len(np.unique(p.phase)) > 250  # a per-sprite random number, not a constant per layer


def test_scatter_nearer_sprites_are_larger():
    p = scatter((50, 50), 300, seed=2, size=(2.0, 9.0), depth_layers=4)
    order = np.argsort(p.depth, kind="stable")
    assert np.all(np.diff(p.size[order]) >= 0)
    near, far = p.size[p.depth == p.depth.max()], p.size[p.depth == p.depth.min()]
    assert near.min() > far.max()


def test_scatter_with_zero_count_returns_empty_arrays():
    p = scatter((20, 30), 0, seed=5)
    for name in FIELDS:
        assert getattr(p, name).shape == (0,)
        assert getattr(p, name).dtype == np.float32


def test_scatter_handles_more_sprites_than_pixels_and_a_one_pixel_picture():
    p = scatter((1, 1), 50, seed=4)
    assert len(p.x) == 50
    assert np.all((p.x >= 0) & (p.x < 1) & (p.y >= 0) & (p.y < 1))


@pytest.mark.parametrize(
    ("kwargs", "name"),
    [
        ({"shape": (10, 10), "count": -1}, "count"),
        ({"shape": (10, 10), "count": 5, "size": (8.0, 2.0)}, "size"),
        ({"shape": (10, 10), "count": 5, "size": (0.0, 2.0)}, "size"),
        ({"shape": (10, 10), "count": 5, "depth_layers": 0}, "depth_layers"),
        ({"shape": (0, 10), "count": 5}, "shape"),
        ({"shape": (10, 10, 3), "count": 5}, "shape"),
    ],
)
def test_scatter_rejects_invalid_arguments_naming_them(kwargs, name):
    with pytest.raises(ValueError, match=name):
        scatter(seed=0, **kwargs)


def test_placement_is_frozen():
    p = scatter((10, 10), 3, seed=0)
    with pytest.raises(AttributeError):
        p.x = p.y  # type: ignore[misc]
    assert isinstance(p, Placement)
