"""Tone remaps: how brightness values are moved onto other brightness values, the same way for every effect that
reshapes tones (`levels`, `auto_levels`, `curves`).

Promises: every function takes a float32 array (a picture, whose last axis is the colour channels, or any shape),
returns a new float32 array of the same shape (the input is never modified), and values in 0..1 come out in 0..1.
"""

from collections.abc import Sequence

import numpy as np
from scipy.interpolate import PchipInterpolator

# constant: only keeps equal or crossed input points from dividing by zero; the span then counts as this tiny, so the
# picture is cut at the black point into the two output ends (a threshold).
_MIN_SPAN = 1e-6


def remap_levels(
    rgb: np.ndarray,
    *,
    black: float | tuple[float, ...],
    white: float | tuple[float, ...],
    gamma: float,
    output_black: float | tuple[float, ...],
    output_white: float | tuple[float, ...],
) -> np.ndarray:
    """Photoshop's Levels: normalise between the input points, apply the midtone gamma, place between the output points.

    A value at `black` or below goes to `output_black`, one at `white` or above to `output_white`, and between them
    out = output_black + ((value - black) / (white - black)) ** (1 / gamma) * (output_white - output_black). So a
    `gamma` above 1 brightens the midtones. Every point is a number shared by all channels or one number per channel
    (a tuple as long as the last axis). Input points that are equal or crossed cut the picture at `black` into the
    two output ends instead of dividing by zero. Points are in 0..1 and `gamma` is above 0.
    """
    low = np.asarray(black, np.float32)
    span = np.maximum(np.asarray(white, np.float64) - np.asarray(black, np.float64), _MIN_SPAN).astype(np.float32)
    x = np.clip((rgb - low) / span, 0.0, 1.0)
    x = x ** np.float32(1.0 / gamma)
    start = np.asarray(output_black, np.float32)
    reach = (np.asarray(output_white, np.float64) - np.asarray(output_black, np.float64)).astype(np.float32)
    return (start + x * reach).astype(np.float32)


def monotone_curve(inputs: Sequence[float], outputs: Sequence[float], values: np.ndarray) -> np.ndarray:
    """The tone curve through the control points (`inputs[i]`, `outputs[i]`), read at `values`.

    The curve is the piecewise cubic Hermite interpolant with Fritsch-Carlson slopes (PCHIP, the monotone cubic of
    Fritsch and Carlson, 1980; the same as `scipy.interpolate.PchipInterpolator`): smooth, passing exactly through every
    point, and never overshooting. Between two points it stays between their outputs, so where the outputs rise the
    curve rises (a brighter value is never mapped darker) and a bump or dip is never invented. With two points it is a
    straight line. Below the first input and above the last the curve is held at that point's output. `inputs` must
    strictly increase (a ValueError says so otherwise) and all `inputs` and `outputs` are in 0..1.
    """
    curve = PchipInterpolator(np.asarray(inputs, np.float64), np.asarray(outputs, np.float64))
    low, high = inputs[0], inputs[-1]
    # the clip on the way out only removes rounding error, so the promise above holds exactly
    return np.clip(curve(np.clip(values, low, high)), min(outputs), max(outputs)).astype(np.float32)
