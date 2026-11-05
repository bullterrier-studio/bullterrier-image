"""Screening light over a picture, as the Python light effects that remain do."""

import numpy as np


def screen(base: np.ndarray, light: np.ndarray) -> np.ndarray:
    """`light` screened over `base`: `1 - (1 - base) * (1 - light)`, written `base + light * (1 - base)`, in float32.

    `base` is a float picture in 0..1 and `light` an array that broadcasts against it (a halo of the same shape, or a
    halo times a colour). Light is clipped to 0..1 first, so the result never lies below `base` and never above 1:
    black light leaves `base` exactly as it is, full light gives white, and light adds up without clipping harshly.
    """
    return (base + np.clip(light, 0, 1) * (1.0 - base)).astype(np.float32)
