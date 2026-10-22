import numpy as np
import pytest

from photo_fx.ops.sprites import Placement, scatter, stamp

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


# ---------------------------------------------------------------- stamp


def sprite(x: float, y: float, size: float = 10.0, depth: float = 1.0) -> Placement:
    """One sprite; depth 1 (full brightness) unless stated."""
    return Placement(*(np.array([v], dtype=np.float32) for v in (x, y, size, depth, 0.0)))


def many(xs, ys, size: float = 10.0, depth: float = 1.0) -> Placement:
    n = len(xs)
    full = lambda v: np.full(n, v, dtype=np.float32)  # noqa: E731
    return Placement(np.asarray(xs, np.float32), np.asarray(ys, np.float32), full(size), full(depth), full(0.0))


def black(h: int = 101, w: int = 101) -> np.ndarray:
    return np.zeros((h, w, 3), dtype=np.float32)


CENTRE = 50.5  # the middle of pixel (50, 50) on a 101x101 picture


def test_stamp_of_an_empty_placement_is_an_equal_new_array(photo):
    before = photo.copy()
    empty = scatter(photo.shape[:2], 0, seed=0)
    out = stamp(photo, empty)
    np.testing.assert_array_equal(out, photo)
    assert out is not photo and not np.shares_memory(out, photo)
    np.testing.assert_array_equal(photo, before)


def test_stamp_does_not_modify_its_input_when_drawing(photo):
    before = photo.copy()
    out = stamp(photo, scatter(photo.shape[:2], 40, seed=0))
    np.testing.assert_array_equal(photo, before)
    assert not np.array_equal(out, photo)


def test_a_central_sprite_lights_its_neighbourhood_and_leaves_a_far_corner_exactly_unchanged():
    out = stamp(black(), sprite(CENTRE, CENTRE), blend="add")
    assert out[50, 50].min() > 0.99
    assert out[48:53, 48:53].min() > 0.3
    assert np.all(out[:20, :20] == 0)
    assert np.all(out[-20:, -20:] == 0)


def test_the_profile_peaks_at_the_centre_and_falls_off_monotonically():
    out = stamp(black(), sprite(CENTRE, CENTRE, size=20), blend="add")[..., 0]
    assert out.max() == out[50, 50]
    for line in (out[50, 50:], out[50:, 50], out[50, 50::-1], out[50::-1, 50]):
        assert np.all(np.diff(line) <= 1e-6)
    assert out[50, 50] > out[50, 58] > out[50, 62] > 0.0  # a real fall-off, not a plateau
    assert out[50, 75] == 0.0


def test_the_sprite_is_centred_where_it_is_asked_to_be_in_x_and_y_separately():
    out = stamp(black(60, 120), sprite(90.3, 20.8, size=8), blend="add")[..., 0]
    rows, cols = np.mgrid[0:60, 0:120] + 0.5  # pixel centres
    assert (out * cols).sum() / out.sum() == pytest.approx(90.3, abs=0.02)  # centre of mass of the light
    assert (out * rows).sum() / out.sum() == pytest.approx(20.8, abs=0.02)


@pytest.mark.parametrize(("small", "large"), [(4.0, 8.0), (6.0, 12.0), (5.0, 15.0)])
def test_total_light_follows_the_area_law(small, large):
    light = lambda size: stamp(black(), sprite(CENTRE, CENTRE, size), blend="add").sum()  # noqa: E731
    assert light(large) / light(small) == pytest.approx((large / small) ** 2, rel=0.15)


def test_sub_pixel_sprites_still_add_light_that_grows_with_size():
    light = [stamp(black(), sprite(CENTRE, CENTRE, s), blend="add").sum() for s in (0.25, 0.5, 1.0, 2.0)]
    assert light[0] > 0
    assert light == sorted(light) and len(set(light)) == 4


def test_add_light_is_linear_in_opacity():
    colour = (0.9, 0.5, 0.2)
    light = lambda op: stamp(black(), sprite(CENTRE, CENTRE), color=colour, opacity=op, blend="add").sum()  # noqa: E731
    assert light(0.5) == pytest.approx(2 * light(0.25), rel=1e-4)
    assert light(1.0) == pytest.approx(4 * light(0.25), rel=1e-4)
    assert light(0.0) == 0.0


def test_nearer_sprites_are_brighter_than_farther_ones_of_the_same_size():
    far = stamp(black(), sprite(CENTRE, CENTRE, depth=0.0), blend="add").sum()
    mid = stamp(black(), sprite(CENTRE, CENTRE, depth=0.5), blend="add").sum()
    near = stamp(black(), sprite(CENTRE, CENTRE, depth=1.0), blend="add").sum()
    assert 0 < far < mid < near


def test_colour_lands_in_the_matching_channels():
    out = stamp(black(), sprite(CENTRE, CENTRE), color=(1.0, 0.0, 0.5), blend="add")
    assert out[50, 50, 0] > 0.99 and out[50, 50, 1] == 0 and out[50, 50, 2] == pytest.approx(0.5, abs=0.01)


def test_add_is_clipped_to_one_after_summing_overlaps():
    out = stamp(black(), many([CENTRE] * 5, [CENTRE] * 5), opacity=0.5, blend="add")
    assert out.max() == 1.0  # five halves sum to 2.5 and are clipped, not wrapped or rescaled
    two = stamp(black(), many([CENTRE] * 2, [CENTRE] * 2), opacity=0.25, blend="add")
    assert two[50, 50, 0] == pytest.approx(0.5, abs=1e-6)  # below the clip the sum is exact


def test_screen_of_a_single_sprite_matches_the_screen_formula_at_one_pixel():
    canvas = np.full((101, 101, 3), 0.3, dtype=np.float32)
    colour, opacity = np.array([1.0, 0.5, 0.2]), 0.6
    out = stamp(canvas, sprite(CENTRE, CENTRE, size=10), color=tuple(colour), opacity=opacity, blend="screen")
    s = colour * opacity  # alpha is 1 at the core of a size-10 sprite at depth 1
    np.testing.assert_allclose(out[50, 50], 1 - (1 - 0.3) * (1 - s), atol=1e-6)
    assert np.all(out[0, 0] == 0.3)


def test_screen_never_exceeds_one_even_with_many_stacked_sprites_and_overlaps_combine_as_screens():
    out = stamp(black(), many([CENTRE] * 50, [CENTRE] * 50), blend="screen")
    assert out.max() <= 1.0 and out[50, 50, 0] > 0.999
    two = stamp(black(), many([CENTRE] * 2, [CENTRE] * 2), opacity=0.5, blend="screen")
    assert two[50, 50, 0] == pytest.approx(1 - 0.5 * 0.5, abs=1e-6)


def test_over_with_full_opacity_replaces_the_core_with_the_colour_and_spares_the_rest():
    canvas = np.full((101, 101, 3), 0.7, dtype=np.float32)
    out = stamp(canvas, sprite(CENTRE, CENTRE, size=12), color=(0.1, 0.2, 0.3), opacity=1.0, blend="over")
    np.testing.assert_allclose(out[50, 50], (0.1, 0.2, 0.3), atol=1e-5)
    np.testing.assert_array_equal(out[:20, :20], canvas[:20, :20])


def test_over_composites_stacked_sprites_one_after_the_other():
    canvas = np.full((101, 101, 3), 0.8, dtype=np.float32)
    out = stamp(canvas, many([CENTRE] * 2, [CENTRE] * 2), color=(0.0, 0.0, 0.0), opacity=0.5, blend="over")
    assert out[50, 50, 0] == pytest.approx(0.8 * 0.5 * 0.5, abs=1e-6)


@pytest.mark.parametrize("blend", ["add", "screen", "over"])
def test_every_blend_returns_float32_in_range_for_a_busy_picture(photo, blend):
    out = stamp(photo, scatter(photo.shape[:2], 300, seed=1), blend=blend, opacity=0.9)
    assert out.dtype == np.float32 and out.shape == photo.shape
    assert out.min() >= 0.0 and out.max() <= 1.0


def test_stamp_accepts_float64_and_non_contiguous_canvases_and_returns_float32():
    wide = np.zeros((101, 202, 3), dtype=np.float64)
    canvas = wide[:, ::2]
    out = stamp(canvas, sprite(CENTRE, CENTRE))
    assert out.dtype == np.float32 and out.shape == (101, 101, 3) and out[50, 50, 0] > 0.99
    assert wide.max() == 0.0


@pytest.mark.parametrize(
    ("x", "y", "lit_band", "dark_band"),
    [
        (0.0, 50.5, (slice(None), slice(0, 3)), (slice(None), slice(30, None))),  # left edge
        (101.0, 50.5, (slice(None), slice(-3, None)), (slice(None), slice(0, 70))),  # right edge
        (50.5, 0.0, (slice(0, 3), slice(None)), (slice(30, None), slice(None))),  # top edge
        (50.5, 101.0, (slice(-3, None), slice(None)), (slice(0, 70), slice(None))),  # bottom edge
        (0.0, 0.0, (slice(0, 3), slice(0, 3)), (slice(30, None), slice(30, None))),  # corner
        (101.0, 101.0, (slice(-3, None), slice(-3, None)), (slice(0, 70), slice(0, 70))),  # opposite corner
    ],
)
def test_sprites_straddling_an_edge_are_clipped_and_do_not_wrap_around(x, y, lit_band, dark_band):
    out = stamp(black(), sprite(x, y, size=12), blend="add")
    assert out[lit_band].max() > 0.5
    assert np.all(out[dark_band] == 0)


def test_sprites_wholly_outside_leave_the_picture_untouched_even_at_absurd_distances():
    xs = [-50.0, 151.0, 50.0, 50.0, 1e12, -1e12, 3e38]
    ys = [50.0, 50.0, -50.0, 151.0, 1e12, 5.0, -3e38]
    canvas = np.full((101, 101, 3), 0.25, dtype=np.float32)
    for blend in ("add", "screen", "over"):
        np.testing.assert_array_equal(stamp(canvas, many(xs, ys), blend=blend), canvas)


def test_a_sprite_much_larger_than_the_picture_covers_all_of_it_without_blowing_up():
    out = stamp(black(20, 30), sprite(10.0, 5.0, size=20000), color=(0.5, 0.5, 0.5), opacity=1.0, blend="add")
    np.testing.assert_allclose(out, 0.5, atol=1e-6)
    off_centre = stamp(black(20, 30), sprite(-5000.0, -5000.0, size=20000), color=(0.5, 0.5, 0.5), blend="add")
    np.testing.assert_allclose(off_centre, 0.5, atol=1e-6)  # centred far outside, yet its body still covers the picture


def test_stamp_copes_with_tiny_pictures():
    out = stamp(black(1, 1), sprite(0.5, 0.5, size=4), blend="add")
    assert out.shape == (1, 1, 3) and out[0, 0, 0] > 0.99
    assert stamp(black(1, 40), many([5.5, 30.5], [0.5, 0.5]), blend="add")[0, 5, 0] > 0.5


def test_drawing_all_sprites_at_once_equals_drawing_each_alone_and_adding(photo):
    p = scatter(photo.shape[:2], 25, seed=11, size=(3.0, 30.0))
    zero = np.zeros_like(photo)
    together = stamp(zero, p, blend="add", opacity=0.02)
    one_by_one = sum(
        stamp(zero, Placement(*(getattr(p, f)[i : i + 1] for f in FIELDS)), blend="add", opacity=0.02)
        for i in range(25)
    )
    np.testing.assert_allclose(together, np.clip(one_by_one, 0, 1), atol=1e-5)
    assert together.max() < 1.0  # no clipping happened, so the comparison is meaningful


def test_stamp_rejects_non_finite_positions_naming_the_placement():
    for bad in (np.nan, np.inf, -np.inf):
        with pytest.raises(ValueError, match="placement"):
            stamp(black(), sprite(bad, 5.0))
        with pytest.raises(ValueError, match="placement"):
            stamp(black(), sprite(5.0, bad))


# ---------------------------------------------------------------- streaks


def lit_box(img: np.ndarray, threshold: float = 0.5) -> tuple[int, int]:
    """(width, height) in pixel steps (last - first lit column or row) of the pixels brighter than `threshold`."""
    rows, cols = np.nonzero(img[..., 0] > threshold)
    return int(cols.max() - cols.min()), int(rows.max() - rows.min())


@pytest.mark.parametrize("length", [2.0, 3.0, 5.0])
def test_a_horizontal_streak_is_about_length_times_wider_than_tall(length):
    out = stamp(black(), sprite(CENTRE, CENTRE, size=16), shape="streak", angle=0.0, length=length, blend="add")
    width, height = lit_box(out)
    assert width / height == pytest.approx(length, rel=0.15)
    assert height > 8  # it is the disc's width that stays 16, so a streak is not shrunk to get longer


def test_a_vertical_streak_is_about_length_times_taller_than_wide():
    out = stamp(black(), sprite(CENTRE, CENTRE, size=16), shape="streak", angle=90.0, length=3.0, blend="add")
    width, height = lit_box(out)
    assert height / width == pytest.approx(3.0, rel=0.15)


def test_a_diagonal_streak_is_as_wide_as_tall_and_lies_along_the_diagonal():
    out = stamp(black(), sprite(CENTRE, CENTRE, size=10), shape="streak", angle=45.0, length=5.0, blend="add")
    width, height = lit_box(out)
    assert abs(width - height) <= 2
    assert out[50 + 10, 50 + 10, 0] > 0.5  # down and to the right (y points down)
    assert out[50 - 10, 50 + 10, 0] == 0.0  # the other diagonal is dark


def test_a_streak_with_a_non_integer_angle_keeps_its_direction():
    out = stamp(black(), sprite(CENTRE, CENTRE, size=10), shape="streak", angle=-30.0, length=4.0, blend="add")
    assert out[50 - 8, 50 + 14, 0] > 0.5  # up and to the right: tan(30) * 14 = 8
    assert out[50 + 8, 50 + 14, 0] == 0.0


def test_a_disc_ignores_angle_and_length():
    base = stamp(black(), sprite(CENTRE, CENTRE), blend="add")
    np.testing.assert_array_equal(stamp(black(), sprite(CENTRE, CENTRE), angle=33.0, length=9.0, blend="add"), base)


def test_streak_light_grows_with_length():
    light = lambda n: stamp(black(), sprite(CENTRE, CENTRE, 8), shape="streak", length=n, blend="add").sum()  # noqa: E731
    assert light(4.0) / light(2.0) == pytest.approx(2.0, rel=0.15)


def test_streaks_are_clipped_at_the_edges_like_discs():
    out = stamp(black(), many([0.0, 101.0], [50.5, 50.5], size=8), shape="streak", angle=0.0, length=6.0, blend="add")
    assert out[50, 0, 0] > 0.9 and out[50, 100, 0] > 0.9
    assert np.all(out[:, 40:60] == 0)  # nothing wrapped to the middle
