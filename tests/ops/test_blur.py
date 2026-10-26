"""Tests of `ops/blur.py`: the Gaussian blurs the effects share, against scipy's direct convolution as the oracle."""

import time

import numpy as np
import pytest
from scipy import ndimage

from bullterrier_image.ops.blur import Edge, gaussian_blur, graded_blur

EDGES: list[Edge] = ["reflect", "mirror", "nearest", "wrap", "constant"]
TOLERANCE = 1e-5  # the bound `gaussian_blur` documents for pictures in 0..1
CONSTANT = 0.3  # beyond-the-edge value for edge 'constant': not 0, so a dropped value shows
# A single pixel, a single row or column, odd rectangles, and one larger than the biggest kernel radius below but one.
SIZES = [(1, 1), (1, 7), (7, 1), (13, 29), (61, 37), (150, 220)]
# Kernel radii (truncate 4) of 6, 28, 32, 80 and 360 px: small and large kernels, longer than most of the pictures.
SIGMAS = [1.5, 7.0, 8.0, 20.0, 90.0]


def picture(shape: tuple[int, ...], seed: int = 0) -> np.ndarray:
    """Gradients plus noise in 0..1, float32: edges that differ from each other, so every edge rule matters."""
    rng = np.random.default_rng(seed)
    rows, cols = np.mgrid[0 : shape[0], 0 : shape[1]].astype(np.float32)
    ramp = (rows / max(shape[0], 1) * 0.5 + cols / max(shape[1], 1) * 0.3).reshape(shape[:2] + (1,) * (len(shape) - 2))
    return (ramp + rng.random(shape, dtype=np.float32) * 0.2).astype(np.float32)


def scipy_blur(image: np.ndarray, sigma: float, edge: Edge, truncate: float = 4.0) -> np.ndarray:
    """The oracle: scipy's direct convolution over the two picture axes only, never across channels."""
    sigmas = (sigma, sigma) + (0.0,) * (image.ndim - 2)
    return ndimage.gaussian_filter(image, sigmas, mode=edge, cval=CONSTANT, truncate=truncate)


def blur(image: np.ndarray, sigma: float, edge: Edge, truncate: float = 4.0) -> np.ndarray:
    return gaussian_blur(image, sigma, truncate=truncate, edge=edge, constant_value=CONSTANT)


@pytest.mark.parametrize("sigma", SIGMAS)
@pytest.mark.parametrize("edge", EDGES)
@pytest.mark.parametrize("size", SIZES, ids=[f"{h}x{w}" for h, w in SIZES])
def test_the_blur_matches_a_direct_convolution_within_the_documented_bound(size, edge, sigma):
    image = picture(size)
    got = blur(image, sigma, edge)
    assert got.shape == image.shape
    assert np.abs(got.astype(np.float64) - scipy_blur(image, sigma, edge)).max() <= TOLERANCE


@pytest.mark.parametrize("edge", EDGES)
@pytest.mark.parametrize("sigma", [3.0, 40.0])
def test_each_channel_of_a_colour_picture_is_blurred_on_its_own(edge, sigma):
    image = picture((45, 70, 3), seed=1)
    image[..., 1] = 1.0 - image[..., 1]  # channels unlike each other: blurring across them would show
    got = blur(image, sigma, edge)
    assert np.abs(got.astype(np.float64) - scipy_blur(image, sigma, edge)).max() <= TOLERANCE


@pytest.mark.parametrize("truncate", [1.3, 2.5, 10.0])
def test_truncate_cuts_the_kernel_where_scipy_does(truncate):
    """Kernel radii of 32.5, 62.5 and 250 px: two halves, where rounding the radius up or down shows."""
    image = picture((90, 120, 3), seed=2)
    got = blur(image, 25.0, "reflect", truncate)
    assert np.abs(got.astype(np.float64) - scipy_blur(image, 25.0, "reflect", truncate)).max() <= TOLERANCE


@pytest.mark.parametrize(("sigma", "truncate"), [(0.0, 4.0), (1.5, 4.0), (7.0, 4.0), (2.0, 10.0), (30.0, 1.0)])
@pytest.mark.parametrize("edge", EDGES)
def test_a_kernel_radius_below_32_px_gives_exactly_scipys_result(sigma, truncate, edge):
    image = picture((61, 83, 3), seed=3)
    np.testing.assert_array_equal(blur(image, sigma, edge, truncate), scipy_blur(image, sigma, edge, truncate))


@pytest.mark.parametrize("sigma", [2.0, 25.0, 120.0])
@pytest.mark.parametrize("edge", EDGES)
def test_the_blur_never_leaves_the_range_of_the_picture_and_what_lies_beyond_it(edge, sigma):
    image = picture((70, 90), seed=4) * 0.5 + 0.4  # 0.4..0.9: the constant 0.3 lies below it
    got = blur(image, sigma, edge)
    low = min(float(image.min()), CONSTANT) if edge == "constant" else float(image.min())
    assert got.min() >= low
    assert got.max() <= image.max()


@pytest.mark.parametrize("sigma", [2.0, 25.0, 120.0])
@pytest.mark.parametrize("edge", EDGES)
def test_an_even_picture_stays_exactly_as_it_is(edge, sigma):
    """Whatever the blur averages is the same value (beyond the edge too: the constant is that value), so the result
    is that value exactly, not merely close to it."""
    even = np.full((50, 70, 3), CONSTANT, dtype=np.float32)
    np.testing.assert_array_equal(blur(even, sigma, edge), even)


@pytest.mark.parametrize("sigma", [2.0, 50.0])
def test_the_blur_keeps_float32_and_leaves_its_input_alone(sigma):
    image = picture((40, 60, 3), seed=5)
    before = image.copy()
    got = gaussian_blur(image, sigma, truncate=4.0, edge="reflect")
    assert got.dtype == np.float32
    np.testing.assert_array_equal(image, before)


def test_edge_constant_needs_the_value_beyond_the_edge():
    with pytest.raises(ValueError, match="constant_value"):
        gaussian_blur(picture((8, 8)), 40.0, truncate=4.0, edge="constant")


def cpu_seconds(image: np.ndarray, sigma: float) -> float:
    """Process CPU time per blur, the least of three measurements, each over as many blurs as take 50 ms or more.

    The span is several ticks of the coarsest process clock (15.6 ms on Windows, where one blur can measure 0), and the
    least measurement is the one least disturbed by the rest of the machine."""
    per_blur = []
    for _ in range(3):
        blurs, start = 0, time.process_time()
        while (spent := time.process_time() - start) < 0.05:
            gaussian_blur(image, sigma, truncate=4.0, edge="reflect")
            blurs += 1
        per_blur.append(spent / blurs)
    return min(per_blur)


def test_a_large_blur_costs_a_few_times_a_small_one_not_a_hundred():
    """A direct convolution costs in proportion to the kernel: a 1201-tap one about 70 times a 17-tap one on the same
    picture. The blur promises a cost that hardly grows with sigma; the ratio, unlike a time, is the same on any
    machine."""
    image = picture((1000, 1000), seed=6)
    small, large = cpu_seconds(image, 2.0), cpu_seconds(image, 150.0)
    assert large < 15 * small


COLOUR = (40, 56, 3)  # a colour picture larger than every kernel below at radius 6, so each edge rule shows


@pytest.mark.parametrize(
    ("weight", "radius"),
    [
        pytest.param(0.0, 6.0, id="weight 0 everywhere: the sharp picture"),
        pytest.param(1.0, 0.0, id="radius 0: no blur at any weight"),
        pytest.param(0.4, 0.0, id="radius 0 between levels: no blur"),
    ],
)
def test_the_graded_blur_returns_an_unblurred_picture_as_a_copy(weight: float, radius: float) -> None:
    image = picture(COLOUR, seed=7)
    got = graded_blur(image, np.full(COLOUR[:2], weight, np.float32), radius, levels=4, truncate=4.0, edge="reflect")
    np.testing.assert_array_equal(got, image)
    assert got is not image


@pytest.mark.parametrize("edge", ["reflect", "mirror", "nearest", "wrap"])
@pytest.mark.parametrize("levels", [1, 4, 16])
def test_weight_1_everywhere_is_the_gaussian_at_the_full_radius(levels: int, edge) -> None:
    image = picture(COLOUR, seed=8)
    got = graded_blur(image, np.ones(COLOUR[:2], np.float32), 6.0, levels=levels, truncate=4.0, edge=edge)
    assert np.abs(got.astype(np.float64) - scipy_blur(image, 6.0, edge)).max() <= TOLERANCE


@pytest.mark.parametrize(
    ("levels", "weight", "mix"),
    [
        pytest.param(4, 0.375, [(2.0, 0.5), (4.0, 0.5)], id="halfway between levels 1 and 2 of 4: half of each"),
        pytest.param(4, 0.125, [(0.0, 0.5), (2.0, 0.5)], id="halfway to level 1: half sharp, half sigma radius / 4"),
        pytest.param(2, 0.75, [(4.0, 0.5), (8.0, 0.5)], id="halfway between levels 1 and 2 of 2"),
        pytest.param(1, 0.25, [(0.0, 0.75), (8.0, 0.25)], id="1 level: a quarter of the way to the full blur"),
        pytest.param(4, 0.5, [(4.0, 1.0)], id="on level 2 of 4: that copy alone"),
    ],
)
def test_a_weight_between_two_levels_mixes_the_two_copies_either_side(
    levels: int, weight: float, mix: list[tuple[float, float]]
) -> None:
    """Radius 8: copy k of `levels` has sigma 8 x k / levels, the sharp picture being copy 0; a weight w puts a pixel at
    w x levels on that stack, a share of each copy either side by how near it lies. The left half of the picture has
    weight 0 and stays sharp, so every pixel follows its own weight."""
    image = picture(COLOUR, seed=9)
    weights = np.zeros(COLOUR[:2], np.float32)
    weights[:, COLOUR[1] // 2 :] = weight
    got = graded_blur(image, weights, 8.0, levels=levels, truncate=4.0, edge="reflect")
    copies = [share * (image if sigma == 0 else blur(image, sigma, "reflect")) for sigma, share in mix]
    expected = np.where(weights[..., None] > 0, np.sum(copies, axis=0), image)
    np.testing.assert_allclose(got, expected, atol=1e-6)


@pytest.mark.parametrize("levels", [1, 3, 16])
def test_the_graded_blur_never_leaves_the_range_of_the_picture(levels: int) -> None:
    image = picture(COLOUR, seed=10) * 0.5 + 0.3  # 0.3..0.8: away from 0 and 1, so a wider range would show
    weights = np.random.default_rng(11).random(COLOUR[:2], dtype=np.float32)
    got = graded_blur(image, weights, 40.0, levels=levels, truncate=4.0, edge="reflect")
    assert got.min() >= image.min()
    assert got.max() <= image.max()
