"""The `lens_distortion` presets, resolved by `fx`. The kernel: `crates/core/tests/it/lens_distortion.rs`."""

import numpy as np

from bullterrier_image import Recipe, fx, render


def test_the_fisheye_preset_swells_the_middle() -> None:
    """On a horizontal ramp a pixel right of the centre shows a point nearer the centre when the middle swells, so
    its value lies between the centre value 0.5 and the ramp's own. The centre stays put."""
    ramp = np.tile(np.linspace(0, 1, 101, dtype=np.float32)[None, :, None], (101, 1, 3))
    out = render(ramp, Recipe([fx.lens_distortion(preset="fisheye")], name="t", size=None))
    row = out[50, :, 0] - 0.5
    assert row[50] == 0.0
    assert 0 < row[60] < ramp[50, 60, 0] - 0.5
