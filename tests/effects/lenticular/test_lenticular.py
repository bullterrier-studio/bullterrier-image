"""Behaviour of the `lenticular` effect: vertical strips, every other one slid sideways."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "lenticular"
STRIP = 5


@pytest.fixture
def noise() -> np.ndarray:
    """41 rows and 61 columns of noise: 12 strips of 5 columns and a last one of a single column."""
    return np.random.default_rng(2).random((41, 61, 3), dtype=np.float32)


def strip_columns(width: int, parity: int) -> np.ndarray:
    """The columns of the strips whose number (counted from 0 at the left) has this parity, for strips of `STRIP`."""
    return np.flatnonzero((np.arange(width) // STRIP) % 2 == parity)


@pytest.mark.parametrize(
    ("shift", "edge", "pad_mode"),
    [
        (3, "wrap", "wrap"),
        (-4, "wrap", "wrap"),
        (3, "nearest", "edge"),
        (-4, "nearest", "edge"),
        (3, "reflect", "symmetric"),
        (-4, "mirror", "reflect"),
    ],
    ids=[
        "right, wrapping",
        "left, wrapping",
        "right, edge repeated",
        "left, edge repeated",
        "right, reflected",
        "left, mirrored",
    ],
)
def test_the_first_strip_and_every_second_one_after_it_keep_their_place_and_the_others_slide_by_shift(
    noise, shift, edge, pad_mode
) -> None:
    """The oracle slides the whole picture by hand: it pads the sides by 5 with numpy's rule for what lies beyond the
    edge and takes the columns `shift` to the left, which is the picture moved right by `shift`."""
    padded = np.pad(noise, ((0, 0), (5, 5), (0, 0)), mode=pad_mode)
    slid = padded[:, 5 - shift : 5 - shift + noise.shape[1]]
    out = registry.apply(NAME, noise, strip=STRIP, shift=shift, edge=edge)
    still, moved = strip_columns(61, 0), strip_columns(61, 1)
    assert np.array_equal(out[:, still], noise[:, still])
    assert np.array_equal(out[:, moved], slid[:, moved])


def test_a_shift_between_whole_pixels_blends_the_two_pixels_it_lies_between(noise) -> None:
    """Moving right by 2.25 puts three quarters of the picture moved by 2 and a quarter of it moved by 3."""
    out = registry.apply(NAME, noise, strip=STRIP, shift=2.25, edge="wrap")
    expected = 0.75 * np.roll(noise, 2, axis=1) + 0.25 * np.roll(noise, 3, axis=1)
    moved = strip_columns(61, 1)
    np.testing.assert_allclose(out[:, moved], expected[:, moved], atol=1e-6)


def test_shift_zero_leaves_the_picture_as_it_is(noise) -> None:
    assert np.array_equal(registry.apply(NAME, noise, strip=STRIP, shift=0), noise)


def test_a_strip_wider_than_the_picture_leaves_it_whole(noise) -> None:
    assert np.array_equal(registry.apply(NAME, noise, strip=200, shift=7), noise)
