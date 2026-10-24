"""Behaviour of the `ntsc` effect: colour smeared along the scan lines, with the dot crawl of a composite signal."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "ntsc"
# The FCC YIQ matrix as published (47 CFR 73.682), typed here as the oracle's own copy.
YIQ = np.array([[0.299, 0.587, 0.114], [0.5959, -0.2746, -0.3213], [0.2115, -0.5227, 0.3112]])


def yiq(rgb: np.ndarray) -> np.ndarray:
    return rgb @ YIQ.T


def gaussian_kernel(sigma: float, truncate: float) -> np.ndarray:
    """The normalised Gaussian of standard deviation `sigma`, cut off at `truncate` sigmas (radius int(t * s + 0.5))."""
    radius = int(truncate * sigma + 0.5)
    offsets = np.arange(-radius, radius + 1)
    kernel = np.exp(-0.5 * (offsets / sigma) ** 2)
    return kernel / kernel.sum()


def smeared_along_rows(values: np.ndarray, sigma: float, truncate: float) -> np.ndarray:
    """Brute force: each row of `values` (height, width) convolved with the Gaussian, its edge pixel repeated."""
    kernel = gaussian_kernel(sigma, truncate)
    radius = len(kernel) // 2
    padded = np.pad(values, ((0, 0), (radius, radius)), mode="symmetric")
    return np.array(
        [[padded[r, c : c + len(kernel)] @ kernel for c in range(values.shape[1])] for r in range(len(values))]
    )


@pytest.fixture
def two_colours() -> np.ndarray:
    """16 x 24: the left half a saturated orange, the right half a saturated teal (a vertical colour edge)."""
    picture = np.empty((16, 24, 3), np.float32)
    picture[:, :12], picture[:, 12:] = (0.8, 0.45, 0.2), (0.2, 0.5, 0.6)
    return picture


@pytest.mark.parametrize("settings", [{"crawl": 0.0}, {"crawl": 1.0, "phase": 0.3}, {"sigma": 20.0, "crawl": 0.7}])
def test_the_luma_of_a_grey_picture_is_unchanged_because_it_has_no_colour_to_smear_or_leak(settings) -> None:
    levels = np.random.default_rng(2).random((20, 30, 1), dtype=np.float32)
    grey = np.repeat(levels, 3, axis=-1)
    np.testing.assert_allclose(registry.apply(NAME, grey, **settings), grey, atol=1e-5)


@pytest.mark.parametrize(("sigma", "truncate"), [(2.0, 4.0), (3.5, 2.0)])
def test_colour_is_smeared_along_each_row_by_a_gaussian_and_brightness_is_not(two_colours, sigma, truncate) -> None:
    out = yiq(registry.apply(NAME, two_colours, sigma=sigma, truncate=truncate, edge="reflect", crawl=0.0))
    before = yiq(two_colours)
    np.testing.assert_allclose(out[..., 0], before[..., 0], atol=1e-5)  # brightness exactly as it was
    for channel in (1, 2):
        np.testing.assert_allclose(
            out[..., channel], smeared_along_rows(before[..., channel], sigma, truncate), atol=1e-5
        )


def test_a_vertical_colour_edge_is_smeared_but_a_horizontal_one_is_not(two_colours) -> None:
    horizontal_edge = np.swapaxes(two_colours, 0, 1)  # the same two colours, now stacked: the edge runs along the rows
    across = yiq(registry.apply(NAME, two_colours, sigma=3.0, crawl=0.0))[..., 1:]
    along = yiq(registry.apply(NAME, horizontal_edge, sigma=3.0, crawl=0.0))[..., 1:]
    sharp = yiq(horizontal_edge)[..., 1:]
    assert np.abs(np.diff(across[0], axis=0)).max() < 0.5 * np.abs(np.diff(yiq(two_colours)[0, :, 1:], axis=0)).max()
    np.testing.assert_allclose(along, sharp, atol=1e-5)


@pytest.mark.parametrize("phase", [0.0, 0.125, 0.6])
def test_the_dot_crawl_leaks_the_colour_signal_into_brightness_on_the_ntsc_subcarrier_pattern(phase) -> None:
    """A flat colour picture, so smearing changes nothing and only the crawl is left. At 4 samples per subcarrier
    cycle, with the phase flipping every line (227.5 cycles per line), the brightness changes by crawl * (I cos t +
    Q sin t) with t = 2 pi (x / 4 + y / 2 + phase)."""
    colour = np.array([0.6, 0.4, 0.3], np.float32)
    flat = np.broadcast_to(colour, (8, 16, 3)).copy()
    i, q = yiq(colour)[1:]
    y, x = np.mgrid[0:8, 0:16]
    angle = 2 * np.pi * (x / 4 + y / 2 + phase)
    out = yiq(registry.apply(NAME, flat, sigma=2.0, crawl=0.5, phase=phase))
    np.testing.assert_allclose(out[..., 0], yiq(colour)[0] + 0.5 * (i * np.cos(angle) + q * np.sin(angle)), atol=1e-5)
    np.testing.assert_allclose(out[..., 1], i, atol=1e-5)  # the colour itself is where it was
    np.testing.assert_allclose(out[..., 2], q, atol=1e-5)


def test_a_whole_cycle_of_phase_is_no_change_and_half_a_cycle_flips_the_crawl(two_colours) -> None:
    base = registry.apply(NAME, two_colours, crawl=0.3, phase=0.0)
    np.testing.assert_allclose(registry.apply(NAME, two_colours, crawl=0.3, phase=1.0), base, atol=1e-5)
    flipped = yiq(registry.apply(NAME, two_colours, crawl=0.3, phase=0.5))[..., 0]
    smeared = yiq(registry.apply(NAME, two_colours, crawl=0.0))[..., 0]
    assert np.abs(flipped - smeared).max() > 0.01  # there is a crawl to flip
    np.testing.assert_allclose(flipped - smeared, -(yiq(base)[..., 0] - smeared), atol=1e-5)
