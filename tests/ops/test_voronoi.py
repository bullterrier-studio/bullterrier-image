import time

import numpy as np
import pytest
from scipy.spatial import ConvexHull

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
    assert pts.shape == (500, 2)
    assert pts.dtype == np.float32
    assert pts[:, 0].min() >= 0
    assert pts[:, 0].max() < 80
    assert pts[:, 1].min() >= 0
    assert pts[:, 1].max() < 30
    # x spans the width and y the height: a swapped (h, w) or (x, y) would break one of these
    assert pts[:, 0].max() > 30
    assert pts[:, 1].max() > 20


def test_random_points_never_reach_the_far_edge_even_when_rounding_to_float32_would_put_them_on_it():
    # Among numpy's draws for seed 177 is 1 - 3e-8 (point 59228). Scaled by 17 and rounded to float32 it is exactly
    # 17.0, on the far edge, outside the picture. Found by searching seeds; a draw that large is a one-in-10^7 event.
    pts = voronoi.random_points((17, 17), 59_229, seed=177)
    assert pts.max() < 17
    assert pts.min() >= 0
    assert pts.max() > 16.99999  # the draw really came that close to the edge


def test_random_points_stay_in_half_open_bounds_for_many_seeds():
    for seed in range(300):
        pts = voronoi.random_points((7, 3), 200, seed=seed)
        assert (pts >= 0).all()
        assert (pts[:, 0] < 3).all()
        assert (pts[:, 1] < 7).all()


def test_random_points_cover_the_picture_uniformly():
    shape = (30, 80)
    pts = voronoi.random_points(shape, 10000, seed=2)
    # 4 x 4 grid of equal tiles: each expects 625 points (standard deviation about 24); the tolerance is over 6 sigma
    counts, _, _ = np.histogram2d(pts[:, 1], pts[:, 0], bins=4, range=[[0, 30], [0, 80]])
    assert np.abs(counts - 625).max() < 150


def test_random_points_zero_count_is_empty():
    assert voronoi.random_points((10, 10), 0, seed=0).shape == (0, 2)


def test_random_points_reject_negative_count():
    with pytest.raises(ValueError, match=r"count.*-1"):
        voronoi.random_points((10, 10), -1, seed=0)


# ---------------------------------------------------------------- cell_labels


def _pixel_distances(shape, points):
    """(h, w, n) distance from every pixel centre to every point, by brute force."""
    h, w = shape
    ys, xs = np.mgrid[0:h, 0:w]
    centres = np.stack([xs + 0.5, ys + 0.5], axis=-1).astype(np.float64)
    return np.linalg.norm(centres[:, :, None, :] - points[None, None, :, :].astype(np.float64), axis=-1)


def test_cell_labels_pick_the_nearest_point_for_every_pixel():
    shape = (37, 53)
    points = voronoi.random_points(shape, 12, seed=5)
    labels = voronoi.cell_labels(shape, points)
    dist = _pixel_distances(shape, points)
    chosen = np.take_along_axis(dist, labels[:, :, None].astype(np.intp), axis=2)[:, :, 0]
    assert np.allclose(chosen, dist.min(axis=2))


def test_cell_labels_with_one_point_are_all_zero():
    labels = voronoi.cell_labels((7, 11), np.array([[3.0, 2.0]], dtype=np.float32))
    assert labels.shape == (7, 11)
    assert (labels == 0).all()


def test_cell_labels_give_every_pixel_one_valid_int32_label():
    shape = (37, 53)
    points = voronoi.random_points(shape, 12, seed=5)
    labels = voronoi.cell_labels(shape, points)
    assert labels.dtype == np.int32
    assert labels.shape == shape
    assert labels.min() >= 0
    assert labels.max() < 12


def test_cell_labels_follow_point_order_not_position():
    # left point listed second: a mix-up of the point index with its sort order would flip the labels
    points = np.array([[15.0, 2.0], [5.0, 2.0]], dtype=np.float32)
    labels = voronoi.cell_labels((4, 20), points)
    assert (labels[:, :10] == 1).all()
    assert (labels[:, 10:] == 0).all()


def test_cell_labels_reject_no_points():
    with pytest.raises(ValueError, match="at least one point"):
        voronoi.cell_labels((5, 5), np.zeros((0, 2), dtype=np.float32))


def test_cell_labels_of_a_large_picture_are_fast():
    # CPU time of this process (all its threads), best of three: a busy machine does not fail a correct implementation.
    # About 1.2 s here; a per-pixel loop or a full distance matrix would take minutes.
    points = voronoi.random_points((1600, 1600), 2000, seed=0)
    timings = []
    for _ in range(3):
        start = time.process_time()
        voronoi.cell_labels((1600, 1600), points)
        timings.append(time.process_time() - start)
    assert min(timings) < 4.0


# ---------------------------------------------------------------- cell_means


def test_cell_means_of_a_two_colour_image_split_on_a_voronoi_boundary_give_both_colours_exactly():
    shape = (9, 20)
    points = np.array([[4.0, 4.0], [15.0, 4.0]], dtype=np.float32)  # boundary at x = 9.5
    labels = voronoi.cell_labels(shape, points)
    colour_a = np.array([0.2, 0.7, 0.35], dtype=np.float32)
    colour_b = np.array([0.9, 0.1, 0.6], dtype=np.float32)
    image = np.where(labels[:, :, None] == 0, colour_a, colour_b).astype(np.float32)
    means = voronoi.cell_means(image, labels, 2)
    assert means.shape == (2, 3)
    assert means.dtype == np.float32
    assert np.array_equal(means[0], colour_a)
    assert np.array_equal(means[1], colour_b)


def test_cell_means_weighted_by_cell_size_equal_the_image_mean():
    shape = (31, 47)
    labels = voronoi.cell_labels(shape, voronoi.random_points(shape, 20, seed=2))
    image = np.random.default_rng(0).random((*shape, 3)).astype(np.float32)
    means = voronoi.cell_means(image, labels, 20)
    sizes = np.bincount(labels.ravel(), minlength=20)
    assert np.allclose(np.average(means, axis=0, weights=sizes), image.mean(axis=(0, 1)), atol=1e-6)


def test_cell_means_of_each_cell_match_a_masked_mean():
    shape = (12, 17)
    labels = voronoi.cell_labels(shape, voronoi.random_points(shape, 5, seed=8))
    image = np.random.default_rng(1).random((*shape, 3)).astype(np.float32)
    means = voronoi.cell_means(image, labels, 5)
    for k in range(5):
        assert np.allclose(means[k], image[labels == k].mean(axis=0), atol=1e-6)


def test_cell_means_of_a_cell_without_pixels_are_zero():
    labels = np.zeros((4, 4), dtype=np.int32)
    image = np.full((4, 4, 3), 0.5, dtype=np.float32)
    means = voronoi.cell_means(image, labels, 3)
    assert np.array_equal(means[0], [0.5, 0.5, 0.5])
    assert (means[1:] == 0).all()


def test_cell_means_of_a_grayscale_image_have_one_channel():
    labels = np.array([[0, 0, 1], [0, 1, 1]], dtype=np.int32)
    image = np.array([[0.0, 0.5, 1.0], [1.0, 0.25, 0.5]], dtype=np.float32)
    means = voronoi.cell_means(image, labels, 2)
    assert means.shape == (2, 1)
    assert np.allclose(means[:, 0], [0.5, 0.58333333])


def test_cell_means_reject_an_image_whose_size_differs_from_the_labels():
    labels = np.zeros((2, 3), dtype=np.int32)
    with pytest.raises(ValueError, match="shape"):
        voronoi.cell_means(np.zeros((2, 6), dtype=np.float32), labels, 1)  # would silently read as 2 channels
    with pytest.raises(ValueError, match="shape"):
        voronoi.cell_means(np.zeros((3, 2, 3), dtype=np.float32), labels, 1)


def test_cell_means_reject_labels_outside_the_cell_count():
    labels = np.array([[0, 3]], dtype=np.int32)
    with pytest.raises(ValueError, match="labels must lie in"):
        voronoi.cell_means(np.zeros((1, 2, 3), dtype=np.float32), labels, 3)


# ---------------------------------------------------------------- cell_borders


def test_cell_borders_of_a_vertical_split_are_a_one_pixel_wide_vertical_line():
    labels = np.zeros((6, 10), dtype=np.int32)
    labels[:, 5:] = 1
    expected = np.zeros((6, 10), dtype=bool)
    expected[:, 4] = True  # the last pixel of the left cell, whose right neighbour differs
    borders = voronoi.cell_borders(labels)
    assert borders.dtype == bool
    assert np.array_equal(borders, expected)


def test_cell_borders_of_a_horizontal_split_are_a_one_pixel_high_line():
    labels = np.zeros((6, 10), dtype=np.int32)
    labels[2:, :] = 1
    expected = np.zeros((6, 10), dtype=bool)
    expected[1, :] = True
    assert np.array_equal(voronoi.cell_borders(labels), expected)


def test_cell_borders_of_a_single_cell_are_empty():
    assert not voronoi.cell_borders(np.zeros((5, 8), dtype=np.int32)).any()


def test_cell_borders_flag_the_pixel_before_a_change_never_the_one_after():
    labels = np.array([[0, 0, 0], [0, 1, 0], [0, 0, 0]], dtype=np.int32)
    expected = np.array([[False, True, False], [True, True, False], [False, False, False]])
    assert np.array_equal(voronoi.cell_borders(labels), expected)


# ---------------------------------------------------------------- relax


def _area_spread(shape, points):
    """Coefficient of variation of the Voronoi cell areas (pixel counts)."""
    areas = np.bincount(voronoi.cell_labels(shape, points).ravel(), minlength=len(points))
    return areas.std() / areas.mean()


def test_relax_makes_cell_areas_more_even():
    shape = (120, 160)
    points = voronoi.random_points(shape, 60, seed=0)
    relaxed = voronoi.relax(points, shape, iterations=3)
    assert _area_spread(shape, relaxed) < 0.7 * _area_spread(shape, points)


def test_relax_makes_cell_areas_more_even_on_a_large_picture():
    shape = (700, 1000)  # bigger than the sampling grid relax uses internally
    points = voronoi.random_points(shape, 80, seed=4)
    relaxed = voronoi.relax(points, shape, iterations=3)
    assert _area_spread(shape, relaxed) < 0.7 * _area_spread(shape, points)


def test_relax_keeps_points_inside_the_picture_even_from_the_corner():
    shape = (30, 80)
    points = np.vstack([voronoi.random_points(shape, 40, seed=1), [[0.0, 0.0]]]).astype(np.float32)
    relaxed = voronoi.relax(points, shape, iterations=5)
    assert relaxed.shape == points.shape
    assert relaxed.dtype == np.float32
    assert relaxed[:, 0].min() >= 0
    assert relaxed[:, 0].max() < 80
    assert relaxed[:, 1].min() >= 0
    assert relaxed[:, 1].max() < 30


def test_relax_with_zero_iterations_returns_the_input_values_in_a_new_array():
    points = voronoi.random_points((30, 80), 10, seed=1)
    result = voronoi.relax(points, (30, 80), iterations=0)
    assert np.array_equal(result, points)
    assert result is not points


def test_relax_is_deterministic_and_leaves_its_input_untouched():
    points = voronoi.random_points((30, 80), 10, seed=1)
    before = points.copy()
    first = voronoi.relax(points, (30, 80))
    second = voronoi.relax(points, (30, 80))
    assert np.array_equal(first, second)
    assert np.array_equal(points, before)
    assert not np.array_equal(first, points)


def test_relax_moves_a_single_point_to_the_centre_of_a_non_square_picture():
    relaxed = voronoi.relax(np.array([[3.0, 4.0]], dtype=np.float32), (20, 60), iterations=1)
    assert np.allclose(relaxed, [[30.0, 10.0]])


def test_relax_moves_each_point_to_the_centroid_of_its_cell():
    # the boundary between the points is x = 18 (a pixel edge), so the cells are x in [0, 18) and [18, 40), y in [0, 20)
    points = np.array([[5.0, 3.0], [31.0, 3.0]], dtype=np.float32)
    relaxed = voronoi.relax(points, (20, 40), iterations=1)
    assert np.allclose(relaxed, [[9.0, 10.0], [29.0, 10.0]])


def test_relax_leaves_a_point_whose_cell_has_no_pixels_where_it_was():
    points = np.array([[5.0, 10.0], [5.0, 10.0], [31.0, 10.0]], dtype=np.float32)  # the first two coincide
    relaxed = voronoi.relax(points, (20, 40), iterations=1)
    assert np.isfinite(relaxed).all()
    assert 5.0 in relaxed[:, 0]
    assert np.isclose(relaxed[:, 0], 9.0).any()


def test_relax_rejects_negative_iterations():
    with pytest.raises(ValueError, match=r"iterations.*-1"):
        voronoi.relax(voronoi.random_points((10, 10), 3, seed=0), (10, 10), iterations=-1)


# ---------------------------------------------------------------- triangulate


def _triangle_areas(points, triangles):
    a, b, c = (points[triangles[:, k]].astype(np.float64) for k in range(3))
    return 0.5 * np.abs((b[:, 0] - a[:, 0]) * (c[:, 1] - a[:, 1]) - (c[:, 0] - a[:, 0]) * (b[:, 1] - a[:, 1]))


def test_triangulate_unit_square_with_centre_gives_the_four_expected_triangles():
    points = np.array([[0, 0], [1, 0], [1, 1], [0, 1], [0.5, 0.5]], dtype=np.float32)
    triangles = voronoi.triangulate(points)
    assert triangles.shape == (4, 3)
    assert {frozenset(t) for t in triangles.tolist()} == {
        frozenset({0, 1, 4}),
        frozenset({1, 2, 4}),
        frozenset({2, 3, 4}),
        frozenset({3, 0, 4}),
    }


def test_triangulate_gives_non_degenerate_triangles_that_cover_the_convex_hull():
    points = voronoi.random_points((40, 90), 40, seed=6)
    triangles = voronoi.triangulate(points)
    areas = _triangle_areas(points, triangles)
    assert (areas > 0).all()
    assert np.isclose(areas.sum(), ConvexHull(points.astype(np.float64)).volume, rtol=1e-5)  # in 2-D, volume is area


def test_triangulate_of_a_regular_grid_has_no_zero_area_triangles():
    xs, ys = np.meshgrid(np.arange(5), np.arange(4))  # many cocircular points: the hard case for Delaunay
    points = np.stack([xs.ravel(), ys.ravel()], axis=1).astype(np.float32)
    assert (_triangle_areas(points, voronoi.triangulate(points)) > 0).all()


@pytest.mark.parametrize("count", [0, 1, 2])
def test_triangulate_rejects_fewer_than_three_points(count):
    with pytest.raises(ValueError, match=f"at least 3 points, got {count}"):
        voronoi.triangulate(voronoi.random_points((10, 10), count, seed=0))


def test_triangulate_rejects_collinear_points():
    points = np.array([[0, 0], [1, 1], [2, 2], [5, 5]], dtype=np.float32)
    with pytest.raises(ValueError, match="collinear"):
        voronoi.triangulate(points)


@pytest.mark.parametrize(
    "points",
    [
        np.array(
            [[0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1]], dtype=np.float32
        ),  # distinct 3-D points: a valid tetrahedron
        np.array([0, 1, 2, 5, 9], dtype=np.float32),  # 1-D
        np.array([[[0], [0]], [[1], [0]], [[0], [1]]], dtype=np.float32),  # (3, 2, 1)
    ],
    ids=["n_by_3", "1d", "n_by_2_by_1"],
)
def test_triangulate_rejects_points_that_are_not_an_n_by_2_array(points):
    with pytest.raises(ValueError, match="shape"):
        voronoi.triangulate(points)


def test_triangulate_ignores_a_repeated_point():
    points = np.array([[0, 0], [1, 0], [0, 1], [0, 0]], dtype=np.float32)
    triangles = voronoi.triangulate(points)
    assert len(triangles) == 1
    assert len({0, 3} & set(triangles[0].tolist())) == 1


def test_triangulate_rejects_fewer_than_three_distinct_points():
    with pytest.raises(ValueError, match="collinear"):
        voronoi.triangulate(np.array([[0, 0], [0, 0], [1, 1]], dtype=np.float32))
