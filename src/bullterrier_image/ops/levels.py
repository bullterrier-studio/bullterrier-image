"""Sharing a position between evenly spaced levels: the weights that blend renders made at a few values into one
picture that varies pixel by pixel."""

import numpy as np


def level_weight(position: np.ndarray, level: int) -> np.ndarray:
    """The share of `level` at each pixel: `position` (any shape, float32) counts levels, so 2.25 lies a quarter of the
    way from level 2 to level 3. The weight is 1 on the level, falls linearly to 0 at the neighbouring levels and is 0
    beyond them (a tent), so the weights of all levels add up to 1 wherever `position` lies within the levels. float32,
    `position`'s shape; a level no pixel uses is all zeros."""
    return np.clip(1.0 - np.abs(position - level), 0.0, 1.0)
