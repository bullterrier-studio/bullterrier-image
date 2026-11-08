"""The `perspective` presets, resolved by `fx`. The kernel is tested in `crates/core/tests/it/perspective.rs`."""

import numpy as np

from bullterrier_image import Recipe, fx, render


def test_the_shear_preset_slides_rows_sideways_in_proportion_to_their_distance_from_the_middle() -> None:
    """A white column on black, centre row 50: the centre row stays, rows 20 above and 20 below slide 0.2 x 20 = 4
    columns, opposite ways."""
    picture = np.zeros((101, 101, 3), np.float32)
    picture[:, 50] = 1.0
    out = render(picture, Recipe([fx.perspective(preset="shear", interpolation="nearest")], name="t", size=None))
    line = out[..., 0].argmax(axis=1)
    assert line[50] == 50
    assert sorted((line[30], line[70])) == [46, 54]
