"""Tests of `ops/shift.py`: a picture or one channel moved by whole or fractional pixels, with a rule for the edge.

The oracle is a brute-force reference written here: pad the picture with numpy's own edge modes (`np.pad`), then read
each output pixel bilinearly at the point it came from.
"""

from typing import Literal

import numpy as np
import pytest

from bullterrier_image.ops.shift import BoxedImage, Edge

EDGES: list[Edge] = ["wrap", "nearest", "reflect", "mirror"]
# numpy's names for the same rules: its 'symmetric' repeats the edge pixel (our 'reflect'), its 'reflect' does not
NUMPY_PAD: dict[Edge, Literal["wrap", "edge", "symmetric", "reflect"]] = {
    "wrap": "wrap",
    "nearest": "edge",
    "reflect": "symmetric",
    "mirror": "reflect",
}


def noise(shape: tuple[int, ...], seed: int = 0) -> np.ndarray:
    return np.random.default_rng(seed).random(shape, dtype=np.float32)


def reference(image: np.ndarray, dy: float, dx: float, edge: Edge) -> np.ndarray:
    """Output (i, j) is the picture at (i - dy, j - dx), bilinear between the four pixels around that point, with the
    picture continued beyond its edge by numpy's padding."""
    margin = int(max(abs(dy), abs(dx))) + 3
    padding = [(margin, margin), (margin, margin)] + [(0, 0)] * (image.ndim - 2)
    padded = np.pad(image.astype(np.float64), padding, mode=NUMPY_PAD[edge])
    out = np.zeros(image.shape, np.float64)
    for i in range(image.shape[0]):
        for j in range(image.shape[1]):
            y, x = i - dy + margin, j - dx + margin
            y0, x0 = int(np.floor(y)), int(np.floor(x))
            wy, wx = y - y0, x - x0
            out[i, j] = (
                (1 - wy) * (1 - wx) * padded[y0, x0]
                + (1 - wy) * wx * padded[y0, x0 + 1]
                + wy * (1 - wx) * padded[y0 + 1, x0]
                + wy * wx * padded[y0 + 1, x0 + 1]
            )
    return out


def boxed(rows: slice, cols: slice, shape: tuple[int, int] = (40, 50)) -> np.ndarray:
    image = np.zeros(shape, np.float32)
    image[rows, cols] = noise((len(range(*rows.indices(shape[0]))), len(range(*cols.indices(shape[1])))), seed=3) + 0.1
    return image


@pytest.mark.parametrize("edge", EDGES)
@pytest.mark.parametrize(
    ("dy", "dx"),
    [(0, 0), (3, -4), (0, 7.5), (-2.25, 0), (5.5, -3.75), (13, 17), (-60.5, 123.25), (39.9, 0.1)],
    ids=lambda value: str(value),
)
@pytest.mark.parametrize(
    "image",
    [
        pytest.param(boxed(slice(10, 14), slice(20, 27)), id="a box in the middle"),
        pytest.param(boxed(slice(0, 5), slice(0, 9)), id="a box in the corner"),
        pytest.param(boxed(slice(0, 40), slice(0, 50)), id="a box that is the whole picture"),
        pytest.param(boxed(slice(7, 8), slice(8, 9)), id="one pixel"),
        pytest.param(np.zeros((40, 50), np.float32), id="nothing"),
    ],
)
def test_a_boxed_image_shifts_to_the_pixels_shift_gives_and_zero_elsewhere(image, dy, dx, edge: Edge) -> None:
    rows, cols, block = BoxedImage(image, edge=edge).shifted(dy, dx)
    placed = np.zeros(image.shape, np.float64)
    placed[rows, cols] = block
    np.testing.assert_allclose(placed, reference(image, dy, dx, edge), atol=2e-6)


@pytest.mark.parametrize("edge", EDGES)
def test_the_block_of_a_small_box_is_small_so_the_cost_follows_the_box(edge: Edge) -> None:
    rows, cols, block = BoxedImage(boxed(slice(10, 14), slice(20, 27)), edge=edge).shifted(5.5, -3.75)
    assert block.shape == (len(range(*rows.indices(40))), len(range(*cols.indices(50))))
    assert block.shape[0] <= 4 + 1
    assert block.shape[1] <= 7 + 1


def test_the_block_of_an_image_that_is_all_zero_is_empty() -> None:
    assert BoxedImage(np.zeros((8, 9), np.float32), edge="wrap").shifted(2.5, 1.0)[2].size == 0
