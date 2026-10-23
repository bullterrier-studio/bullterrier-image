"""Behaviour of the `posterize` effect."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "posterize"


@pytest.mark.parametrize(
    ("levels", "value", "snapped"),
    [
        pytest.param(2, 0.49, 0.0, id="2 levels: just below the middle goes to black"),
        pytest.param(2, 0.51, 1.0, id="2 levels: just above the middle goes to full"),
        pytest.param(3, 0.2, 0.0, id="3 levels (0, 1/2, 1): 0.2 is nearest 0"),
        pytest.param(3, 0.3, 0.5, id="3 levels: 0.3 is nearest 1/2"),
        pytest.param(3, 0.7, 0.5, id="3 levels: 0.7 is nearest 1/2"),
        pytest.param(3, 0.8, 1.0, id="3 levels: 0.8 is nearest 1"),
        pytest.param(4, 0.4, 1 / 3, id="4 levels (0, 1/3, 2/3, 1): 0.4 is nearest 1/3"),
        pytest.param(4, 0.6, 2 / 3, id="4 levels: 0.6 is nearest 2/3"),
        pytest.param(5, 0.9, 1.0, id="5 levels (0, 1/4, 1/2, 3/4, 1): 0.9 is nearest 1"),
    ],
)
def test_every_value_snaps_to_the_nearest_of_the_evenly_spaced_levels(
    levels: int, value: float, snapped: float
) -> None:
    picture = np.full((1, 1, 3), value, np.float32)
    np.testing.assert_allclose(registry.apply(NAME, picture, levels=levels), snapped, atol=1e-6)


def test_channels_are_posterised_on_their_own() -> None:
    picture = np.array([[[0.1, 0.5, 0.9]]], np.float32)
    np.testing.assert_allclose(registry.apply(NAME, picture, levels=3)[0, 0], [0.0, 0.5, 1.0], atol=1e-6)


@pytest.mark.parametrize("levels", [2, 3, 5, 16, 100], ids=lambda n: f"{n} levels")
def test_a_channel_has_at_most_that_many_values_and_black_and_white_stay(photo: np.ndarray, levels: int) -> None:
    out = registry.apply(NAME, photo, levels=levels)
    for channel in range(3):
        assert len(np.unique(out[..., channel])) <= levels
    ends = np.array([[[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]]], np.float32)
    np.testing.assert_array_equal(registry.apply(NAME, ends, levels=levels), ends)


@pytest.mark.parametrize("levels", [2, 3, 7, 64], ids=lambda n: f"{n} levels")
def test_posterising_twice_changes_nothing_more(photo: np.ndarray, levels: int) -> None:
    once = registry.apply(NAME, photo, levels=levels)
    np.testing.assert_allclose(registry.apply(NAME, once, levels=levels), once, atol=1e-6)


@pytest.mark.parametrize("levels", [2, 4, 9], ids=lambda n: f"{n} levels")
def test_no_value_moves_by_more_than_half_a_step_and_brighter_never_becomes_darker(
    photo: np.ndarray, levels: int
) -> None:
    out = registry.apply(NAME, photo, levels=levels)
    assert np.abs(out - photo).max() <= 0.5 / (levels - 1) + 1e-6
    ramp = np.linspace(0, 1, 997, dtype=np.float32)[None, :, None] * np.ones((1, 1, 3), np.float32)
    assert (np.diff(registry.apply(NAME, ramp, levels=levels), axis=1) >= 0).all()


def test_256_levels_leave_an_8_bit_picture_as_it_is() -> None:
    eight_bit = (np.arange(256, dtype=np.float32) / 255)[None, :, None] * np.ones((1, 1, 3), np.float32)
    np.testing.assert_allclose(registry.apply(NAME, eight_bit, levels=256), eight_bit, atol=1e-6)
