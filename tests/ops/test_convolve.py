"""Tests of `ops/convolve.py`: FFT convolution with a 2-D kernel, against a direct sum written out in the test."""

from typing import Literal

import numpy as np
import pytest

from photo_fx.ops.convolve import Edge, convolve

EDGES: list[Edge] = ["reflect", "mirror", "nearest", "wrap"]
NUMPY_PAD: dict[Edge, Literal["symmetric", "reflect", "edge", "wrap"]] = {
    "reflect": "symmetric",
    "mirror": "reflect",
    "nearest": "edge",
    "wrap": "wrap",
}
# A single pixel, a single row or column, odd rectangles; kernels larger than most of them.
SHAPES = [(1, 1), (1, 7), (7, 1), (13, 29), (30, 21)]
KERNEL_SHAPES = [(1, 1), (3, 3), (5, 3), (3, 7), (9, 9)]


def random_picture(shape: tuple[int, ...], seed: int = 0) -> np.ndarray:
    return np.random.default_rng(seed).random(shape, dtype=np.float32)


def random_kernel(shape: tuple[int, int], seed: int = 1) -> np.ndarray:
    """Non-negative weights summing to 1 with no symmetry, so a flipped or transposed kernel gives another answer."""
    weights = np.random.default_rng(seed).random(shape)
    return weights / weights.sum()


def direct_sum(image: np.ndarray, kernel: np.ndarray, edge: Edge) -> np.ndarray:
    """The definition: out[y, x] = sum over offsets (i, j) of kernel[i + r, j + s] * image[y - i, x - j], with the
    picture extended beyond its edge by `edge`, one multiplication at a time."""
    reach_y, reach_x = kernel.shape[0] // 2, kernel.shape[1] // 2
    height, width = image.shape[:2]
    # extend by more than any offset needs, so a picture smaller than the kernel is still read at every offset
    pad = max(reach_y, reach_x) * (max(height, width) + 2)
    widths = [(pad, pad), (pad, pad)] + [(0, 0)] * (image.ndim - 2)
    padded = np.pad(image.astype(np.float64), widths, mode=NUMPY_PAD[edge])
    out = np.zeros(image.shape)
    for y in range(height):
        for x in range(width):
            for i in range(-reach_y, reach_y + 1):
                for j in range(-reach_x, reach_x + 1):
                    out[y, x] += kernel[i + reach_y, j + reach_x] * padded[pad + y - i, pad + x - j]
    return out


@pytest.mark.parametrize("edge", EDGES)
@pytest.mark.parametrize("kernel_shape", KERNEL_SHAPES, ids=lambda s: f"kernel {s[0]}x{s[1]}")
@pytest.mark.parametrize("shape", SHAPES, ids=lambda s: f"picture {s[0]}x{s[1]}")
def test_the_result_is_the_direct_sum_over_the_kernel(shape: tuple[int, int], kernel_shape, edge: Edge) -> None:
    picture = random_picture((*shape, 3))
    kernel = random_kernel(kernel_shape)
    np.testing.assert_allclose(convolve(picture, kernel, edge=edge), direct_sum(picture, kernel, edge), atol=2e-6)


def test_a_picture_without_channels_is_convolved_as_one_channel() -> None:
    picture = random_picture((17, 23))
    kernel = random_kernel((5, 3))
    out = convolve(picture, kernel, edge="reflect")
    assert out.shape == picture.shape
    np.testing.assert_allclose(out, direct_sum(picture, kernel, "reflect"), atol=2e-6)


@pytest.mark.parametrize(
    ("offset", "moved"),
    [((1, 0), (1, 0)), ((0, -2), (0, -2)), ((-1, 3), (-1, 3))],
    ids=["one row down", "two columns left", "one row up, three columns right"],
)
def test_a_weight_off_the_middle_moves_the_picture_that_way(offset: tuple[int, int], moved: tuple[int, int]) -> None:
    """A lone weight at (i, j) from the kernel's middle puts each pixel at (y + i, x + j): a convolution, not a
    correlation."""
    kernel = np.zeros((7, 9))
    kernel[3 + offset[0], 4 + offset[1]] = 1.0
    dot = np.zeros((21, 21, 3), np.float32)
    dot[10, 10] = 1.0
    out = convolve(dot, kernel, edge="reflect")
    expected = np.zeros_like(dot)
    expected[10 + moved[0], 10 + moved[1]] = 1.0
    np.testing.assert_allclose(out, expected, atol=1e-6)


@pytest.mark.parametrize("edge", EDGES)
def test_a_constant_picture_stays_as_it_is(edge: Edge) -> None:
    flat = np.full((16, 11, 3), 0.7, np.float32)
    np.testing.assert_allclose(convolve(flat, random_kernel((9, 5)), edge=edge), 0.7, atol=1e-6)


def test_the_mean_is_kept_when_the_picture_wraps_around() -> None:
    picture = random_picture((30, 40, 3))
    out = convolve(picture, random_kernel((7, 7)), edge="wrap")
    np.testing.assert_allclose(out.mean(axis=(0, 1)), picture.mean(axis=(0, 1)), atol=1e-5)


@pytest.mark.parametrize("kernel_shape", [(1, 1), (9, 9), (101, 101)])
def test_the_result_never_leaves_the_range_of_the_picture(kernel_shape: tuple[int, int]) -> None:
    picture = np.clip(random_picture((40, 50, 3)) * 1.4 - 0.2, 0.1, 0.9)  # dark and bright extremes, away from 0 and 1
    out = convolve(picture, random_kernel(kernel_shape), edge="mirror")
    assert out.min() >= picture.min()
    assert out.max() <= picture.max()


def test_the_input_and_the_kernel_are_left_alone_and_the_dtype_is_kept() -> None:
    picture = random_picture((12, 12, 3))
    kernel = random_kernel((5, 5))
    picture_before, kernel_before = picture.copy(), kernel.copy()
    out = convolve(picture, kernel, edge="nearest")
    assert out.dtype == np.float32
    assert out is not picture
    np.testing.assert_array_equal(picture, picture_before)
    np.testing.assert_array_equal(kernel, kernel_before)


@pytest.mark.parametrize(
    ("kernel", "complaint"),
    [
        pytest.param(np.full((4, 3), 1 / 12), "odd", id="an even number of rows"),
        pytest.param(np.full((3, 2), 1 / 6), "odd", id="an even number of columns"),
        pytest.param(np.full(5, 0.2), "2-D", id="a 1-D kernel"),
        pytest.param(np.full((3, 3), 0.2), "sum to 1", id="weights summing to more than 1"),
        pytest.param(np.full((3, 3), 0.05), "sum to 1", id="weights summing to less than 1"),
        pytest.param(np.full((3, 3), 1.001 / 9), "sum to 1", id="weights summing to 1.001"),
        pytest.param(np.array([[-1.0, 1.0, 1.0]]), "non-negative", id="a negative weight"),
    ],
)
def test_a_kernel_that_is_not_a_weighted_average_is_refused(kernel: np.ndarray, complaint: str) -> None:
    with pytest.raises(ValueError, match=complaint):
        convolve(random_picture((6, 6, 3)), kernel, edge="reflect")


def test_a_box_kernel_is_accepted() -> None:
    np.testing.assert_allclose(
        convolve(np.full((6, 6, 3), 0.3, np.float32), np.full((3, 3), 1 / 9), edge="reflect"), 0.3
    )
