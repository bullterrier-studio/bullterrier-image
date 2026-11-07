"""The `bloom` presets, resolved by `fx`. The kernel is tested in `crates/core/tests/it/bloom.rs`."""

import numpy as np
import pytest

from bullterrier_image import Recipe, fx, render


def flat(value: float, size: int = 32) -> np.ndarray:
    return np.full((size, size, 3), value, np.float32)


def screen(base: float, light: float) -> float:
    """The screen blend mode, from its definition: 1 - (1 - a)(1 - b)."""
    return 1.0 - (1.0 - base) * (1.0 - light)


def with_preset(preset: str, picture: np.ndarray, **values: float) -> np.ndarray:
    return render(picture, Recipe(name="preset", steps=[fx.bloom(preset=preset, **values)]))


# What each preset is, as a light: the sigma of its one Gaussian and its strength per channel on a white spot
# (halation's colour #ff4a14 is 255, 74, 20 of 255).
PRESET_LIGHT = {
    "soft_focus": (12.0, (0.5, 0.5, 0.5)),
    "halation": (30.0, (1.8, 1.8 * 74 / 255, 1.8 * 20 / 255)),
}


@pytest.mark.parametrize("preset", PRESET_LIGHT)
def test_a_preset_spreads_a_white_spot_as_one_gaussian_of_its_sigma_in_its_light(preset: str) -> None:
    """The impulse response worked out here: a lone white pixel on black becomes, in each channel, the preset's
    strength times a 2-D Gaussian of its sigma (a single octave)."""
    sigma, strengths = PRESET_LIGHT[preset]
    size = 480  # the preset's px are design px: on a picture 480 px wide they are the picture's px
    centre = size // 2
    picture = flat(0.0, size)
    picture[centre, centre] = 1.0
    y, x = np.mgrid[0:size, 0:size]
    squared = ((x - centre) ** 2 + (y - centre) ** 2).astype(np.float64)
    gaussian = np.exp(-squared / (2 * sigma**2)) / (2 * np.pi * sigma**2)
    inside = (squared > 0) & (squared <= (2.5 * sigma) ** 2)  # well within the blur's reach of 4 sigma
    out = with_preset(preset, picture)
    for channel, strength in enumerate(strengths):
        np.testing.assert_allclose(out[..., channel][inside], strength * gaussian[inside], atol=2e-6)


def test_the_soft_focus_preset_screens_half_a_blurred_copy_of_the_whole_picture() -> None:
    """With nothing to blur (radius 0) the copy is the picture itself: v screened with v / 2 is v + v/2 (1 - v)."""
    np.testing.assert_allclose(with_preset("soft_focus", flat(0.2), radius=0), flat(0.28), atol=1e-6)  # 0.2 + 0.1 x 0.8


def test_the_soft_focus_preset_brightens_shadows_too(photo: np.ndarray) -> None:
    out = with_preset("soft_focus", photo)
    assert (out >= photo - 1e-6).all()
    assert (out[photo < 0.3] > photo[photo < 0.3]).all()  # threshold 0: even the shadows glow


def test_the_halation_preset_lights_an_even_picture_in_the_film_orange_from_its_brightest_channel() -> None:
    """With nothing to blur (radius 0): the light is amount 1.8 x (0.7 - 0.5) / (1 - 0.5) = 0.72 of #ff4a14."""
    light = 0.72 * np.array([1.0, 74 / 255, 20 / 255])
    expected = [screen(v, c) for v, c in zip((0.7, 0.5, 0.2), light, strict=True)]
    picture = np.broadcast_to(np.array([0.7, 0.5, 0.2], np.float32), (16, 16, 3)).copy()
    np.testing.assert_allclose(with_preset("halation", picture, radius=0)[8, 8], expected, atol=1e-5)
