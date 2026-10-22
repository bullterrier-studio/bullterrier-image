"""Behaviour of photo_fx.ops.warp, checked against independent oracles (np.roll, np.pad, hand-computed bilinear values)."""

import numpy as np
import pytest

from photo_fx.ops.warp import pixel_grid, remap

MODES = ["clamp", "mirror", "wrap", "fill"]


def test_pixel_grid_is_column_and_row_index():
    x, y = pixel_grid((3, 5))
    assert x.shape == y.shape == (3, 5)
    assert x.dtype == y.dtype == np.float32
    assert x.tolist() == [[0, 1, 2, 3, 4]] * 3
    assert y.tolist() == [[0] * 5, [1] * 5, [2] * 5]
