"""Behaviour of the `grade` effect."""

import colorsys

import numpy as np
import pytest

from bullterrier_image.core import registry

FLOAT32_TOLERANCE = 2 * float(np.finfo(np.float32).eps)  # 2 x float32 epsilon: what grade's rounding may move a value
FIVE_DECIMALS = 1e-5  # the expected values in the table above are written to five decimals


def test_grade_exposure_one_stop_doubles_brightness() -> None:
    dark = np.full((8, 8, 3), 0.2, dtype=np.float32)
    assert np.allclose(registry.apply("grade", dark, exposure=1.0, contrast=1.0), 0.4, atol=1e-5)


def test_grade_contrast_turns_around_its_pivot() -> None:
    grey = np.array([[[0.3, 0.3, 0.3], [0.4, 0.4, 0.4], [0.5, 0.5, 0.5]]], np.float32)
    pivoted = registry.apply("grade", grey, contrast=2.0, pivot=0.3)
    np.testing.assert_allclose(pivoted[0, :, 0], [0.3, 0.5, 0.7], atol=1e-6)  # 0.3 + (v - 0.3) * 2
    default = registry.apply("grade", grey, contrast=2.0)
    np.testing.assert_allclose(default[0, :, 0], [0.1, 0.3, 0.5], atol=1e-6)  # around 0.5: 0.5 + (v - 0.5) * 2


def chroma(picture: np.ndarray) -> np.ndarray:
    """Strongest minus weakest channel of every pixel: the HSV chroma."""
    return picture.max(axis=-1) - picture.min(axis=-1)


def hsv_vibrance(picture: np.ndarray, vibrance: float) -> np.ndarray:
    """Oracle: raise HSV saturation s to s + vibrance * s * (1 - s) with the standard-library conversion."""
    out = np.zeros_like(picture)
    for y in range(picture.shape[0]):
        for x in range(picture.shape[1]):
            h, s, v = colorsys.rgb_to_hsv(*(float(c) for c in picture[y, x]))
            out[y, x] = colorsys.hsv_to_rgb(h, s + vibrance * s * (1 - s), v)
    return out


@pytest.mark.parametrize(
    ("pixel", "vibrance", "expected"),
    [
        # Saturation S = (max - min) / max rises to S + vibrance * S * (1 - S) with the strongest channel kept.
        pytest.param((0.6, 0.5, 0.4), 0.5, (0.6, 0.46667, 0.33333), id="muted: S = 1/3 rises to 0.4444"),
        pytest.param((0.6, 0.5, 0.4), 1.0, (0.6, 0.43333, 0.26667), id="muted at the maximum: S rises to 0.5556"),
        pytest.param((0.6, 0.5, 0.4), 0.0, (0.6, 0.5, 0.4), id="vibrance 0 changes nothing"),
        pytest.param((1.0, 0.0, 0.0), 1.0, (1.0, 0.0, 0.0), id="a fully saturated colour has no room to rise"),
        pytest.param((0.5, 0.5, 0.5), 1.0, (0.5, 0.5, 0.5), id="a grey stays grey"),
        pytest.param((0.0, 0.0, 0.0), 1.0, (0.0, 0.0, 0.0), id="black stays black"),
    ],
)
def test_vibrance_raises_muted_colours_by_the_documented_amount(
    pixel: tuple[float, float, float], vibrance: float, expected: tuple[float, float, float]
) -> None:
    out = registry.apply("grade", np.array([[pixel]], np.float32), vibrance=vibrance)
    np.testing.assert_allclose(out[0, 0], expected, atol=FIVE_DECIMALS)


def test_vibrance_agrees_with_the_standard_library_hsv_conversion(photo: np.ndarray) -> None:
    small = photo[:5, :6]
    np.testing.assert_allclose(
        registry.apply("grade", small, vibrance=0.7), hsv_vibrance(small, 0.7), atol=FIVE_DECIMALS
    )


@pytest.mark.parametrize("saturation", [2.0, 5.0])
def test_vibrance_works_on_the_saturated_picture(saturation: float) -> None:
    muted = np.array([[[0.6, 0.5, 0.4], [0.5, 0.55, 0.45], [0.9, 0.3, 0.2]]], np.float32)  # at 5 the last clips
    saturated = registry.apply("grade", muted, saturation=saturation)
    np.testing.assert_allclose(
        registry.apply("grade", muted, saturation=saturation, vibrance=1.0),
        hsv_vibrance(saturated, 1.0),
        atol=FIVE_DECIMALS,
    )


@pytest.mark.parametrize("vibrance", [0.1, 0.5, 1.0])
def test_vibrance_never_lowers_chroma_and_keeps_hue_and_value(vibrance: float, photo: np.ndarray) -> None:
    base = registry.apply("grade", photo)
    out = registry.apply("grade", photo, vibrance=vibrance)
    assert (chroma(out) >= chroma(base) - FLOAT32_TOLERANCE).all()
    np.testing.assert_allclose(out.max(axis=-1), base.max(axis=-1), atol=FLOAT32_TOLERANCE)
    coloured = chroma(base) > 0.05  # hue is ill-defined and float-sensitive for near-greys
    for before, after in zip(base[coloured][:300], out[coloured][:300], strict=True):
        hue_before = colorsys.rgb_to_hsv(*(float(c) for c in before))[0]
        hue_after = colorsys.rgb_to_hsv(*(float(c) for c in after))[0]
        assert abs((hue_after - hue_before + 0.5) % 1 - 0.5) < 1e-3


def test_vibrance_gives_muted_colours_proportionally_more_than_vivid_ones() -> None:
    saturations = np.linspace(0.05, 0.95, 10, dtype=np.float32)
    row = np.stack([np.ones_like(saturations), 1 - saturations, 1 - saturations], axis=-1)[None]  # tints of red
    gain = chroma(registry.apply("grade", row, vibrance=0.8))[0] / chroma(row)[0]
    assert (gain >= 1 - FLOAT32_TOLERANCE).all()
    assert (np.diff(gain) < 0).all()  # the more saturated, the smaller the relative gain


def test_more_vibrance_never_gives_less_chroma(photo: np.ndarray) -> None:
    weaker, stronger = registry.apply("grade", photo, vibrance=0.3), registry.apply("grade", photo, vibrance=0.6)
    assert (chroma(stronger) >= chroma(weaker) - FLOAT32_TOLERANCE).all()


def test_vibrance_leaves_greys_unchanged() -> None:
    ramp = np.linspace(0, 1, 11, dtype=np.float32)[None, :, None].repeat(3, axis=2)
    np.testing.assert_allclose(registry.apply("grade", ramp, vibrance=1.0), ramp, atol=FLOAT32_TOLERANCE)


def test_vibrance_has_nothing_to_lift_once_saturation_is_zero(photo: np.ndarray) -> None:
    flat = registry.apply("grade", photo, saturation=0.0)
    np.testing.assert_allclose(
        registry.apply("grade", photo, saturation=0.0, vibrance=1.0), flat, atol=FLOAT32_TOLERANCE
    )


def test_the_default_leaves_a_photo_unchanged(photo: np.ndarray) -> None:
    np.testing.assert_allclose(registry.apply("grade", photo), photo, atol=FLOAT32_TOLERANCE)
