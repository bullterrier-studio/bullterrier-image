"""Behaviour of the `vibrance` effect."""

import colorsys

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "vibrance"


def chroma(picture: np.ndarray) -> np.ndarray:
    """Strongest minus weakest channel of every pixel: the HSV chroma."""
    return picture.max(axis=-1) - picture.min(axis=-1)


@pytest.mark.parametrize(
    ("pixel", "amount", "expected"),
    [
        # Saturation S = (max - min) / max rises to S + amount * S * (1 - S) with the strongest channel kept.
        pytest.param((0.6, 0.5, 0.4), 0.5, (0.6, 0.46667, 0.33333), id="muted: S = 1/3 rises to 0.4444"),
        pytest.param((0.6, 0.5, 0.4), 1.0, (0.6, 0.43333, 0.26667), id="muted at the maximum: S rises to 0.5556"),
        pytest.param((0.6, 0.5, 0.4), 0.0, (0.6, 0.5, 0.4), id="amount 0 changes nothing"),
        pytest.param((1.0, 0.0, 0.0), 1.0, (1.0, 0.0, 0.0), id="a fully saturated colour has no room to rise"),
        pytest.param((0.5, 0.5, 0.5), 1.0, (0.5, 0.5, 0.5), id="a grey stays grey"),
        pytest.param((0.0, 0.0, 0.0), 1.0, (0.0, 0.0, 0.0), id="black stays black"),
    ],
)
def test_muted_colours_gain_saturation_by_the_documented_amount(
    pixel: tuple[float, float, float], amount: float, expected: tuple[float, float, float]
) -> None:
    out = registry.apply(NAME, np.array([[pixel]], np.float32), amount=amount)
    np.testing.assert_allclose(out[0, 0], expected, atol=1e-5)


def test_the_result_agrees_with_the_standard_library_hsv_conversion(photo: np.ndarray) -> None:
    amount = 0.7
    small = photo[:5, :6]
    expected = np.zeros_like(small)
    for y in range(small.shape[0]):
        for x in range(small.shape[1]):
            h, s, v = colorsys.rgb_to_hsv(*(float(c) for c in small[y, x]))
            expected[y, x] = colorsys.hsv_to_rgb(h, s + amount * s * (1 - s), v)
    np.testing.assert_allclose(registry.apply(NAME, small, amount=amount), expected, atol=1e-5)


@pytest.mark.parametrize("amount", [0.1, 0.5, 1.0])
def test_chroma_never_falls_and_hue_and_value_stay_where_they_were(amount: float, photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, amount=amount)
    assert (chroma(out) >= chroma(photo) - 1e-6).all()
    np.testing.assert_allclose(out.max(axis=-1), photo.max(axis=-1), atol=1e-6)
    coloured = chroma(photo) > 0.05  # hue is ill-defined and float-sensitive for near-greys
    for before, after in zip(photo[coloured][:300], out[coloured][:300], strict=True):
        hue_before = colorsys.rgb_to_hsv(*(float(c) for c in before))[0]
        hue_after = colorsys.rgb_to_hsv(*(float(c) for c in after))[0]
        assert abs((hue_after - hue_before + 0.5) % 1 - 0.5) < 1e-3


def test_muted_colours_gain_proportionally_more_than_vivid_ones() -> None:
    saturations = np.linspace(0.05, 0.95, 10, dtype=np.float32)
    row = np.stack([np.ones_like(saturations), 1 - saturations, 1 - saturations], axis=-1)[None]  # tints of red
    gain = chroma(registry.apply(NAME, row, amount=0.8))[0] / chroma(row)[0]
    assert (gain >= 1 - 1e-6).all()
    assert (np.diff(gain) < 0).all()  # the more saturated, the smaller the relative gain


def test_more_amount_never_gives_less_chroma(photo: np.ndarray) -> None:
    weaker, stronger = registry.apply(NAME, photo, amount=0.3), registry.apply(NAME, photo, amount=0.6)
    assert (chroma(stronger) >= chroma(weaker) - 1e-6).all()


def test_greys_are_unchanged_at_every_amount() -> None:
    ramp = np.linspace(0, 1, 11, dtype=np.float32)[None, :, None].repeat(3, axis=2)
    np.testing.assert_allclose(registry.apply(NAME, ramp, amount=1.0), ramp, atol=1e-6)


def test_the_default_leaves_a_photo_unchanged(photo: np.ndarray) -> None:
    np.testing.assert_allclose(registry.apply(NAME, photo), photo, atol=1e-6)
