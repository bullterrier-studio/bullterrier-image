"""Edge detection: where the brightness changes quickly, and which way it changes.

One call, `detect_edges`, answers both questions for a grey or RGB picture and returns them as two aligned maps
(`Edges`): how strong the edge is at each pixel, and the direction of the brightness gradient there. The direction
convention is the same for every method, so an effect can switch method without touching its own geometry code:

- x points right and y points DOWN the picture (the array's row axis), and `direction = atan2(d/dy, d/dx)`.
- 0 means brightness rises towards the right, +pi/2 means it rises towards the bottom of the picture.
- The direction is the edge's normal: a stroke drawn along the edge runs at `direction + pi/2`.

Magnitudes are normalised so that the strongest response any 0..1 picture can produce reads 1.0, whatever the method,
and they scale linearly with contrast and ignore a constant brightness offset (except `canny`, which is binary).
"""

import math
from dataclasses import dataclass
from typing import Literal

import numpy as np
from scipy import ndimage

__all__ = ["Edges", "detect_edges"]

# Rec. 709 luminance weights for red, green, blue.
_LUMA = np.array([0.2126, 0.7152, 0.0722], dtype=np.float32)

# The strongest 3x3 Sobel response to pixels in 0..1 is the corner pattern (three bright pixels in one corner, six
# dark), sqrt(4^2 + 2^2). Dividing by it keeps every magnitude in [0, 1] without clipping, so scaling stays linear.
_SOBEL_MAX = math.sqrt(20.0)

# Difference of Gaussians: the wider blur is this many times the narrower one (the classic ratio that approximates the
# Laplacian of Gaussian).
_DOG_RATIO = 1.6

# Gradients closer than this share of each other count as equal when thinning edges (float32 rounding is ~1e-7).
_TIE = 1e-4

_GAUSSIAN_TRUNCATE = 4.0  # scipy's default; named because the DoG normalisation has to rebuild the very same kernels


@dataclass(frozen=True)
class Edges:
    """Edge strength and direction for every pixel of a picture.

    Attributes:
        magnitude: `(h, w)` float32 in [0, 1]. Edge strength, normalised so the strongest possible step is 1.
        direction: `(h, w)` float32 in radians, in (-pi, pi]. Angle of the brightness gradient (the edge's normal)
            with x to the right and y down; 0 means brightness rises towards the right. Where there is no gradient
            it is 0.
    """

    magnitude: np.ndarray
    direction: np.ndarray


def detect_edges(
    image: np.ndarray,
    method: Literal["sobel", "canny", "dog"] = "sobel",
    *,
    sigma: float = 1.0,
    low: float = 0.1,
    high: float = 0.3,
) -> Edges:
    """Find the edges of a picture and the direction each one faces.

    Args:
        image: Grey `(h, w)` or RGB `(h, w, 3)` array, float, values in 0..1. RGB is reduced to Rec. 709 luminance
            first. It is never modified.
        method: `"sobel"` gives the gradient magnitude, a soft map that is strongest on the edge itself. `"dog"` gives
            `|G(sigma) - G(1.6 sigma)|`, a band-pass response that is zero exactly on a step and peaks on both sides
            of it (about one to two sigma away); it favours features about `sigma` wide. `"canny"` gives thin,
            one-pixel lines: non-maximum suppression across the edge, then hysteresis (see `low`, `high`); magnitude
            is 1.0 on kept pixels and 0 elsewhere.
        sigma: Standard deviation of the Gaussian pre-blur, in pixels, greater than 0. Larger values ignore finer
            detail and noise. For `"dog"` it is the narrower of the two blurs.
        low: `"canny"` only. Weakest edge strength that may still belong to a line, as a share (0 to below `high`) of
            the strongest gradient in this picture. Weak pixels are kept only when connected to a strong one.
        high: `"canny"` only. Edge strength that starts a line, as a share (above `low`, at most 1) of the strongest
            gradient in this picture. Raising it never adds edge pixels. `low` and `high` are validated whatever the
            method, so a recipe fails the same way regardless of which method it picks.

    Returns:
        `Edges` with `(h, w)` float32 `magnitude` and `direction`, see its docstring.

    Raises:
        ValueError: `image` is not `(h, w)` or `(h, w, 3)`; `method` is unknown; `sigma` is not a positive finite
            number; or `low`/`high` do not satisfy `0 <= low < high <= 1`. The message names the argument.
    """
    if method not in ("sobel", "canny", "dog"):
        raise ValueError(f"method must be 'sobel', 'canny' or 'dog', got {method!r}")
    if not (0 < sigma < math.inf):
        raise ValueError(f"sigma must be a positive, finite number of pixels, got {sigma}")
    if not low >= 0:
        raise ValueError(f"low must be at least 0, got {low}")
    if not high <= 1:
        raise ValueError(f"high must be at most 1, got {high}")
    if not low < high:
        raise ValueError(f"low must be smaller than high, got low={low}, high={high}")
    grey = _to_grey(image)

    blurred = ndimage.gaussian_filter(grey, sigma, mode="nearest")
    gx, gy = _sobel(blurred)
    gradient = np.hypot(gx, gy) / _SOBEL_MAX
    direction = _gradient_direction(gx, gy)

    if method == "sobel":
        magnitude = gradient
    elif method == "dog":
        magnitude = _difference_of_gaussians(grey, blurred, sigma)
    else:
        magnitude = _canny_lines(gradient, direction, low, high)
    return Edges(magnitude=magnitude.astype(np.float32, copy=False), direction=direction)


def _to_grey(image: np.ndarray) -> np.ndarray:
    """A fresh float32 `(h, w)` luminance picture whose darkest pixel is exactly 0."""
    array = np.asarray(image)
    if array.ndim == 3 and array.shape[2] == 3:
        grey = array.astype(np.float32) @ _LUMA
    elif array.ndim == 2:
        grey = array.astype(np.float32)
    else:
        raise ValueError(f"image must have shape (h, w) or (h, w, 3), got {array.shape}")
    # Only differences matter, so shifting the darkest pixel to 0 loses nothing; it makes a flat picture exactly flat
    # (rounding in the blurs would otherwise leave ~1e-8 specks) and offsets exactly irrelevant.
    return grey - grey.min() if grey.size else grey


def _sobel(grey: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """Un-normalised Sobel derivatives (d/dx, d/dy) with x to the right and y down; borders replicate the edge pixel."""
    # Written with shifted slices rather than ndimage.correlate1d: about three times faster on large pictures.
    padded = np.pad(grey, 1, mode="edge")
    across_x = padded[:, 2:] - padded[:, :-2]  # (h + 2, w): right minus left
    across_y = padded[2:, :] - padded[:-2, :]  # (h, w + 2): below minus above
    gx = across_x[:-2] + 2 * across_x[1:-1] + across_x[2:]
    gy = across_y[:, :-2] + 2 * across_y[:, 1:-1] + across_y[:, 2:]
    return gx, gy


def _gradient_direction(gx: np.ndarray, gy: np.ndarray) -> np.ndarray:
    """Angle of (gx, gy) in (-pi, pi] as float32; 0 where both are zero."""
    angle = np.arctan2(gy, gx).astype(np.float32)
    # atan2 returns -pi for a leftward gradient with y = -0.0, and signed zeros are an accident of the arithmetic; the
    # documented range excludes -pi.
    angle[angle <= -np.float32(np.pi)] = np.float32(np.pi)
    angle[(gx == 0) & (gy == 0)] = 0.0
    return angle


def _difference_of_gaussians(grey: np.ndarray, narrow: np.ndarray, sigma: float) -> np.ndarray:
    """|G(sigma) - G(1.6 sigma)| of `grey`, divided by the largest value any 0..1 picture could give."""
    wide = ndimage.gaussian_filter(grey, _DOG_RATIO * sigma, mode="nearest")
    scale = _dog_peak_response(sigma)
    if scale == 0.0:  # both blurs are narrower than a pixel, so they are the identity and cancel
        return np.zeros_like(grey)
    return np.abs(narrow - wide) / np.float32(scale)


def _dog_peak_response(sigma: float) -> float:
    """The most |G(sigma) - G(1.6 sigma)| can give for 0..1 pixels (bright where its kernel is positive)."""
    wide_sigma = _DOG_RATIO * sigma
    radius = int(_GAUSSIAN_TRUNCATE * wide_sigma + 0.5)
    impulse = np.zeros(2 * radius + 1)
    impulse[radius] = 1.0
    # Filtering an impulse with the very function used on the picture yields exactly the 1-D kernels it applied.
    narrow_k = ndimage.gaussian_filter1d(impulse, sigma, truncate=_GAUSSIAN_TRUNCATE)
    wide_k = ndimage.gaussian_filter1d(impulse, wide_sigma, truncate=_GAUSSIAN_TRUNCATE)
    positive = negative = 0.0
    for narrow_i, wide_i in zip(narrow_k, wide_k, strict=True):  # row by row: memory O(radius), not O(radius^2)
        row = narrow_i * narrow_k - wide_i * wide_k
        positive += row[row > 0].sum()
        negative -= row[row < 0].sum()
    return max(positive, negative)


def _canny_lines(gradient: np.ndarray, direction: np.ndarray, low: float, high: float) -> np.ndarray:
    """Thin edge lines: 1.0 on kept pixels, 0 elsewhere."""
    if not gradient.any():
        return np.zeros_like(gradient)
    ridge = _non_maximum_suppression(gradient, direction)
    strongest = gradient.max()
    candidate = ridge & (gradient > low * strongest)
    seed = ridge & (gradient > high * strongest)

    # Hysteresis: keep every connected group of candidate pixels that contains at least one seed.
    groups, count = ndimage.label(candidate, structure=np.ones((3, 3), dtype=bool))
    has_seed = np.zeros(count + 1, dtype=bool)
    has_seed[groups[seed]] = True
    has_seed[0] = False  # label 0 is the background
    return has_seed[groups].astype(np.float32)


def _non_maximum_suppression(gradient: np.ndarray, direction: np.ndarray) -> np.ndarray:
    """Mask of pixels not weaker than their two neighbours along the gradient, i.e. the crest across an edge."""
    height, width = gradient.shape
    padded = np.pad(gradient, 1)
    # The gradient axis rounded to the nearest of four lines through the pixel (0, 45, 90, 135 degrees). The angle is
    # taken modulo pi because a direction and its opposite name the same pair of neighbours.
    sector = np.floor(np.mod(direction, np.pi) / (np.pi / 4) + 0.5).astype(np.int8) % 4
    # (dy, dx) of the "next" neighbour for each sector; the "previous" one is the opposite offset.
    steps = ((0, 1), (1, 1), (1, 0), (1, -1))
    keep = np.zeros(gradient.shape, dtype=bool)
    for index, (dy, dx) in enumerate(steps):
        after = padded[1 + dy : 1 + dy + height, 1 + dx : 1 + dx + width]
        before = padded[1 - dy : 1 - dy + height, 1 - dx : 1 - dx + width]
        # A step between two pixels makes both equally strong. The earlier one wins such a tie, and "equally" allows for
        # rounding so that the winner does not flip when the picture is shifted in brightness or transposed.
        keep |= (sector == index) & (gradient >= after * (1 - _TIE)) & (gradient > before * (1 + _TIE))
    return keep
