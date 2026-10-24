"""The two steps every glowing effect shares: picking out the highlights, and screening light over the picture."""

import numpy as np


def highlights(rgb: np.ndarray, threshold: float) -> np.ndarray:
    """The part of each value above `threshold`, rescaled so that white is 1: `(value - threshold) / (1 - threshold)`.

    `rgb` is a float picture in 0..1; every channel is judged on its own, so coloured light keeps its colour. The
    result has the input's shape and dtype, lies in 0..1, is 0 wherever the input is at or below `threshold` and rises
    linearly to 1 at white. `threshold` must be in 0..1 and below 1 (a threshold of 1 has nothing above it).
    """
    return np.clip((rgb - threshold) / (1.0 - threshold), 0.0, 1.0)


def screen(base: np.ndarray, light: np.ndarray) -> np.ndarray:
    """`light` screened over `base`: `1 - (1 - base) * (1 - light)`, written `base + light * (1 - base)`, in float32.

    `base` is a float picture in 0..1 and `light` an array that broadcasts against it (a halo of the same shape, or a
    halo times a colour). Light is clipped to 0..1 first, so the result never lies below `base` and never above 1:
    black light leaves `base` exactly as it is, full light gives white, and light adds up without clipping harshly.
    """
    return (base + np.clip(light, 0, 1) * (1.0 - base)).astype(np.float32)
