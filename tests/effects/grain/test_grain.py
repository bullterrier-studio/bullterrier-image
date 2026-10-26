"""Behaviour of the `grain` effect: seeded noise whose strength follows the brightness, grey or per colour channel."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "grain"
# A weak grain, so that no value of a mid or dark patch is clipped (std 0.013 at mid-grey, 0.02 at black)
WEAK = {"amount": 0.013, "balance": -7 / 13}


def patch(colour: tuple[float, float, float], size: int = 128) -> np.ndarray:
    return np.broadcast_to(np.array(colour, np.float32), (size, size, 3)).copy()


def added(picture: np.ndarray, **settings) -> np.ndarray:
    """What the grain adds to `picture`: (h, w, 3), float64."""
    return (registry.apply(NAME, picture, **settings) - picture).astype(np.float64)


def neighbour_correlation(noise: np.ndarray) -> float:
    """Correlation of each pixel's noise with the one to its right (0 = independent pixels, 1 = smooth)."""
    return float(np.corrcoef(noise[:, :-1].ravel(), noise[:, 1:].ravel())[0, 1])


def test_the_same_seed_gives_the_same_grain_and_another_seed_other_grain(photo: np.ndarray) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo, seed=1), registry.apply(NAME, photo, seed=1))
    assert np.abs(registry.apply(NAME, photo, seed=1) - registry.apply(NAME, photo, seed=2)).mean() > 0.001


@pytest.mark.parametrize("balance", [-1.0, 0.0, 1.0])
@pytest.mark.parametrize("color", [True, False])
@pytest.mark.parametrize("distribution", ["gaussian", "uniform"])
def test_an_amount_of_zero_leaves_the_picture_unchanged_at_any_balance(
    distribution: str, balance: float, color: bool, photo: np.ndarray
) -> None:
    out = registry.apply(NAME, photo, amount=0.0, balance=balance, color=color, distribution=distribution)
    np.testing.assert_array_equal(out, photo)
    assert not np.shares_memory(out, photo)


@pytest.mark.parametrize("distribution", ["gaussian", "uniform"])
@pytest.mark.parametrize("color", [True, False])
@pytest.mark.parametrize(
    ("balance", "brightness", "expected_std"),
    [
        pytest.param(0.0, 0.5, 0.01, id="even grain: the amount at any brightness"),
        pytest.param(0.0, 0.2, 0.01, id="even grain in the shadows"),
        pytest.param(-0.5, 0.5, 0.01, id="mid-grey: the amount at any balance"),
        pytest.param(-1.0, 0.2, 0.016, id="only shadows: 1.6 x at a brightness of 0.2"),
        pytest.param(1.0, 0.8, 0.016, id="only highlights: 1.6 x at a brightness of 0.8"),
        pytest.param(1.0, 0.2, 0.004, id="only highlights: 0.4 x at a brightness of 0.2"),
        pytest.param(-0.5, 0.8, 0.007, id="shadows-heavy: 0.7 x at a brightness of 0.8"),
    ],
)
def test_the_grain_deviation_is_the_amount_at_mid_grey_and_follows_the_balance_across_brightness(
    balance: float, brightness: float, expected_std: float, color: bool, distribution: str
) -> None:
    """On a flat grey patch (a weak grain is never clipped at these levels). The values are worked out by hand from
    amount x (1 + balance x (2 x brightness - 1)) with an amount of 0.01."""
    grain = added(
        patch((brightness,) * 3), amount=0.01, balance=balance, color=color, distribution=distribution, seed=1
    )
    assert grain.std() == pytest.approx(expected_std, rel=0.03)
    assert grain.mean() == pytest.approx(0.0, abs=0.0005)  # brightens and darkens alike


def test_brightness_is_the_luminance_of_the_pixel_for_all_three_channels() -> None:
    """Rec. 709 luminance of (0.2, 0.4, 0.8) is 0.2126 x 0.2 + 0.7152 x 0.4 + 0.0722 x 0.8 = 0.38626 (the mean of the
    channels would be 0.467): every channel's grain has the deviation at that brightness, here 0.01 x (1 - (2 x
    0.38626 - 1)) for a balance of -1."""
    grain = added(patch((0.2, 0.4, 0.8)), amount=0.01, balance=-1.0, color=True, seed=1)
    expected = 0.01 * (1 - (2 * 0.38626 - 1))
    for channel in range(3):
        assert grain[..., channel].std() == pytest.approx(expected, rel=0.03)


def test_the_grain_of_a_picture_follows_its_own_brightness() -> None:
    """A dark left half (0.15) and a bright right half (0.85) with shadow grain only (balance -1, amount 0.01): the
    deviation is 0.02 x (1 - brightness) in each half."""
    half = np.concatenate([patch((0.15,) * 3, 96)[:, :48], patch((0.85,) * 3, 96)[:, :48]], axis=1)
    grain = added(half, amount=0.01, balance=-1.0, seed=3)
    assert grain[:, :48].std() == pytest.approx(0.02 * 0.85, rel=0.05)
    assert grain[:, 48:].std() == pytest.approx(0.02 * 0.15, rel=0.05)


def test_grey_grain_is_the_same_in_every_channel_and_colour_grain_is_independent_per_channel() -> None:
    grey = added(patch((0.5,) * 3), **WEAK, color=False, seed=2)
    np.testing.assert_array_equal(grey[..., 0], grey[..., 1])
    np.testing.assert_array_equal(grey[..., 0], grey[..., 2])
    colour = added(patch((0.5,) * 3), **WEAK, color=True, seed=2)
    correlations = np.corrcoef(colour.reshape(-1, 3).T)
    assert np.abs(correlations[np.triu_indices(3, k=1)]).max() < 0.03


def test_size_above_one_pixel_clumps_the_grain_and_one_pixel_does_not() -> None:
    def correlation(size: float) -> float:
        return neighbour_correlation(added(patch((0.5,) * 3), **WEAK, size=size, seed=1)[..., 0])

    assert abs(correlation(1.0)) < 0.03
    assert correlation(3.0) > 0.5
    assert correlation(6.0) > correlation(3.0)


@pytest.mark.parametrize("distribution", ["gaussian", "uniform"])
@pytest.mark.parametrize("size", [1.0, 4.0])
def test_clumping_keeps_the_standard_deviation(size: float, distribution: str) -> None:
    grain = added(patch((0.5,) * 3), amount=0.03, size=size, distribution=distribution, seed=5)
    assert grain.std() == pytest.approx(0.03, rel=0.02)


def test_edge_and_truncate_decide_what_the_clumping_blur_sees_beyond_the_border() -> None:
    """The blur (size 8: sigma 4) reaches truncate x 4 px in from the border: 4 px at 1, 16 px at 4."""
    base = {**WEAK, "size": 8.0, "seed": 2}
    flat = patch((0.5,) * 3, size=96)
    reflect_near, wrap_near = (added(flat, **base, truncate=1.0, edge=edge)[..., 0] for edge in ("reflect", "wrap"))
    reflect_far, wrap_far = (added(flat, **base, truncate=4.0, edge=edge)[..., 0] for edge in ("reflect", "wrap"))
    inner = (slice(8, -8), slice(8, -8))
    assert np.corrcoef(reflect_near[inner].ravel(), wrap_near[inner].ravel())[0, 1] > 1 - 1e-6  # out of reach
    assert np.corrcoef(reflect_far[inner].ravel(), wrap_far[inner].ravel())[0, 1] < 1 - 1e-6  # reached
    assert np.corrcoef(reflect_near[:, :2].ravel(), wrap_near[:, :2].ravel())[0, 1] < 0.99  # at the border


def test_uniform_grain_stays_within_its_bound_and_gaussian_grain_does_not() -> None:
    """A uniform distribution of standard deviation s spans +-sqrt(3) s, evenly; a Gaussian's tail goes further."""
    bound = 0.03 * np.sqrt(3)
    uniform = added(patch((0.5,) * 3), amount=0.03, distribution="uniform", seed=1)
    assert np.abs(uniform).max() <= bound * 1.0001  # float32 rounding of the picture
    assert np.abs(uniform).max() > 0.99 * bound  # and it fills the range
    assert np.mean(np.abs(uniform) < bound / 2) == pytest.approx(0.5, abs=0.01)  # evenly: half within half the bound
    assert np.abs(added(patch((0.5,) * 3), amount=0.03, distribution="gaussian", seed=1)).max() > 2 * bound * 0.9


def test_the_result_is_clipped_to_the_picture_range() -> None:
    out = registry.apply(NAME, patch((1.0,) * 3), amount=0.5)
    assert out.max() == 1.0
    assert out.min() < 0.9  # darkened where the noise is negative, never above white
