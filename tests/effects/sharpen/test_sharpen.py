"""Behaviour of the `sharpen` effect: an unsharp mask, picture + amount x (picture - blurred picture)."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "sharpen"


def kernel_1d(sigma: float, truncate: float = 4.0) -> np.ndarray:
    """The Gaussian weights at integer offsets from exp(-d^2 / 2 sigma^2), cut at truncate x sigma, summing to 1."""
    reach = int(truncate * sigma + 0.5)
    offsets = np.arange(-reach, reach + 1)
    weights = np.exp(-(offsets**2) / (2.0 * sigma**2))
    return weights / weights.sum()


def bump(height: float, base: float = 0.5, size: int = 41) -> np.ndarray:
    """A flat grey picture with one pixel `height` above it in the middle."""
    grey = np.full((size, size, 3), base, np.float32)
    grey[size // 2, size // 2] += height
    return grey


def edge(low: float = 0.3, high: float = 0.7) -> np.ndarray:
    """A vertical step edge: `low` on the left half, `high` on the right half."""
    step = np.full((24, 48, 3), low, np.float32)
    step[:, 24:] = high
    return step


@pytest.mark.parametrize("value", [0.0, 0.35, 1.0])
def test_a_flat_picture_is_untouched_by_any_amount(value: float) -> None:
    flat = np.full((20, 30, 3), value, np.float32)
    np.testing.assert_allclose(registry.apply(NAME, flat, amount=5.0, radius=3.0), value, atol=1e-6)


def test_an_edge_overshoots_on_both_sides_and_far_from_it_nothing_changes() -> None:
    out = registry.apply(NAME, edge(), amount=1.0, radius=2.0, threshold=0.0)
    row = out[12, :, 0]
    assert row[23] < 0.3 - 0.01  # the dark side dips below its own level
    assert row[24] > 0.7 + 0.01  # the bright side rises above its own level
    np.testing.assert_allclose(row[:8], 0.3, atol=1e-6)  # beyond 4 sigma the picture is as it was
    np.testing.assert_allclose(row[-8:], 0.7, atol=1e-6)


@pytest.mark.parametrize(
    ("radius", "amount"), [(1.5, 1.0), (2.0, 0.4), (3.0, 2.5)], ids=["sigma 1.5", "sigma 2, amount 0.4", "amount 2.5"]
)
def test_the_result_is_the_picture_plus_amount_times_its_difference_from_the_blur(radius: float, amount: float) -> None:
    """One bump of 0.1 on a 0.5 grey, worked out by hand: the blur at the bump is base + h w0^2, one pixel beside it
    base + h w0 w1, with w the 1-D Gaussian weights of sigma `radius`."""
    height, base = 0.1, 0.5
    weights = kernel_1d(radius)
    w0, w1 = weights[len(weights) // 2], weights[len(weights) // 2 + 1]
    out = registry.apply(NAME, bump(height, base), radius=radius, amount=amount, threshold=0.0)
    middle = 20
    assert out[middle, middle, 0] == pytest.approx(base + height + amount * height * (1 - w0 * w0), abs=2e-6)
    assert out[middle, middle + 1, 0] == pytest.approx(base - amount * height * w0 * w1, abs=2e-6)


@pytest.mark.parametrize(
    ("height", "threshold", "changes"),
    [(0.04, 0.05, False), (0.04, 0.02, True), (0.3, 0.05, True), (0.3, 0.5, False)],
    ids=["detail under the threshold", "detail over it", "bold detail", "bold detail, high threshold"],
)
def test_threshold_leaves_detail_below_it_alone(height: float, threshold: float, changes: bool) -> None:
    """A pixel changes only where it differs from the blur by at least `threshold` (0.04 x (1 - w0^2) is about 0.036
    at the bump, bold detail 0.3 x (1 - w0^2) about 0.27)."""
    grey = bump(height)
    out = registry.apply(NAME, grey, radius=2.0, amount=1.0, threshold=threshold)
    assert bool((out != grey).any()) is changes


def test_threshold_decides_pixel_by_pixel_not_for_the_picture() -> None:
    """A bold edge and a faint ripple in one picture: only the edge is sharpened."""
    both = edge(0.2, 0.8)
    both[:, 2::2, :] += 0.01  # a faint ripple on the left flat
    out = registry.apply(NAME, both, radius=1.5, amount=2.0, threshold=0.05)
    np.testing.assert_array_equal(out[:, :10], both[:, :10])  # the ripple is left alone
    assert out[12, 24, 0] > 0.8 + 0.05  # the edge overshoots


def test_radius_sets_how_far_from_an_edge_it_reaches() -> None:
    near = registry.apply(NAME, edge(), amount=1.0, radius=1.0, threshold=0.0)[12, :, 0]
    far = registry.apply(NAME, edge(), amount=1.0, radius=4.0, threshold=0.0)[12, :, 0]
    assert near[24 + 5] == pytest.approx(0.7, abs=1e-6)  # 5 px out is beyond 4 x 1 px
    assert far[24 + 5] != pytest.approx(0.7, abs=1e-3)


def test_it_sharpens_each_channel_on_its_own() -> None:
    red_edge = edge()
    red_edge[..., 1:] = 0.5
    out = registry.apply(NAME, red_edge, amount=1.0, radius=2.0, threshold=0.0)
    np.testing.assert_allclose(out[..., 1:], 0.5, atol=1e-6)
    assert out[12, 24, 0] > 0.71


SIGMA_1_5 = kernel_1d(1.5)  # 13 weights: the centre one is index 6
CENTRE = len(SIGMA_1_5) // 2


@pytest.mark.parametrize(
    ("edge", "edge_pixel_weight"),
    [
        ("reflect", SIGMA_1_5[CENTRE] + SIGMA_1_5[CENTRE + 1]),  # one offset beyond the edge shows the edge pixel
        ("nearest", SIGMA_1_5[CENTRE] + SIGMA_1_5[:CENTRE].sum()),  # every offset beyond the edge does
        ("wrap", SIGMA_1_5[CENTRE]),  # none does: the other edge is 20 px away
    ],
    ids=["reflect repeats the edge pixel once", "nearest repeats it throughout", "wrap never"],
)
def test_edge_sets_what_the_blur_sees_beyond_the_edge(edge: str, edge_pixel_weight: float) -> None:
    """A bump in the left-edge column of a grey picture: the blur there is the bump times the weight of the bump's own
    pixel along the row (`edge_pixel_weight`, worked out from the Gaussian weights) times the centre weight along the
    column."""
    height = 0.1
    grey = np.full((21, 21, 3), 0.5, np.float32)
    grey[10, 0] += height
    out = registry.apply(NAME, grey, radius=1.5, amount=1.0, threshold=0.0, edge=edge)
    blur_rise = height * SIGMA_1_5[CENTRE] * edge_pixel_weight
    assert out[10, 0, 0] == pytest.approx(0.5 + 2 * height - blur_rise, abs=2e-6)


def test_truncate_sets_where_the_blur_is_cut_off() -> None:
    """Six px from a bump, sigma 2 is 3 sigma out: the blur reaches it with truncate 4 (8 px), not with truncate 2."""
    beside = (20, 26)
    long_blur = registry.apply(NAME, bump(0.4), radius=2.0, truncate=4.0, amount=1.0)[beside][0]
    short_blur = registry.apply(NAME, bump(0.4), radius=2.0, truncate=2.0, amount=1.0)[beside][0]
    assert abs(long_blur - 0.5) > 1e-4
    assert abs(short_blur - 0.5) < 1e-6


@pytest.mark.parametrize("right", [0.0, 1.0], ids=["a mid-grey beside black", "a mid-grey beside white"])
def test_a_strong_sharpen_overshoots_and_is_cut_off_at_black_and_white(right: float) -> None:
    """Amount 5 pushes the pixels beside the edge about 0.75 beyond their level, past both limits."""
    side = np.full((24, 48, 3), 0.5, np.float32)
    side[:, 24:] = right
    out = registry.apply(NAME, side, amount=5.0, radius=2.0, threshold=0.0)
    assert out.min() == 0.0
    assert out.max() == 1.0
