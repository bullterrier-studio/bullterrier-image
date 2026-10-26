"""Dithering: choosing a palette colour for every pixel so that, seen from afar, the pixels average to the picture.

Error diffusion (`diffuse`) passes each pixel's error on to the pixels after it; ordered dithering (`ordered`) shifts
each pixel by a repeating pattern (`threshold_tile`) before choosing. Both return the index of the chosen palette colour
per pixel; the callers paint them.
"""

from functools import lru_cache
from typing import Literal

import numpy as np

from bullterrier_image.ops.cluster import nearest
from bullterrier_image.ops.colorspace import rgb_to_oklab

# Where each method sends a pixel's error: (dx, dy, weight) for the pixels it reaches, and the weights' divisor.
# Every tap lies after the pixel in reading order, and `dx + 2 * dy` is at least 1 for each, which `diffuse` relies on.
# constant: Floyd and Steinberg (1976): 7/16 right, 3/16 below left, 5/16 below, 1/16 below right
_FLOYD_STEINBERG = (((1, 0, 7), (-1, 1, 3), (0, 1, 5), (1, 1, 1)), 16)
# constant: Atkinson (Apple MacPaint): 1/8 to each of six pixels, so a quarter of the error is dropped
_ATKINSON = (((1, 0, 1), (2, 0, 1), (-1, 1, 1), (0, 1, 1), (1, 1, 1), (0, 2, 1)), 8)
_KERNELS = {"floyd_steinberg": _FLOYD_STEINBERG, "atkinson": _ATKINSON}

# constant: Bayer (1973): the 8 x 8 matrix of the recursive construction, 64 levels
_BAYER_SIZE = 8
# constant: Ulichney (1993), void-and-cluster: the tile's side, so 4096 levels
_BLUE_SIZE = 64
# constant: Ulichney (1993): the standard deviation in px of the Gaussian that measures how crowded a spot is
_BLUE_SIGMA = 1.5
# constant: Ulichney (1993): the share of the points the first pattern starts with
_BLUE_START = 0.1
# constant: the mix an ordered dither picks from, as many candidate colours as the common form of Knoll's algorithm
# uses; more would resolve the tile's levels finer and cost more
_CANDIDATES = 16
# constant: pixels dithered at once by `ordered`; bounds the working memory and never changes the result
_BAND_PIXELS = 2**16
# constant: any seed gives a good tile; this one is fixed, so the pattern is the same everywhere
_BLUE_SEED = 0


def diffuse(
    values: np.ndarray, palette: np.ndarray, *, method: Literal["floyd_steinberg", "atkinson"], amount: float
) -> np.ndarray:
    """The palette colour chosen for each pixel of `values` by error diffusion: (height, width) indices into `palette`.

    `values` is a (height, width, 3) picture, `palette` (count, 3) colours, both on the same 0..1 scale. Pixels are
    taken in reading order; each is given the palette colour closest to its value (a tie goes to the one listed
    first), and `amount` (0 to 1) of the difference between the two is added to the pixels `method` names after it.
    `values` is not modified.

    The result is the one the plain loop over pixels gives, computed a diagonal at a time: every tap reaches
    `dx + 2 * dy >= 1` further along the line `x + 2 y = t`, so the pixels with one `t` do not depend on each other and
    are chosen together, in about `width + 2 height` steps instead of one per pixel.
    """
    taps, divisor = _KERNELS[method]
    height, width = values.shape[:2]
    sideways = max(abs(dx) for dx, _, _ in taps)
    below = max(dy for _, dy, _ in taps)
    # the picture, with room beside and below it for the errors that fall off its edge
    work = np.zeros((height + below, width + 2 * sideways, 3), np.float64)
    work[:height, sideways : sideways + width] = values
    chosen = np.zeros((height, width), np.intp)
    for line in range(width + 2 * height - 2):
        rows = np.arange(max(0, -(-(line - width + 1) // 2)), min(height - 1, line // 2) + 1)
        columns = line - 2 * rows
        current = work[rows, columns + sideways]
        picked = nearest(current, palette)
        chosen[rows, columns] = picked
        error = (current - palette[picked]) * (amount / divisor)
        for dx, dy, weight in taps:
            work[rows + dy, columns + sideways + dx] += error * weight
    return chosen


@lru_cache
def _bayer() -> np.ndarray:
    """The ranks 0..63 of the 8 x 8 Bayer matrix: from [[0]], each doubling is [[4M, 4M + 2], [4M + 3, 4M + 1]]."""
    ranks = np.array([[0]], np.intp)
    while ranks.shape[0] < _BAYER_SIZE:
        # constant: Bayer (1973), the recursive construction
        ranks = np.block([[4 * ranks, 4 * ranks + 2], [4 * ranks + 3, 4 * ranks + 1]])
    return ranks


@lru_cache
def _blue_noise() -> np.ndarray:
    """Ranks 0..4095 of the blue-noise tile by void-and-cluster (Ulichney, 1993): the points of every prefix of the
    ranking are spread as evenly as the tile allows, wrapping around its edges."""
    size = _BLUE_SIZE
    total = size * size
    wrapped = np.minimum(np.arange(size), size - np.arange(size))
    crowd = np.exp(-(wrapped[:, None] ** 2 + wrapped[None, :] ** 2) / (2 * _BLUE_SIGMA**2))
    # constant: two copies each way, so a window of the tile's size onto them places the Gaussian anywhere, wrapping
    crowd = np.tile(crowd, (2, 2))

    def around(row: int, column: int) -> np.ndarray:
        """How much a point at (row, column) crowds every spot of the tile."""
        return crowd[size - row : 2 * size - row, size - column : 2 * size - column]

    def energy_of(points: np.ndarray) -> np.ndarray:
        crowded = np.zeros((size, size))
        for row, column in np.argwhere(points):
            crowded += around(row, column)
        return crowded

    def tightest(points: np.ndarray, crowded: np.ndarray) -> tuple[int, ...]:
        return tuple(map(int, np.unravel_index(np.where(points, crowded, -np.inf).argmax(), points.shape)))

    def largest_void(points: np.ndarray, crowded: np.ndarray) -> tuple[int, ...]:
        return tuple(map(int, np.unravel_index(np.where(points, np.inf, crowded).argmin(), points.shape)))

    rng = np.random.default_rng(_BLUE_SEED)
    start = np.zeros(total, bool)
    start[rng.choice(total, int(total * _BLUE_START), replace=False)] = True
    points = start.reshape(size, size)
    crowded = energy_of(points)
    while True:  # move the tightest cluster's point to the largest void until that is where it already is
        cluster = tightest(points, crowded)
        points[cluster], crowded = False, crowded - around(*cluster)
        void = largest_void(points, crowded)
        points[void], crowded = True, crowded + around(*void)
        if void == cluster:
            break
    ranks = np.zeros((size, size), np.intp)
    ones = int(points.sum())
    start_points, start_crowded = points.copy(), crowded.copy()
    for rank in range(ones - 1, -1, -1):  # take the points away, tightest cluster first, counting down
        cluster = tightest(points, crowded)
        ranks[cluster] = rank
        points[cluster], crowded = False, crowded - around(*cluster)
    points, crowded = start_points, start_crowded
    for rank in range(ones, total // 2):  # add points into the largest voids, counting up to half the tile
        void = largest_void(points, crowded)
        ranks[void] = rank
        points[void], crowded = True, crowded + around(*void)
    crowded = energy_of(~points)  # for the rest, the points are the empty spots: the tightest cluster of them is next
    for rank in range(total // 2, total):
        cluster = tightest(~points, crowded)
        ranks[cluster] = rank
        points[cluster], crowded = True, crowded - around(*cluster)
    return ranks


def threshold_tile(method: Literal["bayer", "blue_noise"]) -> np.ndarray:
    """The repeating pattern of an ordered dither: a read-only float32 square whose values are the levels
    (rank + 0.5) / count, each once, in 0..1.

    'bayer' is the 8 x 8 matrix of Bayer's recursive construction. 'blue_noise' is a 64 x 64 void-and-cluster tile: the
    points below any level are spread evenly, so there are no clumps or lines.
    """
    ranks = _bayer() if method == "bayer" else _blue_noise()
    tile = ((ranks + 0.5) / ranks.size).astype(np.float32)
    tile.flags.writeable = False
    return tile


def ordered(values: np.ndarray, palette: np.ndarray, tile: np.ndarray, *, amount: float) -> np.ndarray:
    """The palette colour chosen for each pixel of `values` by ordered dithering: (height, width) indices into
    `palette`, for any palette (not only one that runs from dark to light).

    `values` is a (height, width, 3) picture and `palette` (count, 3) colours, in a space whose first channel is
    brightness and where distance is the Euclidean one (Oklab). Each pixel first gets a mix that averages to its
    value: candidate colours picked one after another, each the palette colour closest to the pixel's value plus the
    error of the candidates so far (Knoll's mixing; the error is what the colours chosen for it still owe it). There
    are n candidates, n = 1 + round(15 `amount`) with `amount` from 0 to 1: 16 at the full pattern, fewer for a
    weaker one, and at `amount` 0 only the first, the palette colour nearest to the value. The candidates are put in
    order of brightness (the first channel; ties by palette order) and the `tile` (from `threshold_tile`), which
    repeats across the picture from its top left corner, picks one of them: a pixel with level T takes the candidate
    at position floor(n T). So with n = 16 a pixel mixed of four dark and twelve light candidates is dark for the
    quarter of the tile's levels below 0.25.
    """
    height, width = values.shape[:2]
    rows, columns = -(-height // tile.shape[0]), -(-width // tile.shape[1])
    levels = np.tile(tile, (rows, columns))[:height, :width]
    darkest_first = np.lexsort(
        (np.arange(len(palette)), palette[:, 0])
    )  # palette indices, darkest first, ties in order
    rank = np.empty(len(palette), np.intp)
    rank[darkest_first] = np.arange(len(palette))
    steps = 1 + round(amount * (_CANDIDATES - 1))  # how many candidates make the mix
    chosen = np.empty((height, width), np.intp)
    # Each pixel depends only on its own value and level, so it is chosen a band of rows at a time: the same result as
    # all at once, with the candidates of one band in memory instead of the whole picture's.
    band = max(1, _BAND_PIXELS // width)
    for top in range(0, height, band):
        goal = values[top : top + band].astype(np.float64)
        here = goal.shape[0]  # the rows of this band
        owed = np.zeros_like(goal)
        candidates = np.empty((steps, here, width), np.intp)
        for step in range(steps):
            picked = nearest((goal + owed).reshape(here * width, 3), palette).reshape(here, width)
            candidates[step] = rank[picked]
            owed += goal - palette[picked]
        candidates.sort(axis=0)
        position = (levels[top : top + band] * steps).astype(np.intp)[None]  # levels are below 1: at most steps - 1
        chosen[top : top + band] = darkest_first[np.take_along_axis(candidates, position, axis=0)[0]]
    return chosen


def choose(
    dots: np.ndarray,
    palette: np.ndarray,
    *,
    method: Literal["floyd_steinberg", "atkinson", "bayer", "blue_noise"],
    amount: float,
) -> np.ndarray:
    """The palette colour chosen for each dot of `dots` by `method`: (height, width) indices into `palette`.

    `dots` is a (height, width, 3) sRGB picture and `palette` (count, 3) sRGB colours, both 0..1. Colours are matched,
    and errors measured, in Oklab. `amount` (0 to 1) is how much of the pattern is used; at 0 every dot takes the
    palette colour nearest to it, whatever the method. `method` is one of the four above ('floyd_steinberg' and
    'atkinson' diffuse the error; 'bayer' and 'blue_noise' dither by a threshold pattern).
    """
    lab_dots, lab_palette = rgb_to_oklab(dots), rgb_to_oklab(palette)
    match method:
        case "bayer" | "blue_noise":
            return ordered(lab_dots, lab_palette, threshold_tile(method), amount=amount)
        case "floyd_steinberg" | "atkinson":
            return diffuse(lab_dots, lab_palette, method=method, amount=amount)
