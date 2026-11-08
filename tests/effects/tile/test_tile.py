"""The `tile` presets, resolved by the registry. The kernel is tested in `crates/core/tests/it/tile.rs`."""

import numpy as np

from bullterrier_image.core import registry


def test_the_scan_roll_preset_is_the_picture_rolled_down_a_quarter(photo) -> None:
    """The rolling scan of a slipped sync, once an effect of its own: one tile, wrapped around."""
    out = registry.apply("tile", photo, **registry.presets("tile")["scan_roll"].params)
    np.testing.assert_allclose(out, np.roll(photo, 24, axis=0), atol=1e-6)
