"""The `rgb_split` presets, resolved by `fx`. The kernel is tested in `crates/core/tests/it/rgb_split.rs`."""

import numpy as np

from bullterrier_image import Recipe, fx, render


def test_the_anaglyph_preset_after_a_grey_is_the_brightness_seen_from_two_sides() -> None:
    """Grey the picture, then split red against cyan. Oracle by hand: Rec. 709 luminance, and for 6 design px (6 px on
    a picture 480 wide) at angle 180 red column j reads the brightness at column j + 6 and green and blue at column
    j - 6, compared away from the borders, where the edge rule decides."""
    picture = np.random.default_rng(3).random((20, 480, 3), dtype=np.float32)
    luma = 0.2126 * picture[..., 0] + 0.7152 * picture[..., 1] + 0.0722 * picture[..., 2]
    recipe = Recipe([fx.grade(saturation=0), fx.rgb_split(preset="anaglyph")], name="t", size=None)
    out = render(picture, recipe)
    np.testing.assert_allclose(out[:, 10:-10, 0], luma[:, 16:-4], atol=1e-5)
    np.testing.assert_allclose(out[:, 10:-10, 1], luma[:, 4:-16], atol=1e-5)
    np.testing.assert_allclose(out[:, 10:-10, 2], luma[:, 4:-16], atol=1e-5)
