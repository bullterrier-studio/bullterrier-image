"""Behaviour of the `noise` effect: random values of a set standard deviation added to every pixel."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "noise"
AMOUNT = 0.03  # small enough that no value of a 0.5 grey is clipped, even for a Gaussian's tail


def added(**settings) -> np.ndarray:
    """The noise added to a mid-grey 128 x 128 picture: (h, w, 3)."""
    return (registry.apply(NAME, np.full((128, 128, 3), 0.5, np.float32), **settings) - 0.5).astype(np.float64)


@pytest.mark.parametrize("color", [True, False])
@pytest.mark.parametrize("distribution", ["gaussian", "uniform"])
def test_amount_zero_leaves_the_picture_unchanged(distribution: str, color: bool, photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, amount=0.0, distribution=distribution, color=color)
    np.testing.assert_array_equal(out, photo)
    assert not np.shares_memory(out, photo)


@pytest.mark.parametrize("color", [True, False])
@pytest.mark.parametrize("distribution", ["gaussian", "uniform"])
def test_amount_is_the_standard_deviation_of_the_added_noise_with_zero_mean(distribution: str, color: bool) -> None:
    noise = added(amount=AMOUNT, distribution=distribution, color=color, seed=1)
    assert noise.std() == pytest.approx(AMOUNT, rel=0.02)
    assert noise.mean() == pytest.approx(0.0, abs=AMOUNT * 0.02)


def test_uniform_noise_stays_within_its_bound_and_gaussian_noise_does_not() -> None:
    """A uniform distribution of standard deviation s spans +-sqrt(3) s, evenly; a Gaussian's tail goes further."""
    bound = AMOUNT * np.sqrt(3)
    uniform = added(amount=AMOUNT, distribution="uniform", seed=1)
    assert np.abs(uniform).max() <= bound
    assert np.abs(uniform).max() > 0.99 * bound  # and it fills the range
    assert np.mean(np.abs(uniform) < bound / 2) == pytest.approx(0.5, abs=0.01)  # evenly: half within half the bound
    assert np.abs(added(amount=AMOUNT, distribution="gaussian", seed=1)).max() > 2 * bound * 0.9


def test_mono_noise_is_the_same_in_every_channel_and_colour_noise_is_independent_per_channel() -> None:
    mono = added(amount=AMOUNT, color=False, seed=2)
    np.testing.assert_array_equal(mono[..., 0], mono[..., 1])
    np.testing.assert_array_equal(mono[..., 0], mono[..., 2])
    colour = added(amount=AMOUNT, color=True, seed=2)
    correlations = np.corrcoef(colour.reshape(-1, 3).T)
    assert np.abs(correlations[np.triu_indices(3, k=1)]).max() < 0.03


def test_pixels_are_independent_of_their_neighbours() -> None:
    noise = added(amount=AMOUNT, seed=3)[..., 0]
    assert abs(np.corrcoef(noise[:, :-1].ravel(), noise[:, 1:].ravel())[0, 1]) < 0.03


def test_the_same_seed_gives_the_same_noise_and_another_seed_other_noise(photo: np.ndarray) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo, seed=1), registry.apply(NAME, photo, seed=1))
    assert np.abs(registry.apply(NAME, photo, seed=1) - registry.apply(NAME, photo, seed=2)).mean() > 0.01


def test_the_result_is_clipped_to_the_picture_range() -> None:
    out = registry.apply(NAME, np.ones((32, 32, 3), np.float32), amount=0.5)
    assert out.max() == 1.0
    assert out.min() < 0.9  # darkened where the noise is negative, never above white
