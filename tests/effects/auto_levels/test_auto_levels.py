"""Behaviour of the `auto_levels` effect: the stretch to the full tonal range, against hand-worked pictures and a
brute-force count of the saturated pixels."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "auto_levels"
CHANNELS = (0, 1, 2)


@pytest.fixture
def distinct() -> np.ndarray:
    """A 40 x 50 picture of low-contrast values (0.2..0.7), no two of which are equal (in a channel or across them),
    so the number of values at each end is exactly what `clip` asks for, never more because of ties."""
    values = np.random.default_rng(3).permutation(6000).reshape(40, 50, 3)
    return (0.2 + values / 6000 * 0.5).astype(np.float32)


def test_each_channel_is_stretched_to_the_full_range_on_its_own() -> None:
    pair = np.array([[[0.2, 0.3, 0.4], [0.4, 0.5, 0.6]]], np.float32)
    out = registry.apply(NAME, pair, clip=0.0, mode="per_channel")
    np.testing.assert_allclose(out[0], [[0, 0, 0], [1, 1, 1]], atol=1e-6)


def test_linked_channels_share_one_stretch_so_colour_relationships_stay() -> None:
    pair = np.array([[[0.2, 0.3, 0.4], [0.4, 0.5, 0.6]]], np.float32)
    out = registry.apply(NAME, pair, clip=0.0, mode="linked")
    np.testing.assert_allclose(out[0], [[0, 0.25, 0.5], [0.5, 0.75, 1]], atol=1e-6)  # (v - 0.2) / (0.6 - 0.2)


@pytest.mark.parametrize("clip", [0.0, 0.001, 0.01, 0.05, 0.2])
def test_clip_sets_the_share_of_pixels_left_saturated_at_each_end(distinct: np.ndarray, clip: float) -> None:
    out = registry.apply(NAME, distinct, clip=clip, mode="per_channel")
    pixels = out.shape[0] * out.shape[1]
    for channel in CHANNELS:
        values = out[..., channel]
        # brute force: with no ties, `clip` of the pixels (rounded down) are cut, and the value that ends the cut
        # lands on the end itself, so between `clip * pixels` and one pixel more are saturated
        for saturated in (np.count_nonzero(values == 0.0), np.count_nonzero(values == 1.0)):
            assert clip * pixels <= saturated <= clip * pixels + 1, (channel, saturated)


@pytest.mark.parametrize("clip", [0.0, 0.01, 0.05])
def test_linked_clip_counts_the_values_of_all_channels_together(distinct: np.ndarray, clip: float) -> None:
    out = registry.apply(NAME, distinct, clip=clip, mode="linked")
    values = 3 * out.shape[0] * out.shape[1]
    for saturated in (np.count_nonzero(out == 0.0), np.count_nonzero(out == 1.0)):
        assert clip * values <= saturated <= clip * values + 1


@pytest.mark.parametrize("mode", ["linked", "per_channel"])
def test_the_darkest_and_brightest_values_reach_black_and_white_when_nothing_is_clipped(
    distinct: np.ndarray, mode: str
) -> None:
    out = registry.apply(NAME, distinct, clip=0.0, mode=mode)
    assert out.min() == 0.0
    assert out.max() == 1.0


@pytest.mark.parametrize("mode", ["linked", "per_channel"])
def test_a_second_stretch_changes_nothing(photo: np.ndarray, mode: str) -> None:
    once = registry.apply(NAME, photo, clip=0.0, mode=mode)
    np.testing.assert_allclose(registry.apply(NAME, once, clip=0.0, mode=mode), once, atol=1e-5)


@pytest.mark.parametrize("mode", ["linked", "per_channel"])
def test_a_brighter_input_is_never_darker_in_the_output(photo: np.ndarray, mode: str) -> None:
    out = registry.apply(NAME, photo, clip=0.02, mode=mode)
    for channel in CHANNELS:
        order = np.argsort(photo[..., channel].ravel(), kind="stable")
        assert np.all(np.diff(out[..., channel].ravel()[order]) >= -1e-6)


@pytest.mark.parametrize(
    "picture",
    [
        pytest.param(np.full((6, 7, 3), 0.4, np.float32), id="a flat grey picture has no range to stretch"),
        pytest.param(np.full((1, 1, 3), 0.9, np.float32), id="a single pixel"),
    ],
)
@pytest.mark.parametrize("mode", ["linked", "per_channel"])
def test_a_picture_with_no_tonal_range_is_left_alone(picture: np.ndarray, mode: str) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, picture, mode=mode), picture)


def test_a_flat_channel_stays_as_it_is_while_the_others_are_stretched() -> None:
    picture = np.array([[[0.2, 0.5, 0.1], [0.6, 0.5, 0.3]]], np.float32)
    out = registry.apply(NAME, picture, clip=0.0, mode="per_channel")
    np.testing.assert_allclose(out[0], [[0, 0.5, 0], [1, 0.5, 1]], atol=1e-6)


def test_a_picture_that_already_fills_the_range_is_unchanged_when_nothing_is_clipped() -> None:
    ramp = np.linspace(0, 1, 101, dtype=np.float32)
    picture = np.stack([ramp, ramp[::-1], np.roll(ramp, 30)], axis=-1)[None]
    np.testing.assert_allclose(registry.apply(NAME, picture, clip=0.0, mode="per_channel"), picture, atol=1e-6)
