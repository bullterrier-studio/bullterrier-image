"""The `displace` presets, resolved by `fx`. The kernel is tested in `crates/core/tests/it/displace.rs`."""

import numpy as np

from bullterrier_image import Recipe, fx, render


def test_the_scatter_preset_breaks_an_edge_into_grain_no_wider_than_its_amount() -> None:
    """A black left half and a white right half, 480 px wide so the preset's 14 design px are 14 px: each pixel jumps
    to a pixel at most 14 px away (nearest reading keeps pure black and white), so beyond 14 px of the edge nothing
    changes and within it both colours mix."""
    picture = np.zeros((96, 480, 3), np.float32)
    picture[:, 240:] = 1.0
    out = render(picture, Recipe([fx.displace(preset="scatter")], name="t", size=None))
    assert set(np.unique(out)) <= {0.0, 1.0}
    np.testing.assert_array_equal(out[:, :225], picture[:, :225])
    np.testing.assert_array_equal(out[:, 255:], picture[:, 255:])
    assert 0 < (out[:, 226:240] == 1.0).mean() < 1  # grain: white strays onto the black side
