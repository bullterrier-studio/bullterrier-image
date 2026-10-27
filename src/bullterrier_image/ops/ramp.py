"""Colour ramps: a position from 0 to 1 looked up in a list of colours, the same way for every effect that colours by
a scalar.

Promises: `ramp` returns a new float32 array and never modifies its input.
"""

from collections.abc import Sequence

import numpy as np


def ramp(t: np.ndarray, colors: Sequence[tuple[float, float, float]]) -> np.ndarray:
    """The colour at each position `t` (any shape, values in 0..1) along `colors`, an (r, g, b) list of at least one.

    The colours sit evenly spaced: with n of them, the k-th at t = k / (n - 1), and between two neighbours the colour is
    their linear mix. A single colour is returned everywhere. Returns float32 of shape `t.shape + (3,)`.
    """
    stops = np.array(colors, dtype=np.float32)
    if len(stops) == 1:  # one colour is a flat ramp: the same colour at both ends
        stops = np.repeat(stops, 2, axis=0)  # constant: a ramp has two ends
    position = t * (len(stops) - 1)
    i = np.minimum(position.astype(int), len(stops) - 2)
    f = (position - i)[..., None]
    return (stops[i] * (1 - f) + stops[i + 1] * f).astype(np.float32)
