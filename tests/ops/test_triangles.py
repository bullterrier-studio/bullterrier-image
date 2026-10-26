"""Tests of `ops/triangles.py`: the picture cut into the Delaunay triangles of some sites and its four corners, against
brute-force references written here."""

import numpy as np
import pytest

from bullterrier_image.ops.triangles import triangles

SHAPE = (36, 48)
SPACING = 12.0  # px: three parts down a side of 36 and four across one of 48
BORDER_POINTS = 2 * 4 + 2 * 3  # round the picture at that spacing


def random_sites(count: int, seed: int = 0) -> np.ndarray:
    return np.random.default_rng(seed).random((count, 2)) * (SHAPE[1], SHAPE[0])


def pixel_centres(shape: tuple[int, int]) -> np.ndarray:
    """(height, width, 2): the (x, y) of every pixel's centre."""
    y, x = np.mgrid[0 : shape[0], 0 : shape[1]]
    return np.stack([x + 0.5, y + 0.5], axis=-1)


def inside(points: np.ndarray, corners: np.ndarray) -> np.ndarray:
    """Which of `points` lie in the triangle `corners` (3, 2), edges included, by the signs of the cross products."""
    signs = []
    for start, end in ((0, 1), (1, 2), (2, 0)):
        side, to = corners[end] - corners[start], points - corners[start]
        signs.append(side[0] * to[..., 1] - side[1] * to[..., 0])
    return (np.all(np.stack(signs) >= -1e-9, axis=0)) | (np.all(np.stack(signs) <= 1e-9, axis=0))


def segment_distance(points: np.ndarray, start: np.ndarray, end: np.ndarray) -> np.ndarray:
    """The distance from each of `points` (..., 2) to the segment from `start` to `end`."""
    along = end - start
    share = np.clip(((points - start) @ along) / (along @ along), 0, 1)
    return np.hypot(*np.moveaxis(points - (start + share[..., None] * along), -1, 0))


def test_the_triangles_of_n_interior_points_and_b_border_points_number_two_n_plus_b_minus_two() -> None:
    """Euler's formula for a triangulation of m points with h on the hull: 2 m - 2 - h triangles. Here m = n + b and
    every border point is on the hull."""
    assert len(triangles(SHAPE, random_sites(11), spacing=SPACING).vertices) == 2 * 11 + BORDER_POINTS - 2


def test_a_spacing_that_does_not_divide_a_side_is_rounded_to_the_nearest_whole_number_of_parts() -> None:
    """48 / 13 = 3.7 parts across and 36 / 13 = 2.8 down: 4 and 3, so 2 * 4 + 2 * 3 = 14 border points, not 10."""
    found = triangles(SHAPE, random_sites(11), spacing=13.0)
    assert len(found.vertices) == 2 * 11 + 14 - 2


def test_the_border_points_lie_every_twelve_px_round_the_edge_and_include_the_corners() -> None:
    found = triangles(SHAPE, random_sites(5, seed=4), spacing=SPACING)
    seen = {tuple(vertex) for triangle in found.vertices for vertex in triangle}
    height, width = SHAPE
    corners = {(0.0, 0.0), (width, 0.0), (0.0, height), (width, height)}
    top = {(x, 0.0) for x in range(0, width, 12)}
    right_side = {(float(width), y) for y in range(0, height, 12)}
    assert corners <= seen
    assert top <= seen
    assert right_side <= seen


def test_a_spacing_larger_than_the_picture_still_gives_the_four_corners_and_covers_it() -> None:
    found = triangles((10, 12), np.array([[3.0, 4.0]]), spacing=1000.0)
    assert found.owner.min() >= 0
    assert len(found.vertices) == 4  # one site and the four corners: 2 * 1 + 4 - 2


@pytest.mark.parametrize("count", [1, 4, 30])
def test_every_pixel_belongs_to_a_triangle_that_contains_its_centre(count: int) -> None:
    found = triangles(SHAPE, random_sites(count, seed=count), spacing=SPACING)
    centres = pixel_centres(SHAPE)
    for index, corners in enumerate(found.vertices):
        mine = found.owner == index
        assert inside(centres[mine], corners).all(), index
    assert found.owner.min() >= 0
    assert found.owner.max() < len(found.vertices)


def test_a_pixels_border_distance_is_its_distance_to_the_nearest_side_shared_with_another_triangle() -> None:
    """The picture's own sides are no border. Inside, the distance to a triangle's sides is exact; where a side lies on
    the picture's edge it is left out, and the distance to the other two sides' lines may run a little short of the
    distance to the sides themselves, never beyond."""
    height, width = SHAPE
    found = triangles(SHAPE, random_sites(9, seed=2), spacing=SPACING)
    centres = pixel_centres(SHAPE)
    for index, corners in enumerate(found.vertices):
        mine = found.owner == index
        points = centres[mine]
        sides = [(corners[a], corners[b]) for a, b in ((0, 1), (1, 2), (2, 0))]
        shared = [
            (a, b)
            for a, b in sides
            if not ((a[0] == b[0] == 0) or (a[0] == b[0] == width) or (a[1] == b[1] == 0) or (a[1] == b[1] == height))
        ]
        expected = np.min([segment_distance(points, a, b) for a, b in shared], axis=0)
        got = found.border_distance[mine]
        if len(shared) == len(sides):
            np.testing.assert_allclose(got, expected, atol=1e-6)
        else:
            assert (got <= expected + 1e-6).all()
            assert (got >= 0).all()


def test_the_distance_to_a_shared_side_is_exact_in_a_hand_worked_case() -> None:
    """Four corners and one site in the middle of a 20 x 20 picture: four triangles that meet at the middle, on the
    two diagonals. A pixel at (x, y) is |x - y| / sqrt(2) or |x + y - 20| / sqrt(2) from the diagonals, whichever is
    less."""
    found = triangles((20, 20), np.array([[10.0, 10.0]]), spacing=20.0)
    x, y = np.meshgrid(np.arange(20) + 0.5, np.arange(20) + 0.5)
    np.testing.assert_allclose(
        found.border_distance, np.minimum(np.abs(x - y), np.abs(x + y - 20)) / np.sqrt(2), atol=1e-9
    )
    assert len(found.vertices) == 4


def test_sites_crowded_in_one_place_still_cover_the_whole_picture() -> None:
    crowded = np.random.default_rng(5).random((12, 2)) * (6, 5) + (20, 15)
    found = triangles(SHAPE, crowded, spacing=SPACING)
    assert found.owner.min() >= 0
    assert found.owner[0, 0] != found.owner[-1, -1]  # the corners of the picture lie in triangles of their own


def test_the_same_sites_give_the_same_pieces() -> None:
    sites = random_sites(7, seed=3)
    first, second = triangles(SHAPE, sites, spacing=SPACING), triangles(SHAPE, sites, spacing=SPACING)
    np.testing.assert_array_equal(first.owner, second.owner)
    np.testing.assert_array_equal(first.border_distance, second.border_distance)
    np.testing.assert_array_equal(first.vertices, second.vertices)
