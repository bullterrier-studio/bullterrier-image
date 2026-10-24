"""Behaviour of the `mirror` effect: part of the picture kept, the rest replaced by its reflection."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "mirror"
SIZES = [(96, 128), (13, 29), (61, 37)]  # even by even, and two odd ones: the middle row or column is its own mirror

# direction -> which rows are kept (None: the rows are not mirrored), which columns are kept
DIRECTIONS = {
    "left_to_right": (None, "first"),
    "right_to_left": (None, "last"),
    "top_to_bottom": ("first", None),
    "bottom_to_top": ("last", None),
    "top_left": ("first", "first"),
    "top_right": ("first", "last"),
    "bottom_left": ("last", "first"),
    "bottom_right": ("last", "last"),
}


def row(*values: float) -> np.ndarray:
    """A grey picture one pixel high whose columns have `values`."""
    return np.broadcast_to(np.array(values, np.float32)[None, :, None], (1, len(values), 3)).copy()


def reflected(rgb: np.ndarray, rows: str | None, columns: str | None) -> np.ndarray:
    """Brute-force reference: every pixel looks up where it comes from. A pixel on the kept side comes from itself,
    any other from its mirror image; 'first' keeps the first `ceil(n / 2)` lines, 'last' the last `ceil(n / 2)`."""
    height, width = rgb.shape[:2]

    def source(index: int, size: int, kept: str | None) -> int:
        if kept is None:
            return index
        on_kept_side = index < (size + 1) // 2 if kept == "first" else index >= size // 2
        return index if on_kept_side else size - 1 - index

    out = np.empty_like(rgb)
    for y in range(height):
        for x in range(width):
            out[y, x] = rgb[source(y, height, rows), source(x, width, columns)]
    return out


@pytest.mark.parametrize("direction", DIRECTIONS)
@pytest.mark.parametrize("size", SIZES, ids=lambda size: f"{size[0]}x{size[1]}")
def test_the_result_is_symmetric_about_each_mirror_line(direction: str, size: tuple[int, int], photo) -> None:
    rows, columns = DIRECTIONS[direction]
    out = registry.apply(NAME, photo[: size[0], : size[1]], direction=direction)
    if columns is not None:
        np.testing.assert_array_equal(out, np.flip(out, axis=1))
    if rows is not None:
        np.testing.assert_array_equal(out, np.flip(out, axis=0))


@pytest.mark.parametrize("direction", DIRECTIONS)
@pytest.mark.parametrize("size", SIZES, ids=lambda size: f"{size[0]}x{size[1]}")
def test_the_kept_part_is_untouched_and_the_rest_is_its_reflection(
    direction: str, size: tuple[int, int], photo
) -> None:
    """An odd picture keeps its middle line whichever side is kept; a direction that names one line leaves the other
    axis exactly as it is."""
    rgb = photo[: size[0], : size[1]]
    out = registry.apply(NAME, rgb, direction=direction)
    np.testing.assert_array_equal(out, reflected(rgb, *DIRECTIONS[direction]))


@pytest.mark.parametrize(
    ("direction", "columns", "expected"),
    [
        ("left_to_right", (1, 2, 3, 4), (1, 2, 2, 1)),
        ("right_to_left", (1, 2, 3, 4), (4, 3, 3, 4)),
        ("left_to_right", (1, 2, 3), (1, 2, 1)),
        ("right_to_left", (1, 2, 3), (3, 2, 3)),
        ("left_to_right", (7,), (7,)),
    ],
    ids=["even, left half", "even, right half", "odd, left half", "odd, right half", "a single column"],
)
def test_a_row_keeps_its_left_or_right_half_and_reflects_it(direction: str, columns, expected) -> None:
    out = registry.apply(NAME, row(*columns) / 10, direction=direction)
    np.testing.assert_allclose(out, row(*expected) / 10, atol=1e-7)


def test_top_to_bottom_reflects_top_onto_bottom_and_leaves_each_row_as_it_is() -> None:
    rows = np.array([1, 2, 3, 4], np.float32)[:, None, None] / 10 + np.zeros((4, 3, 3), np.float32)
    rows += np.array([0.0, 0.01, 0.02], np.float32)[None, :, None]  # distinct columns, to see them stay in place
    out = registry.apply(NAME, rows, direction="top_to_bottom")
    np.testing.assert_array_equal(out, rows[[0, 1, 1, 0]])


@pytest.mark.parametrize(
    ("direction", "quarter"),
    [("top_left", 1), ("top_right", 2), ("bottom_left", 3), ("bottom_right", 4)],
)
def test_a_corner_direction_copies_that_quarter_into_the_other_three(direction: str, quarter: int) -> None:
    quarters = np.array([[1, 2], [3, 4]], np.float32)[:, :, None] / 10 + np.zeros((2, 2, 3), np.float32)
    out = registry.apply(NAME, quarters, direction=direction)
    np.testing.assert_allclose(out, np.full((2, 2, 3), quarter / 10), atol=1e-7)
