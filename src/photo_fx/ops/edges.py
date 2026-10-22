"""Edge detection: where the brightness changes quickly, and which way it changes.

One call, `detect_edges`, answers both questions for a grey or RGB picture and returns them as two aligned maps
(`Edges`): how strong the edge is at each pixel, and the direction of the brightness gradient there. The direction
convention is the same for every method, so an effect can switch method without touching its own geometry code:

- x points right and y points DOWN the picture (the array's row axis), and `direction = atan2(d/dy, d/dx)`.
- 0 means brightness rises towards the right, +pi/2 means it rises towards the bottom of the picture.
- The direction is the edge's normal: a stroke drawn along the edge runs at `direction + pi/2`.

Magnitudes are normalised so that the strongest response any 0..1 picture can produce reads 1.0, whatever the method.
That is a bound, not the reading of a typical edge: see `Edges.magnitude` for what an ideal step reads. Magnitudes scale
linearly with contrast and ignore a constant brightness offset (except `canny`, which is binary).

Every number that shapes the result is a keyword argument of `detect_edges`. What remains fixed is part of the method's
definition: the Rec. 709 luminance weights, the Sobel kernels, and the four-direction rounding of Canny's suppression.
"""

import math
from dataclasses import dataclass
from typing import Literal

import numpy as np
from scipy import ndimage

__all__ = ["Edges", "detect_edges"]

# Rec. 709 luminance weights for red, green, blue (ITU-R BT.709 standard).
_LUMA = np.array([0.2126, 0.7152, 0.0722], dtype=np.float32)

# The strongest 3x3 Sobel response to pixels in 0..1 is the corner pattern (three bright pixels in one corner, six
# dark), sqrt(4^2 + 2^2). Dividing by it keeps every magnitude in [0, 1] without clipping, so scaling stays linear.
# It is derived from the Sobel kernels, not a tunable.
_SOBEL_MAX = math.sqrt(20.0)

BorderMode = Literal["nearest", "reflect", "mirror", "wrap"]

# scipy.ndimage's names for the border behaviours (keys) and numpy.pad's names for the same ones (values). The blur and
# the Sobel derivative must extend the picture the same way, so both are driven from the one `border_mode` argument.
_NUMPY_PAD_MODE = {"nearest": "edge", "reflect": "symmetric", "mirror": "reflect", "wrap": "wrap"}


@dataclass(frozen=True)
class Edges:
    """Edge strength and direction for every pixel of a picture.

    Attributes:
        magnitude: `(h, w)` float32 in [0, 1]. Edge strength, normalised so the strongest response any 0..1 picture
            can produce is exactly 1. Real edges read lower: for `"sobel"` the divisor is sqrt(20), the Sobel
            maximum, which is a diagonal corner pattern, so an ideal black-to-white axis-aligned step reads
            4 / sqrt(20) = 0.894 with no blur and about 0.57 at the default `sigma` of 1. `"dog"` is divided by the
            strongest response of its own kernel the same way (an ideal step reads well below 1). `"canny"` is 1.0
            on kept edge pixels and 0 elsewhere, regardless of edge strength.
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
    dog_ratio: float = 1.6,
    truncate: float = 4.0,
    border_mode: BorderMode = "nearest",
    tie_tolerance: float = 1e-4,
    connectivity: Literal[4, 8] = 8,
) -> Edges:
    """Find the edges of a picture and the direction each one faces.

    Args:
        image: Grey `(h, w)` or RGB `(h, w, 3)` array, at least one pixel, values in 0..1 (the project's float
            pictures; an 8-bit picture must be divided by 255 first). Values outside 0..1, NaN and infinity are
            rejected rather than silently producing magnitudes above 1. RGB is reduced to Rec. 709 luminance first.
            It is never modified.
        method: `"sobel"` gives the gradient magnitude, a soft map that is strongest on the edge itself; `sigma`
            attenuates it (an ideal step reads about 0.57 at sigma 1, see `Edges.magnitude`). `"dog"` gives
            `|G(sigma) - G(dog_ratio * sigma)|`, a band-pass response that is zero exactly on a step and peaks on both
            sides of it (about one to two sigma away); it favours features about `sigma` wide. `"canny"` gives thin
            lines: non-maximum suppression across the edge, then hysteresis (see `low`, `high`); magnitude is 1.0 on
            kept pixels and 0 elsewhere. A step exactly 45 degrees to the pixel grid comes out two pixels thick (two
            parallel diagonal pixel chains), still at most two pixels in every row and column.
        sigma: Standard deviation of the Gaussian pre-blur, in pixels, greater than 0. Larger values ignore finer
            detail and noise. For `"dog"` it is the narrower of the two blurs.
        low: `"canny"` only. Weakest edge strength that may still belong to a line, as a share (0 to below `high`) of
            the strongest gradient in this picture. Weak pixels are kept only when connected to a strong one.
        high: `"canny"` only. Edge strength that starts a line, as a share (above `low`, at most 1) of the strongest
            gradient in this picture. Raising it never adds edge pixels. `low` and `high` are validated whatever the
            method, so a recipe fails the same way regardless of which method it picks.
        dog_ratio: `"dog"` only. How many times wider the second blur is than the first (the k of the DoG literature),
            greater than 1; 1.6 approximates the Laplacian of Gaussian, larger values give a wider, stronger band.
        truncate: Radius of every Gaussian kernel, in standard deviations (scipy's `truncate`), greater than 0. Smaller
            values are faster and cut the kernel's tails; 4.0 loses less than 1e-4 of the weight.
        border_mode: How the picture is extended past its border for the blur and the Sobel derivative (scipy.ndimage
            names): `"nearest"` repeats the border pixel (no false edge at the border), `"reflect"` mirrors including
            the border pixel (d c b a | a b c d), `"mirror"` mirrors excluding it (d c b | a b c d), `"wrap"` tiles
            the picture, so opposite borders count as neighbours.
        tie_tolerance: `"canny"` only. Relative difference (0 up to, not including, 1) below which two neighbouring
            gradient strengths count as equal when thinning edges; the pixel that comes first in reading order wins a
            tie, so a step between two pixels leaves one line. Absorbs float rounding; raise it to thin plateaus of
            near-equal strength.
        connectivity: `"canny"` only. Which pixels count as touching when hysteresis links weak pixels to strong ones:
            4 (edge-sharing only) or 8 (diagonals too, the usual choice).

    Returns:
        `Edges` with `(h, w)` float32 `magnitude` and `direction`, see its docstring.

    Raises:
        ValueError: `image` is empty, is not `(h, w)` or `(h, w, 3)`, or has values outside 0..1 or not finite;
            `method` or `border_mode` is unknown; `sigma` or `truncate` is not a positive finite number; `dog_ratio`
            is not a finite number above 1; `low`/`high` do not satisfy `0 <= low < high <= 1`; `tie_tolerance` is
            outside [0, 1); `connectivity` is not 4 or 8. The message names the argument.
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
    if not (1 < dog_ratio < math.inf):
        raise ValueError(f"dog_ratio must be a finite number above 1, got {dog_ratio}")
    if not (0 < truncate < math.inf):
        raise ValueError(f"truncate must be a positive, finite number of standard deviations, got {truncate}")
    if border_mode not in _NUMPY_PAD_MODE:
        raise ValueError(f"border_mode must be one of {sorted(_NUMPY_PAD_MODE)}, got {border_mode!r}")
    if not 0 <= tie_tolerance < 1:
        raise ValueError(f"tie_tolerance must be at least 0 and below 1, got {tie_tolerance}")
    if connectivity not in (4, 8):
        raise ValueError(f"connectivity must be 4 or 8, got {connectivity!r}")
    grey = _to_grey(image)

    blurred = ndimage.gaussian_filter(grey, sigma, truncate=truncate, mode=border_mode)
    gx, gy = _sobel(blurred, border_mode)
    gradient = np.hypot(gx, gy) / _SOBEL_MAX
    direction = _gradient_direction(gx, gy)

    if method == "sobel":
        magnitude = gradient
    elif method == "dog":
        magnitude = _difference_of_gaussians(grey, blurred, sigma, dog_ratio, truncate, border_mode)
    else:
        magnitude = _canny_lines(gradient, direction, low, high, tie_tolerance, connectivity)
    return Edges(magnitude=magnitude.astype(np.float32, copy=False), direction=direction)


def _to_grey(image: np.ndarray) -> np.ndarray:
    """A fresh float32 `(h, w)` luminance picture whose darkest pixel is exactly 0."""
    array = np.asarray(image)
    if array.ndim not in (2, 3) or (array.ndim == 3 and array.shape[2] != 3):
        raise ValueError(f"image must have shape (h, w) or (h, w, 3), got {array.shape}")
    if array.size == 0:
        raise ValueError(f"image must have at least one pixel, got shape {array.shape}")
    darkest, brightest = array.min(), array.max()
    # Written so that NaN fails too: every comparison with NaN is False.
    if not (darkest >= 0 and brightest <= 1):
        raise ValueError(
            f"image values must lie in 0..1 and be finite, got {darkest} to {brightest}; divide 8-bit pictures by 255"
        )
    grey = array.astype(np.float32) @ _LUMA if array.ndim == 3 else array.astype(np.float32)
    # Only differences matter, so shifting the darkest pixel to 0 changes nothing but float32 rounding, which then
    # scales with the picture's contrast instead of its brightness.
    return grey - grey.min()


def _sobel(grey: np.ndarray, border_mode: BorderMode) -> tuple[np.ndarray, np.ndarray]:
    """Un-normalised Sobel derivatives (d/dx, d/dy) with x right and y down; borders extended per `border_mode`."""
    # Written with shifted slices rather than ndimage.correlate1d: about three times faster on large pictures.
    padded = np.pad(grey, 1, mode=_NUMPY_PAD_MODE[border_mode])
    across_x = padded[:, 2:] - padded[:, :-2]  # (h + 2, w): right minus left
    across_y = padded[2:, :] - padded[:-2, :]  # (h, w + 2): below minus above
    # Sobel kernels (Sobel & Feldman): smooth 1-2-1 along the edge, difference across it.
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


def _difference_of_gaussians(
    grey: np.ndarray, narrow: np.ndarray, sigma: float, ratio: float, truncate: float, border_mode: BorderMode
) -> np.ndarray:
    """|G(sigma) - G(ratio * sigma)| of `grey`, divided by the largest value any 0..1 picture could give."""
    wide = ndimage.gaussian_filter(grey, ratio * sigma, truncate=truncate, mode=border_mode)
    scale = _dog_peak_response(sigma, ratio * sigma, truncate)
    if scale == 0.0:  # both blurs are narrower than a pixel, so they are the identity and cancel
        return np.zeros_like(grey)
    return np.abs(narrow - wide) / np.float32(scale)


def _dog_peak_response(narrow_sigma: float, wide_sigma: float, truncate: float) -> float:
    """The most |G(narrow) - G(wide)| can give for 0..1 pixels (bright where its kernel is positive)."""
    radius = int(truncate * wide_sigma + 0.5)  # scipy's kernel radius rule
    impulse = np.zeros(2 * radius + 1)
    impulse[radius] = 1.0
    # Filtering an impulse with the very function used on the picture yields exactly the 1-D kernels it applied.
    narrow_k = ndimage.gaussian_filter1d(impulse, narrow_sigma, truncate=truncate)
    wide_k = ndimage.gaussian_filter1d(impulse, wide_sigma, truncate=truncate)
    positive = negative = 0.0
    for narrow_i, wide_i in zip(narrow_k, wide_k, strict=True):  # row by row: memory O(radius), not O(radius^2)
        row = narrow_i * narrow_k - wide_i * wide_k
        positive += row[row > 0].sum()
        negative -= row[row < 0].sum()
    return max(positive, negative)


def _canny_lines(
    gradient: np.ndarray, direction: np.ndarray, low: float, high: float, tie_tolerance: float, connectivity: int
) -> np.ndarray:
    """Thin edge lines: 1.0 on kept pixels, 0 elsewhere."""
    if not gradient.any():
        return np.zeros_like(gradient)
    ridge = _non_maximum_suppression(gradient, direction, tie_tolerance)
    strongest = gradient.max()
    candidate = ridge & (gradient > low * strongest)
    seed = ridge & (gradient > high * strongest)  # a subset of `candidate`, since low < high

    # Hysteresis: keep every connected group of candidate pixels that contains at least one seed.
    structure = ndimage.generate_binary_structure(2, 1 if connectivity == 4 else 2)
    groups, count = ndimage.label(candidate, structure=structure)
    has_seed = np.zeros(count + 1, dtype=bool)
    has_seed[groups[seed]] = True  # seeds are never background, so label 0 stays False
    return has_seed[groups].astype(np.float32)


def _non_maximum_suppression(gradient: np.ndarray, direction: np.ndarray, tie_tolerance: float) -> np.ndarray:
    """Mask of pixels not weaker than their two neighbours along the gradient, i.e. the crest across an edge."""
    height, width = gradient.shape
    padded = np.pad(gradient, 1)  # beyond the picture there is no gradient, whatever the blur's border mode
    # The gradient axis rounded to the nearest of four lines through the pixel (0, 45, 90, 135 degrees). The angle is
    # taken modulo pi because a direction and its opposite name the same pair of neighbours.
    sector = np.floor(np.mod(direction, np.pi) / (np.pi / 4) + 0.5).astype(np.int8) % 4
    # (dy, dx) of the "next" neighbour for each sector; the "previous" one is the opposite offset and is always the
    # earlier of the two in reading order (upper row first, then left), which decides ties below.
    steps = ((0, 1), (1, 1), (1, 0), (1, -1))
    keep = np.zeros(gradient.shape, dtype=bool)
    for index, (dy, dx) in enumerate(steps):
        after = padded[1 + dy : 1 + dy + height, 1 + dx : 1 + dx + width]
        before = padded[1 - dy : 1 - dy + height, 1 - dx : 1 - dx + width]
        # A step between two pixels makes both equally strong. The earlier one wins such a tie, and "equally" allows for
        # rounding so that the winner does not flip when the picture is shifted in brightness or transposed.
        keep |= (
            (sector == index) & (gradient >= after * (1 - tie_tolerance)) & (gradient > before * (1 + tie_tolerance))
        )
    return keep
