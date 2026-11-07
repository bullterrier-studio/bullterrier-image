"""The `sharpen` presets, resolved by `Step`. The kernel is tested in `crates/core/tests/it/sharpen.rs`."""

import numpy as np
from scipy import ndimage

from bullterrier_image.core import registry
from bullterrier_image.core.steps import Step

NAME = "sharpen"
# A blur reaching 32 px or more is an FFT convolution within 1e-5 of scipy's filter (the promise of `gaussian_blur`);
# the result is 2 x picture - blur, so the blur's error counts once; 2e-5 adds float32 headroom to that promise.
WIDE_BLUR_VS_SCIPY = 2e-5


def test_the_clarity_preset_is_local_contrast_against_a_wide_blur() -> None:
    """Checked against scipy's Gaussian filter: picture + (picture - its 30 px blur), kept within 0..1."""
    picture = np.random.default_rng(5).random((70, 90, 3), dtype=np.float32)
    blurred = ndimage.gaussian_filter(picture, (30, 30, 0), mode="reflect")
    expected = np.clip(2 * picture - blurred, 0, 1)
    out = registry.apply(NAME, picture, **Step(NAME, preset="clarity").params)
    np.testing.assert_allclose(out, expected, atol=WIDE_BLUR_VS_SCIPY)


def test_a_parameter_given_beside_the_clarity_preset_wins() -> None:
    picture = np.random.default_rng(6).random((40, 50, 3), dtype=np.float32)
    off = Step(NAME, params={"amount": 0.0}, preset="clarity").params
    np.testing.assert_array_equal(registry.apply(NAME, picture, **off), picture)
