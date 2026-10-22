import time
import tracemalloc

import numpy as np
import pytest

from photo_fx.ops import sprites as sprites_module
from photo_fx.ops.sprites import Placement, scatter, stamp

FIELDS = ("x", "y", "size", "depth", "phase")

# numpy reports overflow, invalid casts and divisions as warnings; none may happen, whatever the input
pytestmark = pytest.mark.filterwarnings("error")


# ---------------------------------------------------------------- scatter


def test_scatter_is_deterministic_per_seed_and_differs_across_seeds():
    a, b, other = scatter((40, 90), 200, seed=7), scatter((40, 90), 200, seed=7), scatter((40, 90), 200, seed=8)
    for name in FIELDS:
        np.testing.assert_array_equal(getattr(a, name), getattr(b, name))
    assert not np.array_equal(a.x, other.x)
    assert not np.array_equal(a.y, other.y)


def test_scatter_gives_a_different_placement_for_every_one_of_many_seeds():
    # a seed that were truncated (low bit, a modulus) would repeat placements among consecutive seeds
    placements = {scatter((40, 90), 20, seed=seed).x.tobytes() for seed in range(64)}
    assert len(placements) == 64
    ys = {scatter((40, 90), 20, seed=seed).y.tobytes() for seed in range(64)}
    assert len(ys) == 64


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
    single = scatter((50, 50), 300, seed=2, size=(3.0, 9.0), depth_layers=1)
    assert np.all(single.depth == 1.0) and np.all(single.size == 9.0)  # one layer = everything near and large
    assert p.phase.min() >= 0 and p.phase.max() <= 1
    assert len(np.unique(p.phase)) > 250  # a per-sprite random number, not a constant per layer


def test_scatter_uses_every_layer_even_when_there_are_barely_enough_sprites():
    for seed in range(20):
        assert len(np.unique(scatter((50, 50), 6, seed=seed, depth_layers=6).depth)) == 6


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

    def full(value: float) -> np.ndarray:
        return np.full(n, value, dtype=np.float32)

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

    def light(size):
        return stamp(black(), sprite(CENTRE, CENTRE, size), blend="add").sum()

    assert light(large) / light(small) == pytest.approx((large / small) ** 2, rel=0.15)


def test_sub_pixel_sprites_still_add_light_that_grows_with_size():
    light = [stamp(black(), sprite(CENTRE, CENTRE, s), blend="add").sum() for s in (0.25, 0.5, 1.0, 2.0)]
    assert light[0] > 0
    assert light == sorted(light) and len(set(light)) == 4


def test_add_light_is_linear_in_opacity():
    colour = (0.9, 0.5, 0.2)

    def light(opacity):
        return stamp(black(), sprite(CENTRE, CENTRE), color=colour, opacity=opacity, blend="add").sum()

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


def test_astronomically_large_sprites_and_streaks_still_just_cover_the_picture():
    for shape in ("disc", "streak"):
        out = stamp(black(20, 30), sprite(10.0, 5.0, size=1e30), color=(0.5, 0.5, 0.5), shape=shape, blend="add")
        np.testing.assert_allclose(out, 0.5, atol=1e-6)


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

    def light(length):
        return stamp(black(), sprite(CENTRE, CENTRE, 8), shape="streak", length=length, blend="add").sum()

    assert light(4.0) / light(2.0) == pytest.approx(2.0, rel=0.15)


def test_streaks_are_clipped_at_the_edges_like_discs():
    out = stamp(black(), many([0.0, 101.0], [50.5, 50.5], size=8), shape="streak", angle=0.0, length=6.0, blend="add")
    assert out[50, 0, 0] > 0.9 and out[50, 100, 0] > 0.9
    assert np.all(out[:, 40:60] == 0)  # nothing wrapped to the middle


# ---------------------------------------------------------------- arguments and speed


@pytest.mark.parametrize(
    ("kwargs", "name"),
    [
        ({"blend": "multiply"}, "blend"),
        ({"shape": "star"}, "shape"),
        ({"opacity": -0.1}, "opacity"),
        ({"opacity": 1.5}, "opacity"),
        ({"opacity": float("nan")}, "opacity"),
        ({"color": (1.0, 1.0)}, "color"),
        ({"color": (1.0, float("nan"), 1.0)}, "color"),
        ({"shape": "streak", "length": 0.5}, "length"),
        ({"shape": "streak", "angle": float("inf")}, "angle"),
    ],
)
def test_stamp_rejects_invalid_arguments_naming_them(kwargs, name):
    with pytest.raises(ValueError, match=name):
        stamp(black(), sprite(CENTRE, CENTRE), **kwargs)


@pytest.mark.parametrize("canvas", [np.zeros((10, 10)), np.zeros((10, 10, 4)), np.zeros((10, 10, 3, 1)), np.zeros(3)])
def test_stamp_rejects_a_canvas_that_is_not_hw3_naming_it(canvas):
    with pytest.raises(ValueError, match="canvas"):
        stamp(canvas, sprite(5.0, 5.0))


def test_stamp_rejects_a_placement_whose_fields_differ_in_length_or_are_not_1d():
    good = sprite(5.0, 5.0)
    with pytest.raises(ValueError, match="placement"):
        stamp(black(), Placement(good.x, np.zeros(2, np.float32), good.size, good.depth, good.phase))
    with pytest.raises(ValueError, match="placement"):
        stamp(black(), Placement(good.x[None], good.y[None], good.size[None], good.depth[None], good.phase[None]))


def test_invalid_blend_is_rejected_even_when_there_is_nothing_to_draw():
    with pytest.raises(ValueError, match="blend"):
        stamp(black(), scatter((10, 10), 0, seed=0), blend="nope")


def test_batches_of_large_sprites_equal_drawing_each_alone(monkeypatch):
    # Large sprites on a small picture, with the batch and merge limits lowered so that the drawing is split into many
    # batches and merges: the result must not depend on how the work is split.
    monkeypatch.setattr(sprites_module, "_MAX_PATCH_ELEMENTS", 40_000)
    monkeypatch.setattr(sprites_module, "_MAX_PENDING_ENTRIES", 30_000)
    p = scatter((160, 160), 40, seed=5, size=(60.0, 140.0), depth_layers=2)
    zero = np.zeros((160, 160, 3), np.float32)
    together = stamp(zero, p, blend="add", opacity=0.02)
    one_by_one = sum(
        stamp(zero, Placement(*(getattr(p, f)[i : i + 1] for f in FIELDS)), blend="add", opacity=0.02)
        for i in range(40)
    )
    assert together.max() < 1.0
    np.testing.assert_allclose(together, one_by_one, atol=1e-5)


def test_five_thousand_sprites_on_a_1600_square_picture_take_well_under_two_seconds():
    canvas = np.zeros((1600, 1600, 3), np.float32)
    placement = scatter((1600, 1600), 5000, seed=1)
    best = min(_timed(lambda: stamp(canvas, placement)) for _ in range(3))
    assert best < 1.5, f"{best:.2f} s"
    assert stamp(canvas, placement).max() > 0.5  # it really drew them


def _timed(fn) -> float:
    start = time.perf_counter()
    fn()
    return time.perf_counter() - start


# ---------------------------------------------------------------- sprites that reach in from outside


# A size-8 sprite at depth 1 has radius 4 and, by default, an edge ramp 2 px wide from d=3 to d=5, so its light at
# distance d from its centre is (5 - d) / 2. Pixel centres are at col + 0.5. Each case puts the centre OUTSIDE the
# picture (101 x 101) and names the one border pixel it reaches and the light that pixel must get.
@pytest.mark.parametrize(
    ("x", "y", "pixel", "expected"),
    [
        (-4.0, 50.5, (50, 0), 0.25),  # left: d = 4.5
        (-3.0, 50.5, (50, 0), 0.75),  # d = 3.5
        (105.0, 50.5, (50, 100), 0.25),  # right: pixel centre 100.5, d = 4.5
        (104.0, 50.5, (50, 100), 0.75),
        (50.5, -4.0, (0, 50), 0.25),  # top
        (50.5, -3.0, (0, 50), 0.75),
        (50.5, 105.0, (100, 50), 0.25),  # bottom
        (50.5, 104.0, (100, 50), 0.75),
        (-4.0, -4.0, (0, 0), 0.0),  # diagonal: d = 6.4, out of reach
        (-3.0, -3.0, (0, 0), (5 - np.hypot(3.5, 3.5)) / 2),  # corner: d = 4.95
    ],
)
def test_a_disc_centred_outside_the_picture_still_lights_the_border_pixel_it_reaches(x, y, pixel, expected):
    out = stamp(black(), sprite(x, y, size=8), blend="add")
    assert out[pixel][0] == pytest.approx(expected, abs=1e-6)


# A streak 12 px outside, 4 times longer than wide: along its axis distances shrink 4-fold, so the border pixel at
# axis distance 12.5 is at d = 3.125 and gets (5 - 3.125) / 2 = 0.9375. A disc would not reach it at all.
@pytest.mark.parametrize(
    ("x", "y", "angle", "pixel"),
    [
        (-12.0, 50.5, 0.0, (50, 0)),
        (113.0, 50.5, 180.0, (50, 100)),
        (50.5, -12.0, 90.0, (0, 50)),
        (50.5, 113.0, 270.0, (100, 50)),
    ],
)
def test_a_streak_centred_outside_reaches_in_along_its_axis(x, y, angle, pixel):
    out = stamp(black(), sprite(x, y, size=8), shape="streak", angle=angle, length=4.0, blend="add")
    assert out[pixel][0] == pytest.approx(0.9375, abs=1e-6)
    disc = stamp(black(), sprite(x, y, size=8), blend="add")
    assert disc.sum() == 0.0


def test_a_streak_pointing_away_from_the_picture_does_not_reach_it():
    out = stamp(black(), sprite(-12.0, 50.5, size=8), shape="streak", angle=90.0, length=4.0, blend="add")
    assert out.sum() == 0.0


# ---------------------------------------------------------------- memory


def test_memory_follows_what_is_drawn_not_the_size_of_the_picture():
    canvas = np.zeros((2000, 3000, 3), np.float32)  # 72 MB
    placement = scatter((2000, 3000), 20, seed=1)
    tracemalloc.start()
    try:
        out = stamp(canvas, placement)
        _, peak = tracemalloc.get_traced_memory()
    finally:
        tracemalloc.stop()
    assert out.max() > 0.5
    # the returned copy is 1x the picture; a dense float64 per-pixel accumulator would add 2x on top
    assert peak < 1.5 * canvas.nbytes, f"peak {peak / canvas.nbytes:.2f}x the picture"


# ---------------------------------------------------------------- tunable look: feather, softness, brightness


def test_feather_sets_the_minimum_edge_width_in_pixels():
    # size 2 -> radius 1; the default edge is 1 px wide: the centre pixel is fully lit, the pixel 2 px away is dark.
    # with feather 4 the ramp is (1 + 2 - d) / 4 wide: 0.75 at the centre, 0.25 two pixels away
    wide = stamp(black(), sprite(CENTRE, CENTRE, size=2), feather=4.0, blend="add")[..., 0]
    default = stamp(black(), sprite(CENTRE, CENTRE, size=2), blend="add")[..., 0]
    assert wide[50, 50] == pytest.approx(0.75, abs=1e-6) and wide[50, 52] == pytest.approx(0.25, abs=1e-6)
    assert default[50, 50] == 1.0 and default[50, 52] == 0.0


def test_softness_sets_the_edge_width_as_a_fraction_of_the_radius():
    # size 40 -> radius 20. softness 0.5 (default): ramp 10 px, from d=15 to 25, value (25 - d) / 10.
    # softness 0.1: ramp 2 px, from d=19 to 21, value (21 - d) / 2.
    default = stamp(black(), sprite(CENTRE, CENTRE, size=40), blend="add")[50]
    sharp = stamp(black(), sprite(CENTRE, CENTRE, size=40), softness=0.1, blend="add")[50]
    for column, want_default, want_sharp in [(69, 0.6, 1.0), (70, 0.5, 0.5), (71, 0.4, 0.0)]:
        assert default[column, 0] == pytest.approx(want_default, abs=1e-6)
        assert sharp[column, 0] == pytest.approx(want_sharp, abs=1e-6)


def test_far_and_near_brightness_set_the_brightness_range_over_depth():
    def core(depth, **kwargs):
        return stamp(black(), sprite(CENTRE, CENTRE, depth=depth), blend="add", **kwargs)[50, 50, 0]

    for depth, want in [(0.0, 0.1), (0.5, 0.45), (1.0, 0.8)]:
        assert core(depth, far_brightness=0.1, near_brightness=0.8) == pytest.approx(want, abs=1e-6)
    assert core(0.0) == pytest.approx(0.4, abs=1e-6) and core(1.0) == 1.0  # defaults: 0.4 far, 1 near


def test_over_at_a_far_depth_is_dimmed_by_the_far_brightness_unless_it_is_raised_to_one():
    canvas = np.full((101, 101, 3), 0.7, dtype=np.float32)
    dim = stamp(canvas, sprite(CENTRE, CENTRE, depth=0.0), color=(0, 0, 0), blend="over")
    assert dim[50, 50, 0] == pytest.approx(0.7 * (1 - 0.4), abs=1e-6)
    solid = stamp(canvas, sprite(CENTRE, CENTRE, depth=0.0), color=(0, 0, 0), blend="over", far_brightness=1.0)
    assert solid[50, 50, 0] == pytest.approx(0.0, abs=1e-5)


@pytest.mark.parametrize(
    ("kwargs", "name"),
    [
        ({"feather": 0.0}, "feather"),
        ({"feather": -1.0}, "feather"),
        ({"feather": float("nan")}, "feather"),
        ({"softness": -0.1}, "softness"),
        ({"softness": float("inf")}, "softness"),
        ({"far_brightness": -0.1}, "far_brightness"),
        ({"far_brightness": 1.1}, "far_brightness"),
        ({"near_brightness": 1.1}, "near_brightness"),
        ({"near_brightness": float("nan")}, "near_brightness"),
    ],
)
def test_the_look_parameters_are_validated_naming_them(kwargs, name):
    with pytest.raises(ValueError, match=name):
        stamp(black(), sprite(CENTRE, CENTRE), **kwargs)


def test_a_black_sprite_under_add_changes_nothing_not_even_an_out_of_range_pixel():
    canvas = np.full((101, 101, 3), 1.5, dtype=np.float32)  # out of range on purpose: any clipping would show
    out = stamp(canvas, sprite(CENTRE, CENTRE), color=(0.0, 0.0, 0.0), blend="add")
    np.testing.assert_array_equal(out, canvas)


# ---------------------------------------------------------------- the documented profile


def documented_profile(distance: np.ndarray, size: float, feather: float = 1.0, softness: float = 0.5) -> np.ndarray:
    """The brightness profile the `stamp` docstring promises, written out from its formula (radius r = size / 2,
    edge width w = max(feather, softness * r), alpha = clip((r + w / 2 - d) / w, 0, 1))."""
    radius = size / 2
    width = max(feather, softness * radius)
    return np.clip((radius + width / 2 - distance) / width, 0.0, 1.0)


@pytest.mark.parametrize(
    ("size", "kwargs"),
    [
        (20.0, {}),  # width 5: core to d = 7.5, ramp to d = 12.5
        (20.0, {"softness": 0.2}),  # width 2
        (20.0, {"feather": 6.0}),  # width 6 beats softness * r = 5
        (20.0, {"softness": 2.0}),  # width 20 = size: no flat core, the peak is exactly 1
        (20.0, {"softness": 3.0}),  # width 30: the peak is dimmed to (r + w / 2) / w = 25 / 30
        (3.0, {}),  # feather decides: width 1
        (4.0, {"feather": 4.0}),  # a sprite as wide as the feather still peaks at 1
        (4.0, {"feather": 8.0}),  # wider than the sprite: a dim blob
    ],
)
def test_the_profile_along_a_row_through_the_centre_is_the_documented_flat_core_and_linear_ramp(size, kwargs):
    out = stamp(black(), sprite(CENTRE, CENTRE, size), blend="add", **kwargs)[50, :, 0]
    distance = np.abs(np.arange(101) - 50.0)  # pixel centres are at col + 0.5, the sprite at 50.5
    np.testing.assert_allclose(out, documented_profile(distance, size, **kwargs), atol=1e-6)


def test_a_streaks_profile_is_the_same_curve_with_distances_along_its_axis_divided_by_length():
    out = stamp(black(), sprite(CENTRE, CENTRE, 8.0), shape="streak", angle=0.0, length=4.0, blend="add")[50, :, 0]
    distance = np.abs(np.arange(101) - 50.0) / 4.0
    np.testing.assert_allclose(out, documented_profile(distance, 8.0), atol=1e-6)
    across = stamp(black(), sprite(CENTRE, CENTRE, 8.0), shape="streak", angle=0.0, length=4.0, blend="add")[:, 50, 0]
    np.testing.assert_allclose(across, documented_profile(np.abs(np.arange(101) - 50.0), 8.0), atol=1e-6)


@pytest.mark.parametrize("kwargs", [{}, {"softness": 1.0}, {"feather": 6.0}, {"softness": 0.1, "feather": 2.0}])
def test_total_light_of_a_sprite_is_pi_times_radius_squared_plus_a_twelfth_of_the_edge_width_squared(kwargs):
    size = 20.0
    radius = size / 2
    width = max(kwargs.get("feather", 1.0), kwargs.get("softness", 0.5) * radius)
    out = stamp(black(), sprite(CENTRE, CENTRE, size), blend="add", **kwargs)
    assert out[..., 0].sum() == pytest.approx(np.pi * (radius**2 + width**2 / 12), rel=2e-3)


@pytest.mark.parametrize(("size", "tolerance"), [(12.0, 0.05), (6.0, 0.05), (3.0, 0.05), (2.5, 0.10), (2.0, 0.13)])
@pytest.mark.parametrize("offset", [(0.5, 0.5), (0.0, 0.0), (0.5, 0.0)])  # centred, on a pixel corner, on an edge
def test_light_stays_within_the_documented_distance_of_the_disc_area_at_default_settings(size, tolerance, offset):
    out = stamp(black(), sprite(50.0 + offset[0], 50.0 + offset[1], size), blend="add")
    assert out[..., 0].sum() == pytest.approx(np.pi * size**2 / 4, rel=tolerance)


def test_scatter_sizes_of_middle_layers_are_interpolated_linearly_in_depth():
    p = scatter((50, 50), 300, seed=4, size=(2.0, 8.0), depth_layers=3)
    assert sorted(set(p.size.tolist())) == [2.0, 5.0, 8.0]
    q = scatter((50, 50), 300, seed=4, size=(2.0, 12.0), depth_layers=5)
    assert sorted(set(q.size.tolist())) == [2.0, 4.5, 7.0, 9.5, 12.0]


# ---------------------------------------------------------------- the accumulation strategy is invisible


@pytest.mark.parametrize("blend", ["add", "screen", "over"])
@pytest.mark.parametrize(
    ("shape", "count", "size", "extra"),
    [
        ((60, 90), 300, (2.0, 8.0), {}),
        ((61, 47), 40, (5.0, 60.0), {"opacity": 0.6, "color": (0.9, 0.4, 0.1)}),
        ((70, 50), 120, (3.0, 20.0), {"shape": "streak", "angle": 37.0, "length": 5.0}),
        ((120, 150), 40, (40.0, 90.0), {"opacity": 0.03}),  # overlapping big sprites; the sparse path merges (below)
    ],
)
def test_sparse_and_dense_accumulation_give_byte_identical_pictures(monkeypatch, blend, shape, count, size, extra):
    rng = np.random.default_rng(count)
    canvas = rng.random((*shape, 3), dtype=np.float32)
    placement = scatter(shape, count, seed=count, size=size)
    monkeypatch.setattr(sprites_module, "_MAX_PENDING_ENTRIES", 3000)  # so that the sparse path merges repeatedly
    results = []
    for block in (
        sprites_module._BLOCK,
        513,
    ):  # 513: many small blocks, to exercise the block-wise compaction and resolve
        monkeypatch.setattr(sprites_module, "_BLOCK", block)
        for dense in (False, True):
            monkeypatch.setattr(sprites_module, "_prefers_dense", lambda drawn, pixels, dense=dense: dense)
            results.append(stamp(canvas, placement, blend=blend, **extra))
    assert not np.array_equal(results[0], canvas)
    for other in results[1:]:
        np.testing.assert_array_equal(other, results[0])


@pytest.mark.parametrize("dense", [False, True])
def test_pixels_no_sprite_touches_come_back_unchanged_to_the_last_bit_under_either_accumulation(monkeypatch, dense):
    monkeypatch.setattr(sprites_module, "_prefers_dense", lambda drawn, pixels: dense)
    canvas = np.full((101, 101, 3), 1.5, dtype=np.float32)  # out of range on purpose: any clipping would show
    out = stamp(canvas, sprite(CENTRE, CENTRE, size=10), blend="add", opacity=0.5)
    assert np.all(out[:30] == 1.5) and np.all(out[:, :30] == 1.5)
    assert out[50, 50, 0] == 1.0  # touched pixels are clipped


def test_the_accumulation_strategy_follows_how_much_of_the_picture_is_drawn_on(monkeypatch):
    # A guard for speed without a stopwatch: few small sprites are summed sparsely (memory follows the area drawn),
    # sprites covering the picture many times over are summed per pixel (sorting millions of entries is far slower).
    used = []
    for name in ("_accumulate_sparse", "_accumulate_dense"):
        original = getattr(sprites_module, name)
        monkeypatch.setattr(
            sprites_module, name, lambda *a, original=original, name=name: used.append(name) or original(*a)
        )
    stamp(black(200, 300), scatter((200, 300), 30, seed=1))
    stamp(black(200, 300), scatter((200, 300), 30, seed=1, size=(60.0, 90.0)))
    assert used == ["_accumulate_sparse", "_accumulate_dense"]


def test_a_sprite_larger_than_the_picture_costs_about_the_picture_not_several_times_it():
    canvas = np.zeros((1000, 1000, 3), np.float32)  # 12 MB
    tracemalloc.start()
    try:
        out = stamp(canvas, sprite(500.0, 500.0, size=1e5), color=(0.5, 0.5, 0.5), blend="add")
        _, peak = tracemalloc.get_traced_memory()
    finally:
        tracemalloc.stop()
    np.testing.assert_allclose(out, 0.5, atol=1e-6)
    # measured: about 8x with one shared window, about 14x with a window four times the picture per sprite
    assert peak < 11 * canvas.nbytes, f"peak {peak / canvas.nbytes:.1f}x the picture"
