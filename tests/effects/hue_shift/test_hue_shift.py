"""Behaviour of the `hue_shift` effect."""

import numpy as np

from photo_fx.core import registry
from photo_fx.ops.color import luminance


def test_hue_shift_full_turn_is_identity(photo: np.ndarray) -> None:
    np.testing.assert_allclose(registry.apply("hue_shift", photo, hue=360), photo, atol=1e-5)


def test_hue_shift_keeps_brightness_and_moves_colour() -> None:
    skin_and_wall = np.array([[[0.8, 0.6, 0.5], [0.5, 0.35, 0.3], [0.6, 0.4, 0.4]]], np.float32)  # in-gamut colours
    out = registry.apply("hue_shift", skin_and_wall, hue=120)
    np.testing.assert_allclose(luminance(out), luminance(skin_and_wall), atol=1e-4)
    assert (out.argmax(axis=-1) != skin_and_wall.argmax(axis=-1)).all()  # the dominant channel moved


def test_hue_shift_is_a_colour_effect() -> None:
    assert registry.category("hue_shift") == "colour"
