"""Edge-aware smoothing by the guided filter (He, Sun and Tang 2010), at a cost that does not grow with the radius.

Promises: every function takes 2-D float arrays (height, width) and returns a new float32 array of the same shape;
the inputs are never modified. A window is a square of 2 x radius + 1 px, cut off at the picture's edge.
"""

import numpy as np


def guided_filter(guide: np.ndarray, source: np.ndarray, *, radius: int, epsilon: float) -> np.ndarray:
    """`source` smoothed so that it keeps the edges of `guide`: where the guide is flat it is averaged over the
    window, where the guide has an edge higher than sqrt(`epsilon`) the source keeps its own side of it.

    The filter fits, in every window, the line source ~ a * guide + b with a = cov(guide, source) / (var(guide) +
    `epsilon`) and b = mean(source) - a * mean(guide) (He, Sun and Tang 2010, section 3), and each pixel takes the
    average of the lines of all the windows that hold it, evaluated at its own guide value. So the result is
    - the source itself for `radius` 0, and a constant source, whatever the guide, stays that constant;
    - a plain box average of the source (applied twice) for `epsilon` far above the guide's variance;
    - the source again where it is an affine function of the guide and `epsilon` is far below the guide's variance.
    It is not clipped: it can leave the range of the source by a little near strong edges, so callers that need
    a range clip it. `guide` and `source` have one shape (a picture's luminance, say); `epsilon` is in the units of
    the guide squared and must be above 0 (0 divides by the variance of flat windows). The work is proportional to
    the number of pixels, not to the window (box means by cumulative sums, in float64).
    """
    mean_guide = _box_mean(guide, radius)
    mean_source = _box_mean(source, radius)
    covariance = _box_mean(guide * source, radius) - mean_guide * mean_source
    variance = _box_mean(guide * guide, radius) - mean_guide * mean_guide
    slope = covariance / (variance + epsilon)
    del covariance, variance  # float64 pictures: free them before the next ones
    offset = mean_source - slope * mean_guide
    del mean_source, mean_guide
    return (_box_mean(slope, radius) * guide + _box_mean(offset, radius)).astype(np.float32)


def _box_mean(values: np.ndarray, radius: int) -> np.ndarray:
    """The mean of `values` over the (2 x `radius` + 1) px square around each pixel, cut off at the picture's edge."""
    return _window_mean(_window_mean(np.asarray(values, np.float64), radius, axis=0), radius, axis=1)


def _window_mean(values: np.ndarray, radius: int, *, axis: int) -> np.ndarray:
    """The mean of `values` over the 2 x `radius` + 1 values around each one along `axis`, cut off at the ends."""
    total = np.cumsum(values, axis=axis)  # total[k] is the sum of the values up to and including k
    index = np.arange(values.shape[axis])
    last, first = np.minimum(index + radius, index.size - 1), np.maximum(index - radius, 0)
    before = np.where(
        np.expand_dims(first > 0, 1 - axis), total.take(first - 1, axis=axis), 0.0
    )  # the sum of the values before the window
    return (total.take(last, axis=axis) - before) / np.expand_dims(last - first + 1, 1 - axis)
