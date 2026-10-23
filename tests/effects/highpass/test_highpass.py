"""Behaviour of the `highpass` effect: the picture minus its blur, centred on mid-grey."""

import numpy as np
import pytest
from scipy import ndimage

from photo_fx.core import registry

NAME = "highpass"
MID_GREY = 0.5


def kernel_1d(sigma: float, truncate: float = 4.0) -> np.ndarray:
    """The Gaussian weights at integer offsets from exp(-d^2 / 2 sigma^2), cut at truncate x sigma, summing to 1."""
    reach = int(truncate * sigma + 0.5)
    offsets = np.arange(-reach, reach + 1)
    weights = np.exp(-(offsets**2) / (2.0 * sigma**2))
    return weights / weights.sum()


def step_edge(low: float, high: float) -> np.ndarray:
    """A vertical step edge in the middle of the picture: `low` on the left, `high` on the right."""
    step = np.full((16, 64, 3), low, np.float32)
    step[:, 32:] = high
    return step


@pytest.mark.parametrize("value", [0.0, 0.2, 0.5, 0.9, 1.0])
def test_a_flat_picture_becomes_mid_grey(value: float) -> None:
    flat = np.full((20, 30, 3), value, np.float32)
    np.testing.assert_allclose(registry.apply(NAME, flat, radius=4.0), MID_GREY, atol=1e-6)


@pytest.mark.parametrize("radius", [1.5, 3.0])
def test_a_bump_leaves_its_difference_from_the_blur_around_mid_grey(radius: float) -> None:
    """A bump of height h on a grey: the blur at the bump is h w0^2 above the grey and one pixel beside it h w0 w1
    (w the 1-D Gaussian weights), so the picture minus its blur is h (1 - w0^2) and -h w0 w1, whatever the grey."""
    height = 0.2
    weights = kernel_1d(radius)
    w0, w1 = weights[len(weights) // 2], weights[len(weights) // 2 + 1]
    grey = np.full((41, 41, 3), 0.3, np.float32)
    grey[20, 20] += height
    out = registry.apply(NAME, grey, radius=radius)
    assert out[20, 20, 0] == pytest.approx(MID_GREY + height * (1 - w0 * w0), abs=2e-6)
    assert out[20, 21, 0] == pytest.approx(MID_GREY - height * w0 * w1, abs=2e-6)


def test_the_blur_added_back_gives_the_picture() -> None:
    """With nothing clipped, highpass + blur - mid-grey is the picture: here the blur is that of the same radius."""
    low = np.full((24, 24, 3), 0.45, np.float32)
    low[10:14, 10:14] = 0.55  # detail well inside 0.5 +- 0.5, so nothing is clipped
    detail = registry.apply(NAME, low, radius=2.0) - MID_GREY
    blur_by_gaussian = ndimage.gaussian_filter(
        low, (2.0, 2.0, 0.0), mode="reflect", truncate=4.0
    )  # scipy's, the oracle
    np.testing.assert_allclose(detail + blur_by_gaussian, low, atol=2e-6)


@pytest.mark.parametrize("low_high", [(0.3, 0.7), (0.1, 0.4)], ids=["a bold edge", "a faint one"])
def test_an_edge_is_odd_about_mid_grey(low_high: tuple[float, float]) -> None:
    """Mirroring a step edge swaps its sides, and the picture minus its blur then changes sign."""
    low, high = low_high
    rising = registry.apply(NAME, step_edge(low, high), radius=2.0)[8, :, 0]
    falling = registry.apply(NAME, step_edge(high, low), radius=2.0)[8, :, 0]
    np.testing.assert_allclose(rising - MID_GREY, MID_GREY - falling, atol=2e-6)
    assert rising[31] < MID_GREY < rising[32]  # the dark side of the edge is below mid-grey, the bright side above


def test_far_from_an_edge_the_picture_is_mid_grey() -> None:
    row = registry.apply(NAME, step_edge(0.2, 0.8), radius=2.0)[8, :, 0]
    np.testing.assert_allclose(row[:20], MID_GREY, atol=1e-6)  # 4 sigma = 8 px from the edge is 24 px away
    np.testing.assert_allclose(row[44:], MID_GREY, atol=1e-6)


def test_radius_sets_which_detail_is_kept() -> None:
    """A fine checkerboard survives a large radius, an area-wide ramp does not."""
    rows, cols = np.mgrid[0:48, 0:48]
    checker = np.repeat(((rows + cols) % 2).astype(np.float32)[..., None], 3, axis=2)
    ramp = np.repeat((cols / 47.0).astype(np.float32)[..., None], 3, axis=2)
    kept_checker = registry.apply(NAME, checker, radius=8.0)[20:28, 20:28, 0] - MID_GREY
    kept_ramp = registry.apply(NAME, ramp, radius=8.0)[20:28, 20:28, 0] - MID_GREY
    assert np.abs(kept_checker).min() > 0.4
    assert np.abs(kept_ramp).max() < 0.02


def test_mode_sets_what_the_blur_sees_beyond_the_edge() -> None:
    """In a picture that rises from dark to bright, 'wrap' puts the bright end beside the dark one, so the blur at the
    dark edge is brighter than the picture there: the detail dips below mid-grey. Reflecting does not."""
    ramp = np.tile(np.linspace(0.0, 1.0, 64, dtype=np.float32)[None, :, None], (8, 1, 3))
    reflected = registry.apply(NAME, ramp, radius=4.0, mode="reflect")[4, 0, 0]
    wrapped = registry.apply(NAME, ramp, radius=4.0, mode="wrap")[4, 0, 0]
    assert reflected == pytest.approx(MID_GREY, abs=0.1)
    assert wrapped < MID_GREY - 0.1


def test_truncate_sets_where_the_blur_is_cut_off() -> None:
    """Six px from a bump: sigma 2 reaches there with truncate 4 (8 px), not with truncate 2 (4 px)."""
    grey = np.full((41, 41, 3), 0.5, np.float32)
    grey[20, 20] = 1.0
    long_blur = registry.apply(NAME, grey, radius=2.0, truncate=4.0)[20, 26, 0]
    short_blur = registry.apply(NAME, grey, radius=2.0, truncate=2.0)[20, 26, 0]
    assert abs(long_blur - MID_GREY) > 1e-4
    assert abs(short_blur - MID_GREY) < 1e-6


@pytest.mark.parametrize(("ground", "dot", "limit"), [(0.0, 1.0, 1.0), (1.0, 0.0, 0.0)], ids=["white dot", "black dot"])
def test_detail_beyond_mid_grey_is_cut_off_at_black_and_white(ground: float, dot: float, limit: float) -> None:
    """A sigma-1 blur leaves a lone dot with about 16% of its own height, so its detail is about 0.84 from mid-grey."""
    dotted = np.full((21, 21, 3), ground, np.float32)
    dotted[10, 10] = dot
    out = registry.apply(NAME, dotted, radius=1.0)
    assert out[10, 10, 0] == limit
