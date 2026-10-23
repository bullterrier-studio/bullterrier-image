"""Behaviour of `ops.guided`: the guided filter, against a naive per-window implementation of its definition."""

import numpy as np
import pytest

from photo_fx.ops.guided import guided_filter


def window_mean(values: np.ndarray, radius: int) -> np.ndarray:
    """Every window mean as a slice and a mean: the (2 x radius + 1) px square around each pixel, cut at the edge."""
    height, width = values.shape
    return np.array(
        [
            [
                values[max(y - radius, 0) : y + radius + 1, max(x - radius, 0) : x + radius + 1].mean()
                for x in range(width)
            ]
            for y in range(height)
        ]
    )


def reference(guide: np.ndarray, source: np.ndarray, radius: int, epsilon: float) -> np.ndarray:
    """He, Sun and Tang (2010), algorithm 1, for a grey guide, in float64."""
    guide, source = guide.astype(np.float64), source.astype(np.float64)
    mean_guide, mean_source = window_mean(guide, radius), window_mean(source, radius)
    covariance = window_mean(guide * source, radius) - mean_guide * mean_source
    variance = window_mean(guide * guide, radius) - mean_guide * mean_guide
    slope = covariance / (variance + epsilon)
    offset = mean_source - slope * mean_guide
    return window_mean(slope, radius) * guide + window_mean(offset, radius)


def pictures(seed: int, shape: tuple[int, int]) -> tuple[np.ndarray, np.ndarray]:
    rng = np.random.default_rng(seed)
    guide = rng.random(shape, dtype=np.float32)
    return guide, (0.5 * guide + 0.5 * rng.random(shape, dtype=np.float32)).astype(np.float32)


CASES = {
    "radius 0": ((9, 11), 0, 0.01),
    "radius 1": ((9, 11), 1, 0.01),
    "radius 3, a wide picture": ((6, 23), 3, 0.05),
    "radius 3, a tall picture": ((23, 6), 3, 0.05),
    "a window larger than the picture": ((7, 9), 40, 0.02),
    "a tiny epsilon": ((10, 10), 2, 1e-6),
    "a huge epsilon": ((10, 10), 2, 100.0),
    "one pixel": ((1, 1), 2, 0.01),
    "one row": ((1, 12), 2, 0.01),
}


@pytest.mark.parametrize(("shape", "radius", "epsilon"), CASES.values(), ids=list(CASES))
def test_the_result_is_the_guided_filter_by_its_definition(shape, radius, epsilon) -> None:
    guide, source = pictures(5, shape)
    out = guided_filter(guide, source, radius=radius, epsilon=epsilon)
    assert out.shape == shape
    assert out.dtype == np.float32
    np.testing.assert_allclose(out, reference(guide, source, radius, epsilon), atol=1e-5)


def test_radius_zero_leaves_the_source_as_it_is() -> None:
    guide, source = pictures(1, (8, 8))
    np.testing.assert_allclose(guided_filter(guide, source, radius=0, epsilon=0.01), source, atol=1e-6)


def test_a_constant_source_stays_constant_whatever_the_guide() -> None:
    guide, _ = pictures(2, (12, 15))
    out = guided_filter(guide, np.full((12, 15), 0.37, np.float32), radius=3, epsilon=0.05)
    np.testing.assert_allclose(out, 0.37, atol=1e-6)


def test_a_source_that_is_an_affine_function_of_the_guide_is_kept_when_epsilon_is_tiny() -> None:
    guide, _ = pictures(3, (16, 16))
    source = 2.0 * guide + 0.1
    np.testing.assert_allclose(guided_filter(guide, source, radius=4, epsilon=1e-8), source, atol=1e-4)


def test_a_step_in_the_guide_stays_sharp_when_epsilon_is_below_its_height_and_is_blurred_when_above() -> None:
    step = np.zeros((9, 40), np.float32)
    step[:, 20:] = 1.0
    sharp = guided_filter(step, step, radius=5, epsilon=0.001)
    blurred = guided_filter(step, step, radius=5, epsilon=10.0)
    assert sharp[4, 19] < 0.02
    assert sharp[4, 20] > 0.98
    assert 0.2 < blurred[4, 19] < blurred[4, 20] < 0.8


def test_the_inputs_are_left_alone_and_float64_is_accepted() -> None:
    guide, source = pictures(4, (8, 9))
    before = guide.copy(), source.copy()
    out = guided_filter(guide.astype(np.float64), source.astype(np.float64), radius=2, epsilon=0.01)
    np.testing.assert_array_equal(guide, before[0])
    np.testing.assert_array_equal(source, before[1])
    assert out.dtype == np.float32


@pytest.mark.parametrize("seed", range(6))
def test_a_picture_guided_by_itself_never_leaves_its_own_range(seed: int) -> None:
    """Self-guided, every output is an average of source values, so edges cannot make it overshoot."""
    rng = np.random.default_rng(seed)
    picture = np.where(rng.random((17, 23)) > 0.5, 0.9, 0.1).astype(np.float32) * (seed + 1) / 3 - seed / 5
    out = guided_filter(picture, picture, radius=seed % 4, epsilon=10.0 ** (-seed % 3 - 1))
    assert out.min() >= picture.min() - 1e-5
    assert out.max() <= picture.max() + 1e-5
