"""The `equalize` effect: contrast-limited adaptive histogram equalisation of the picture's brightness."""

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect

# constant: the clipped excess is redistributed until less than this share of a tile's pixels is left over (a pure
# stopping guard: the loop shrinks the excess geometrically, so this ends it long before float64 rounding matters)
_EXCESS_TOLERANCE = 1e-9


class EqualizeParams(EffectParams):
    """Contrast-limited adaptive histogram equalisation (CLAHE) of the brightness, colours kept."""

    tiles: int = param(
        8,
        unit="count",
        min=1,
        max=64,
        doc="Tiles per side: the picture is cut into tiles x tiles regions, each equalised by its own histogram and "
        "blended smoothly into its neighbours. 1 = one histogram for the whole picture (classic equalisation); "
        "raising it adapts to smaller regions and brings out local contrast. A picture with fewer pixel rows or "
        "columns than tiles gets one tile per row or column.",
    )
    limit: float = param(
        3.0,
        unit="ratio",
        min=1,
        max=256,
        doc="Clip limit, as a multiple of the height every bin would have if a tile's histogram were flat: no bin "
        "may be taller, so no tone range is stretched more than this many times. 1 = the histogram is made flat, "
        "which leaves tones evenly spread (a gentle lift of the darkest bin by one step); raising it allows more "
        "contrast and amplifies noise in flat areas. A limit of `bins` or more never clips (classic equalisation).",
    )
    bins: int = param(
        256,
        unit="count",
        min=2,
        max=256,
        doc="Histogram bins over the brightness range 0..1: fewer bins give a coarser mapping and fewer distinct "
        "brightnesses in the result (2 = every tile ends up with two brightnesses), 256 matches 8-bit pictures.",
    )


@effect(category="colour", params=EqualizeParams)
def equalize(rgb: np.ndarray, params: EqualizeParams) -> np.ndarray:
    """Equalise the brightness histogram tile by tile (CLAHE, Zuiderveld 1994), keeping each pixel's colour.

    The brightness is the value V = max(R, G, B) of the sRGB-encoded picture (the V of HSV). Every pixel's red, green
    and blue are multiplied by new V / old V, so hue and HSV saturation are unchanged and no channel can leave 0..1;
    black stays black. Each of the `tiles` x `tiles` regions gets a histogram of V in `bins` bins, clipped so no bin
    is taller than `limit` times the flat height (the clipped counts are spread evenly over all bins, repeatedly,
    until none exceeds it), and its mapping is the cumulative histogram: a pixel in bin k goes to the share of the
    tile's pixels in bins 0..k. A pixel takes the bilinear blend of the mappings of the four tiles whose centres
    surround it (the nearest tiles at the borders), so there are no seams. With one tile and a limit of `bins` or more
    this is classic histogram equalisation.
    """
    height, width = rgb.shape[:2]
    rows, columns = min(params.tiles, height), min(params.tiles, width)  # a tile needs at least one pixel
    value = rgb.max(axis=-1)
    bin_of = np.minimum((value * params.bins).astype(np.intp), params.bins - 1)

    # Tile of each pixel row and column, then one histogram per tile from a single count.
    tile_row = np.arange(height) * rows // height
    tile_column = np.arange(width) * columns // width
    tile_of = tile_row[:, None] * columns + tile_column[None, :]
    counts = np.bincount((tile_of * params.bins + bin_of).ravel(), minlength=rows * columns * params.bins)
    histogram = counts.reshape(rows * columns, params.bins).astype(np.float64)
    pixels = histogram.sum(axis=1, keepdims=True)

    # Clip every bin to `limit` x the flat height and give the excess to all bins alike, until nothing is above it.
    ceiling = params.limit * pixels / params.bins
    while True:
        excess = np.maximum(histogram - ceiling, 0).sum(axis=1, keepdims=True)
        if (excess <= _EXCESS_TOLERANCE * pixels).all():
            break
        histogram = np.minimum(histogram, ceiling) + excess / params.bins
    mapping = (np.cumsum(histogram, axis=1) / pixels).reshape(rows, columns, params.bins).astype(np.float32)

    # Blend the mappings of the four tiles around each pixel, weighted by distance to their centres.
    row_low, row_weight = _neighbours(height, rows)
    column_low, column_weight = _neighbours(width, columns)
    row_high, column_high = np.minimum(row_low + 1, rows - 1), np.minimum(column_low + 1, columns - 1)
    top = _blend(mapping, row_low[:, None], column_low[None, :], column_high[None, :], column_weight, bin_of)
    bottom = _blend(mapping, row_high[:, None], column_low[None, :], column_high[None, :], column_weight, bin_of)
    new_value = top + (bottom - top) * row_weight[:, None]

    gain = np.divide(new_value, value, out=np.zeros_like(value), where=value > 0)
    return (rgb * gain[..., None]).astype(np.float32)


def _neighbours(length: int, tiles: int) -> tuple[np.ndarray, np.ndarray]:
    """For each pixel index along one axis of `length` pixels cut into `tiles` tiles: the tile whose centre lies at or
    before it (the first tile before the first centre) and the share, 0..1, of the way to the next tile's centre."""
    position = (np.arange(length) + 0.5) * tiles / length - 0.5  # in tile widths from the first tile's centre
    low = np.clip(np.floor(position), 0, tiles - 1).astype(np.intp)
    weight = np.clip(position - low, 0.0, 1.0).astype(np.float32)
    return low, weight


def _blend(
    mapping: np.ndarray,
    row: np.ndarray,
    column_low: np.ndarray,
    column_high: np.ndarray,
    column_weight: np.ndarray,
    bin_of: np.ndarray,
) -> np.ndarray:
    """The mapping of tile row `row` read at each pixel's bin, blended between the two tile columns around it."""
    left = mapping[row, column_low, bin_of]
    right = mapping[row, column_high, bin_of]
    return left + (right - left) * column_weight[None, :]
