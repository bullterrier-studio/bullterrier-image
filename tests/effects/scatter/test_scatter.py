"""Behaviour of the `scatter` effect: every pixel replaced by one picked at random from within `radius` px of it."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "scatter"
HEIGHT, WIDTH = 40, 60


def position_picture() -> np.ndarray:
    """Red is the row and green the column of each pixel (as whole numbers over 255 and 255 again), so a pixel that was
    read from somewhere else says where."""
    rows, cols = np.mgrid[0:HEIGHT, 0:WIDTH]
    return np.dstack([rows / 255, cols / 255, np.zeros_like(rows)]).astype(np.float32)


def moves(out: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """How far down and right each pixel of `out` read its colour from, in pixels, for a `position_picture` input."""
    rows, cols = np.mgrid[0:HEIGHT, 0:WIDTH]
    return np.rint(out[..., 0] * 255).astype(int) - rows, np.rint(out[..., 1] * 255).astype(int) - cols


@pytest.mark.parametrize("seed", [0, 1, 2])
@pytest.mark.parametrize("radius", [1, 3, 9])
def test_every_pixel_reads_from_within_the_radius_and_the_moves_fill_the_whole_square_evenly(radius, seed) -> None:
    down, right = moves(registry.apply(NAME, position_picture(), radius=radius, seed=seed, edge="wrap"))
    interior = (slice(radius, HEIGHT - radius), slice(radius, WIDTH - radius))  # where no edge rule is involved
    down, right = down[interior], right[interior]
    assert np.abs(down).max() == radius
    assert np.abs(right).max() == radius
    assert abs(down.mean()) < radius / 4  # no side is favoured: the mean of a symmetric spread is near 0
    assert abs(right.mean()) < radius / 4
    assert len(set(zip(down.ravel().tolist(), right.ravel().tolist(), strict=True))) > (2 * radius + 1) ** 2 // 2


def moves_under_wrap(radius: int, seed: int) -> tuple[np.ndarray, np.ndarray]:
    """The moves of every pixel, read from the position picture under 'wrap': a move is small, so it is the remainder of
    the position it names about the picture's size, nearest to 0."""
    down, right = moves(registry.apply(NAME, position_picture(), radius=radius, seed=seed, edge="wrap"))
    return (down + HEIGHT // 2) % HEIGHT - HEIGHT // 2, (right + WIDTH // 2) % WIDTH - WIDTH // 2


def test_wrap_reads_a_move_that_leaves_the_top_of_the_picture_from_its_bottom() -> None:
    """The check that `moves_under_wrap` is trustworthy: on the first row a move up names a row near the last."""
    first_row = registry.apply(NAME, position_picture(), radius=4, seed=7, edge="wrap")[0]
    read_rows = np.rint(first_row[:, 0] * 255)
    assert (read_rows >= HEIGHT - 4).any()
    assert (read_rows <= 4).any()


@pytest.mark.parametrize(
    ("edge", "pad_mode"),
    [("nearest", "edge"), ("wrap", "wrap"), ("reflect", "symmetric"), ("mirror", "reflect")],
)
def test_every_output_pixel_is_the_input_pixel_its_move_names_by_the_edge_rule(edge, pad_mode) -> None:
    """The oracle reads the noise at each pixel's move (the same under every edge rule: only what lies beyond the
    picture differs) from a copy padded by numpy's own rule for what lies beyond the edge."""
    radius = 4
    down, right = moves_under_wrap(radius, seed=7)
    noise = np.random.default_rng(5).random((HEIGHT, WIDTH, 3), dtype=np.float32)
    padded = np.pad(noise, ((radius, radius), (radius, radius), (0, 0)), mode=pad_mode)
    rows, cols = np.mgrid[0:HEIGHT, 0:WIDTH]
    expected = padded[rows + down + radius, cols + right + radius]
    assert np.array_equal(registry.apply(NAME, noise, radius=radius, edge=edge, seed=7), expected)


def test_radius_zero_leaves_the_picture_as_it_is(photo) -> None:
    assert np.array_equal(registry.apply(NAME, photo, radius=0), photo)
