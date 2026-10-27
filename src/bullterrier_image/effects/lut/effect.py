"""The `lut` effect: a colour grade from a LUT file (a film look), a `.cube` 3D lookup table."""

import numpy as np

from bullterrier_image.core.params import EffectParams, Lut, param
from bullterrier_image.core.registry import effect


class LutParams(EffectParams):
    """The lookup table and how much of its grade shows."""

    file: Lut = param(
        Lut.identity(),
        doc="The 3D colour lookup table: the path of a `.cube` file (Adobe/Resolve format; 1D tables are refused) in a "
        "recipe file (relative to the file) or in `fx`, or, in Python, a Lut or an (n, n, n, 3) array. The default "
        "table changes nothing. Colours between the table's entries are interpolated trilinearly.",
    )
    amount: float = param(
        1.0,
        unit="fraction",
        min=0,
        max=1,
        drivable=True,
        doc="How much of the table's grade shows: 0 leaves the picture unchanged, 0.5 is the midway between the "
        "picture and the graded one, 1 is the table as written.",
    )


@effect(category="colour", params=LutParams)
def lut(rgb: np.ndarray, params: LutParams) -> np.ndarray:
    """Colour grade from a LUT file (a film look): apply a `.cube` 3D lookup table, mixed in by `amount`.

    Each pixel's colour is looked up in the table, interpolating trilinearly between the eight nearest entries; values
    outside the table's domain take its edge entry, and the result is kept in 0..1.
    """
    table = params.file.table
    low, high = np.asarray(params.file.domain_min, np.float32), np.asarray(params.file.domain_max, np.float32)
    position = np.clip((rgb - low) / (high - low), 0.0, 1.0) * (table.shape[0] - 1)
    first = np.minimum(position.astype(np.intp), table.shape[0] - 2)  # the lower entry of the cell, per channel
    fraction = (position - first).astype(np.float32)
    graded = np.zeros_like(rgb)
    for corner in np.ndindex(2, 2, 2):  # constant: a cell has two corners on each of its three axes
        weight = np.prod(np.where(np.array(corner, bool), fraction, 1 - fraction), axis=-1, keepdims=True)
        red, green, blue = np.moveaxis(first + np.array(corner), -1, 0)
        graded += weight * table[red, green, blue]
    return np.clip(rgb + params.amount * (graded - rgb), 0.0, 1.0).astype(np.float32)
