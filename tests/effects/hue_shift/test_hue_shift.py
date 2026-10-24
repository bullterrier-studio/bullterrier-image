"""Behaviour of the `hue_shift` effect."""

import numpy as np

from photo_fx.core import registry


def test_hue_shift_full_turn_is_identity(photo: np.ndarray) -> None:
    np.testing.assert_allclose(registry.apply("hue_shift", photo, hue=360), photo, atol=1e-5)


def test_hue_shift_keeps_brightness_and_moves_colour() -> None:
    skin_and_wall = np.array([[[0.8, 0.6, 0.5], [0.5, 0.35, 0.3], [0.6, 0.4, 0.4]]], np.float32)  # in-gamut colours
    out = registry.apply("hue_shift", skin_and_wall, hue=120)
    rec_709 = [0.2126, 0.7152, 0.0722]  # ITU-R BT.709 luma weights of red, green, blue
    np.testing.assert_allclose(out @ rec_709, skin_and_wall @ rec_709, atol=1e-4)
    assert (out.argmax(axis=-1) != skin_and_wall.argmax(axis=-1)).all()  # the dominant channel moved
