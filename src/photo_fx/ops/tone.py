"""Tone remaps: how brightness values are moved onto other brightness values, the same way for every effect that
reshapes tones (`levels`, `auto_levels`, `curves`).

Promises: every function takes a float32 picture whose last axis is the colour channels, returns a new float32 picture
of the same shape (the input is never modified), and a value in 0..1 comes out in 0..1.
"""

import numpy as np

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
