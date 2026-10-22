"""Behaviour of the `clarity` effect."""

import numpy as np

from photo_fx import effects


def test_clarity_zero_amount_is_identity(photo: np.ndarray) -> None:
    np.testing.assert_allclose(effects.apply("clarity", photo, amount=0.0), photo, atol=1e-6)


def _dot_on_grey(size: int = 21) -> np.ndarray:
    image = np.full((size, size, 3), 0.5, np.float32)
    image[size // 2, size // 2] = 1.0
    return image


def test_clarity_truncate_limits_how_far_the_surroundings_reach() -> None:
    """A Gaussian cut at truncate x sigma (rounded to whole px) has no weight beyond: with sigma 2 and truncate 1 the
    bright dot cannot reach a pixel 4 px away; with the default truncate (4) it does."""
    image = _dot_on_grey()
    near = effects.apply("clarity", image, radius=2.0, truncate=1.0)
    far = effects.apply("clarity", image, radius=2.0)
    assert near[10, 14, 0] == 0.5  # 4 px from the dot: outside the cut kernel, untouched
    assert far[10, 14, 0] < 0.5  # inside the kernel: the bright dot makes it read darker by contrast


def test_clarity_mode_decides_what_lies_beyond_the_picture_edge() -> None:
    flat = np.full((12, 12, 3), 0.5, np.float32)
    np.testing.assert_array_equal(effects.apply("clarity", flat, radius=3.0), flat)  # reflect: the edge sees grey
    dark_outside = effects.apply("clarity", flat, radius=3.0, mode="constant", constant_value=0.0)
    assert dark_outside[0, 0, 0] > 0.5  # darkness beyond the edge makes the border stand out brighter
    assert dark_outside[0, 0, 0] > dark_outside[6, 6, 0]
    grey_outside = effects.apply("clarity", flat, radius=3.0, mode="constant", constant_value=0.5)
    np.testing.assert_allclose(grey_outside, flat, atol=1e-6)


def test_clarity_is_a_light_effect() -> None:
    assert effects.category("clarity") == "light"
