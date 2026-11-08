"""The `chromatic_aberration` presets, resolved by `fx`. The kernel is tested in
`crates/core/tests/it/chromatic_aberration.rs`."""

import numpy as np

from bullterrier_image import Recipe, fx, render


def test_the_prism_preset_fringes_an_edge_off_the_centre_in_a_spread_of_colours() -> None:
    """A white block on black, 480 px wide so the preset's design px are px: the centre pixel is not displaced, and
    along the block's right edge the colour channels part, so that the fringe holds more than two grey levels."""
    picture = np.zeros((101, 480, 3), np.float32)
    picture[:, 200:360] = 1.0
    out = render(picture, Recipe([fx.chromatic_aberration(preset="prism")], name="t", size=None))
    np.testing.assert_array_equal(out[50, 240], picture[50, 240])
    fringe = out[50, 340:380]
    assert not np.allclose(fringe[..., 0], fringe[..., 2])
    assert len(np.unique(fringe[..., 0].round(3))) > 3
