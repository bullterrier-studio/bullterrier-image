import time

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
    assert labels.shape == (7, 11) and (labels == 0).all()


def test_cell_labels_give_every_pixel_one_valid_int32_label():
    shape = (37, 53)
    points = voronoi.random_points(shape, 12, seed=5)
    labels = voronoi.cell_labels(shape, points)
    assert labels.dtype == np.int32 and labels.shape == shape
    assert labels.min() >= 0 and labels.max() < 12


def test_cell_labels_follow_point_order_not_position():
    # left point listed second: a mix-up of the point index with its sort order would flip the labels
    points = np.array([[15.0, 2.0], [5.0, 2.0]], dtype=np.float32)
    labels = voronoi.cell_labels((4, 20), points)
    assert (labels[:, :10] == 1).all() and (labels[:, 10:] == 0).all()


def test_cell_labels_reject_no_points():
    with pytest.raises(ValueError):
        voronoi.cell_labels((5, 5), np.zeros((0, 2), dtype=np.float32))


def test_cell_labels_of_a_large_picture_are_fast():
    points = voronoi.random_points((1600, 1600), 2000, seed=0)
    start = time.perf_counter()
    voronoi.cell_labels((1600, 1600), points)
    assert time.perf_counter() - start < 1.5


# ---------------------------------------------------------------- cell_means


def test_cell_means_of_a_two_colour_image_split_on_a_voronoi_boundary_give_both_colours_exactly():
    shape = (9, 20)
    points = np.array([[4.0, 4.0], [15.0, 4.0]], dtype=np.float32)  # boundary at x = 9.5
    labels = voronoi.cell_labels(shape, points)
    colour_a = np.array([0.2, 0.7, 0.35], dtype=np.float32)
    colour_b = np.array([0.9, 0.1, 0.6], dtype=np.float32)
    image = np.where(labels[:, :, None] == 0, colour_a, colour_b).astype(np.float32)
    means = voronoi.cell_means(image, labels, 2)
    assert means.shape == (2, 3) and means.dtype == np.float32
    assert np.array_equal(means[0], colour_a) and np.array_equal(means[1], colour_b)


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
    assert np.array_equal(means[0], [0.5, 0.5, 0.5]) and (means[1:] == 0).all()


def test_cell_means_of_a_grayscale_image_have_one_channel():
    labels = np.array([[0, 0, 1], [0, 1, 1]], dtype=np.int32)
    image = np.array([[0.0, 0.5, 1.0], [1.0, 0.25, 0.5]], dtype=np.float32)
    means = voronoi.cell_means(image, labels, 2)
    assert means.shape == (2, 1)
    assert np.allclose(means[:, 0], [0.5, 0.58333333])


def test_cell_means_reject_labels_outside_the_cell_count():
    labels = np.array([[0, 3]], dtype=np.int32)
    with pytest.raises(ValueError):
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
