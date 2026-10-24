"""Behaviour of the `shatter` effect: the picture broken into Voronoi pieces, each moved and turned a little, with dark
lines between them."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "shatter"
HEIGHT, WIDTH = 96, 128


def position_picture() -> np.ndarray:
    """Red is each pixel's row and green its column as fractions of the way to the last, so that a pixel of the output
    says where in the picture it was read from (bilinear reading of a gradient is exact between the edges)."""
    rows, cols = np.mgrid[0:HEIGHT, 0:WIDTH]
    return np.dstack([rows / (HEIGHT - 1), cols / (WIDTH - 1), np.zeros_like(rows)]).astype(np.float32)


def sources(margin: int, **settings: object) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """For each pixel the row and the column it reads, in px, and a mask of the pixels `margin` px or more from the
    edge of the picture (a pixel whose move reads beyond the edge is given by the edge rule, not by the move)."""
    out = registry.apply(NAME, position_picture(), gap=0, **settings)
    rows, cols = out[..., 0] * (HEIGHT - 1), out[..., 1] * (WIDTH - 1)
    rr, cc = np.mgrid[0:HEIGHT, 0:WIDTH]
    inside = (rr >= margin) & (rr < HEIGHT - margin) & (cc >= margin) & (cc < WIDTH - margin)
    return rows, cols, inside


@pytest.mark.parametrize("seed", [0, 1])
def test_without_offset_turn_or_gap_the_picture_is_unchanged(photo, seed) -> None:
    out = registry.apply(NAME, photo, offset=0, angle=0, gap=0, seed=seed)
    np.testing.assert_allclose(out, photo, atol=1e-6)


@pytest.mark.parametrize(("pieces", "offset"), [(6, 5.0), (30, 12.0)])
def test_with_offset_only_every_piece_is_the_picture_slid_by_one_move_of_its_own_within_offset(pieces, offset) -> None:
    """Without turning, a pixel reads the picture at its own place plus its piece's move, so the moves found over the
    picture take at most `pieces` values, each within `offset` px up and down and left and right, and are not all the
    same."""
    rows, cols, inside = sources(int(offset) + 1, pieces=pieces, offset=offset, angle=0, seed=5)
    rr, cc = np.mgrid[0:HEIGHT, 0:WIDTH]
    moves = np.stack([rows - rr, cols - cc], axis=-1)[inside]
    distinct = np.unique(np.round(moves, 2), axis=0)
    assert 1 < len(distinct) <= pieces
    assert np.abs(distinct).max() <= offset + 0.01
    assert (np.abs(distinct).max(axis=0) > offset / 2).all()  # the pieces travel both down and across
    assert (distinct.min(axis=0) < 0).all()  # and each way: up and down, left and right
    assert (distinct.max(axis=0) > 0).all()


@pytest.mark.parametrize("pieces", [8, 12])
def test_with_turning_only_every_piece_is_the_picture_turned_rigidly_by_at_most_angle_degrees(pieces) -> None:
    """Within one piece the source of a pixel one step right is one unit away in a direction turned by the piece's
    angle, and the source of the pixel one step down is a quarter turn from that, the same way round (a turn, no
    stretch, no mirror). Neighbours in two pieces differ, so most but not all pairs are rigid."""
    limit = 10.0
    rows, cols, inside = sources(25, pieces=pieces, offset=0, angle=limit, seed=4)
    right = np.stack([rows[:, 1:] - rows[:, :-1], cols[:, 1:] - cols[:, :-1]], axis=-1)[
        :-1
    ]  # (row, col) of a step right
    down = np.stack([rows[1:] - rows[:-1], cols[1:] - cols[:-1]], axis=-1)[:, :-1]  # of a step down
    valid = inside[:-1, :-1] & inside[1:, 1:] & inside[:-1, 1:] & inside[1:, :-1]
    unit_right = np.isclose(np.linalg.norm(right, axis=-1), 1, atol=1e-3)
    # the two steps make a right-handed square (determinant +1, columns first): a turn, not a mirror image
    square = np.isclose(right[..., 1] * down[..., 0] - right[..., 0] * down[..., 1], 1, atol=1e-3)
    rigid = unit_right & square & valid
    assert rigid.sum() > 0.8 * valid.sum()
    angle = np.degrees(np.arctan2(right[..., 0], right[..., 1]))[rigid]  # 0 = a step right reads one column right
    assert np.abs(angle).max() <= limit + 0.1
    assert np.abs(angle).max() > limit / 2  # and the pieces are turned
    assert len(np.unique(np.round(angle, 1))) > 1  # each by its own angle
    assert angle.min() < 0 < angle.max()  # some one way and some the other


def test_a_wider_gap_paints_a_proportionally_wider_band_in_the_gap_colour(photo) -> None:
    """The gap colour covers fully the pixels within half the width, less a half pixel, of a border: a band `gap - 1` px
    wide, so 5 px lines cover twice what 3 px lines do, and every covered pixel has exactly the gap colour."""
    magenta = np.array([1.0, 0.0, 1.0], np.float32)
    painted = {}
    for gap in (3, 5):
        out = registry.apply(NAME, photo, pieces=12, offset=0, angle=0, gap=gap, color="#ff00ff", seed=1)
        painted[gap] = (out == magenta).all(axis=-1).sum()
    assert painted[5] / painted[3] == pytest.approx(2.0, rel=0.15)
    assert painted[3] > 0


def test_the_default_gap_lines_only_darken_thin_lines_of_the_picture(photo) -> None:
    out = registry.apply(NAME, photo, pieces=12, offset=0, angle=0, gap=3, seed=1)
    changed = (out != photo).any(axis=-1)
    assert 0.02 < changed.mean() < 0.3  # lines, not the whole picture
    assert (out <= photo + 1e-6).all()  # the default colour of the lines is black


def moves_under_wrap(offset: float, seed: int) -> tuple[np.ndarray, np.ndarray]:
    """The whole-pixel move of every pixel, read from the position picture under 'wrap' and 'nearest' reading: a move is
    small, so it is the remainder of the position it names about the picture's size, nearest to 0."""
    out = registry.apply(
        NAME,
        position_picture(),
        pieces=8,
        offset=offset,
        angle=0,
        gap=0,
        seed=seed,
        edge="wrap",
        interpolation="nearest",
    )
    rr, cc = np.mgrid[0:HEIGHT, 0:WIDTH]
    down = np.rint(out[..., 0] * (HEIGHT - 1)).astype(int) - rr
    right = np.rint(out[..., 1] * (WIDTH - 1)).astype(int) - cc
    return (down + HEIGHT // 2) % HEIGHT - HEIGHT // 2, (right + WIDTH // 2) % WIDTH - WIDTH // 2


def test_wrap_reads_a_move_that_leaves_the_top_of_the_picture_from_its_bottom() -> None:
    """The check that `moves_under_wrap` is trustworthy: on the first rows a move up names a row near the last."""
    out = registry.apply(
        NAME, position_picture(), pieces=8, offset=12, angle=0, gap=0, seed=3, edge="wrap", interpolation="nearest"
    )
    read_rows = np.rint(out[:6, :, 0] * (HEIGHT - 1))
    assert (read_rows >= HEIGHT - 12).any()


@pytest.mark.parametrize(
    ("edge", "pad_mode"),
    [("nearest", "edge"), ("wrap", "wrap"), ("reflect", "symmetric"), ("mirror", "reflect")],
)
def test_a_move_that_leaves_the_picture_reads_beyond_its_edge_by_the_edge_rule(edge, pad_mode) -> None:
    """The oracle reads noise at each pixel's own place minus the piece's move (found as above, the same under every
    edge rule) from a copy padded by numpy's own rule for what lies beyond the edge."""
    offset, pad = 12, 14
    down, right = moves_under_wrap(offset, seed=3)
    noise = np.random.default_rng(5).random((HEIGHT, WIDTH, 3), dtype=np.float32)
    padded = np.pad(noise, ((pad, pad), (pad, pad), (0, 0)), mode=pad_mode)
    rr, cc = np.mgrid[0:HEIGHT, 0:WIDTH]
    expected = padded[rr + down + pad, cc + right + pad]
    out = registry.apply(
        NAME, noise, pieces=8, offset=offset, angle=0, gap=0, seed=3, edge=edge, interpolation="nearest"
    )
    assert np.array_equal(out, expected)
