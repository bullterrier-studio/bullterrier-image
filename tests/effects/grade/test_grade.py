"""Behaviour of the `grade` effect."""

import numpy as np

from photo_fx.core import registry


def test_grade_exposure_one_stop_doubles_brightness() -> None:
    dark = np.full((8, 8, 3), 0.2, dtype=np.float32)
    assert np.allclose(registry.apply("grade", dark, exposure=1.0, contrast=1.0), 0.4, atol=1e-5)


def test_grade_contrast_turns_around_its_pivot() -> None:
    grey = np.array([[[0.3, 0.3, 0.3], [0.4, 0.4, 0.4], [0.5, 0.5, 0.5]]], np.float32)
    pivoted = registry.apply("grade", grey, contrast=2.0, pivot=0.3)
    np.testing.assert_allclose(pivoted[0, :, 0], [0.3, 0.5, 0.7], atol=1e-6)  # 0.3 + (v - 0.3) * 2
    default = registry.apply("grade", grey, contrast=2.0)
    np.testing.assert_allclose(default[0, :, 0], [0.1, 0.3, 0.5], atol=1e-6)  # around 0.5: 0.5 + (v - 0.5) * 2


def test_grade_is_a_colour_effect() -> None:
    assert registry.category("grade") == "colour"
