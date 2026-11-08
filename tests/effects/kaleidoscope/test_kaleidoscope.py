"""The `kaleidoscope` presets, resolved by `fx`. The kernel is tested in `crates/core/tests/it/kaleidoscope.rs`."""

import numpy as np

from bullterrier_image import Recipe, fx, render


def test_the_mirror_preset_copies_the_left_half_reflected_onto_the_right(photo) -> None:
    out = render(photo, Recipe([fx.kaleidoscope(preset="mirror")], name="t", size=None))
    half = photo.shape[1] // 2
    np.testing.assert_array_equal(out[:, :half], photo[:, :half])
    np.testing.assert_array_equal(out[:, half:], photo[:, :half][:, ::-1])
