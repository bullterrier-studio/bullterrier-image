"""JPEG's lossy core written as maths on a float picture: no file, no bytes, no entropy coding.

`encode` takes a picture to what a JPEG stores before it is packed into bytes, the quantised DCT coefficients of
JFIF YCbCr (ITU-T T.81, T.871); `decode` takes such coefficients back to a picture. Damage done to the coefficients
between the two is what a damaged JPEG shows.

The steps, as the standard has them: sRGB to JFIF YCbCr; the two chroma planes halved in each direction by averaging
2 x 2 patches (4:2:0); every plane shifted by 128 and cut into 8 x 8 blocks; each block through the orthonormal 2-D
DCT; each coefficient divided by its quantisation step and rounded. A picture whose sides do not fit is first
extended to a multiple of 16 by repeating its last row and column, and cropped back by `decode`.
"""

from dataclasses import dataclass

import numpy as np

from bullterrier_image.ops.colorspace import rgb_to_ycbcr, ycbcr_to_rgb

BLOCK = 8  # constant: JPEG transforms blocks of 8 x 8 samples (ITU-T T.81)
_SUBSAMPLING = 2  # constant: 4:2:0 keeps one chroma sample for every 2 x 2 luma samples (ITU-T T.871)
_LEVEL = 128  # constant: samples are shifted by half the 8-bit range before the DCT (ITU-T T.81)
_LEVELS = 255  # constant: the 8-bit range the standard's tables and quantiser are written for

# constant: the example quantisation table of ITU-T T.81 Annex K, table K.1 (luma)
_LUMA_TABLE = np.array(
    [
        [16, 11, 10, 16, 24, 40, 51, 61],
        [12, 12, 14, 19, 26, 58, 60, 55],
        [14, 13, 16, 24, 40, 57, 69, 56],
        [14, 17, 22, 29, 51, 87, 80, 62],
        [18, 22, 37, 56, 68, 109, 103, 77],
        [24, 35, 55, 64, 81, 104, 113, 92],
        [49, 64, 78, 87, 103, 121, 120, 101],
        [72, 92, 95, 98, 112, 100, 103, 99],
    ]
)
# constant: the example quantisation table of ITU-T T.81 Annex K, table K.2 (chroma)
_CHROMA_TABLE = np.array(
    [
        [17, 18, 24, 47, 99, 99, 99, 99],
        [18, 21, 26, 66, 99, 99, 99, 99],
        [24, 26, 56, 99, 99, 99, 99, 99],
        [47, 66, 99, 99, 99, 99, 99, 99],
        [99, 99, 99, 99, 99, 99, 99, 99],
        [99, 99, 99, 99, 99, 99, 99, 99],
        [99, 99, 99, 99, 99, 99, 99, 99],
        [99, 99, 99, 99, 99, 99, 99, 99],
    ]
)

# The orthonormal 1-D DCT-II matrix of size BLOCK (ITU-T T.81 A.3.3): row k holds the k-th cosine.
_ANGLES = np.pi * np.outer(np.arange(BLOCK), 2 * np.arange(BLOCK) + 1) / (2 * BLOCK)
_DCT = (np.sqrt(2 / BLOCK) * np.cos(_ANGLES)).astype(np.float32)
_DCT[0] *= np.sqrt(1 / 2)  # the first row, the mean, is scaled by 1 / sqrt(2) to stay orthonormal


@dataclass(frozen=True)
class Coefficients:
    """The quantised coefficients of a picture, as a JPEG stores them before packing them into bytes.

    `planes` are the Y, Cb and Cr planes, each an int32 array (block rows, block columns, 8, 8) whose blocks hold the
    quantised DCT coefficients (entry [0, 0], the DC value, is the block's mean); the chroma planes have a quarter of
    the luma plane's blocks. `steps` are the quantisation steps the planes were divided by (8 x 8 each, as
    (luma, chroma, chroma)). `shape` is the (height, width) of the picture they came from.
    """

    planes: tuple[np.ndarray, np.ndarray, np.ndarray]
    steps: tuple[np.ndarray, np.ndarray, np.ndarray]
    shape: tuple[int, int]


def encode(rgb: np.ndarray, *, quality: int) -> Coefficients:
    """The quantised DCT coefficients of the picture `rgb` (height, width, 3) at JPEG `quality` 1 to 100.

    The quantisation steps are the Annex K tables scaled by the Independent JPEG Group's rule: a scale of 5000 /
    quality below 50, 200 - 2 * quality from 50 up; each step is (table * scale + 50) // 100, held to 1..255. Quality
    50 is the tables themselves, 100 a step of 1 everywhere.
    """
    height, width = rgb.shape[:2]
    unit = _SUBSAMPLING * BLOCK  # the chroma planes must hold whole blocks, so the luma plane holds twice as much
    rows = np.minimum(np.arange(-(-height // unit) * unit), height - 1)  # repeats the last row (and column)
    columns = np.minimum(np.arange(-(-width // unit) * unit), width - 1)
    samples = rgb_to_ycbcr(rgb[rows][:, columns]) * _LEVELS - _LEVEL
    luma, cb, cr = np.moveaxis(samples, -1, 0)
    # constant: the IJG quality rule, which scales the Annex K tables
    scale = 5000 // quality if quality < 50 else 200 - 2 * quality
    # constant: the IJG rule rounds each scaled entry and holds it to 1..255
    luma_step, chroma_step = (np.clip((table * scale + 50) // 100, 1, 255) for table in (_LUMA_TABLE, _CHROMA_TABLE))
    steps = (luma_step, chroma_step, chroma_step)
    planes = (luma, _halve(cb), _halve(cr))
    quantised = tuple(
        np.rint(_DCT @ _blocks(plane) @ _DCT.T / step.astype(np.float32)).astype(np.int32)
        for plane, step in zip(planes, steps, strict=True)
    )
    return Coefficients(quantised, steps, (height, width))


def decode(coefficients: Coefficients) -> np.ndarray:
    """The picture (height, width, 3), float32 in 0..1, that `coefficients` describe.

    Each coefficient is multiplied by its step, each block goes through the inverse DCT, the chroma planes are doubled
    in each direction by repeating samples, the planes are shifted back by 128, converted from YCbCr to sRGB and
    cropped to the original size. The result is clamped to 0..1 here, the one place it is done.
    """
    luma, cb, cr = (
        _unblocks(_DCT.T @ (plane * step).astype(np.float32) @ _DCT)
        for plane, step in zip(coefficients.planes, coefficients.steps, strict=True)
    )
    chroma = [plane.repeat(_SUBSAMPLING, axis=0).repeat(_SUBSAMPLING, axis=1) for plane in (cb, cr)]
    height, width = coefficients.shape
    samples = np.stack([luma, *chroma], axis=-1)[:height, :width]
    return np.clip(ycbcr_to_rgb((samples + _LEVEL) / _LEVELS), 0, 1)


def _blocks(plane: np.ndarray) -> np.ndarray:
    """`plane` (height, width), both multiples of 8, as (block rows, block columns, 8, 8)."""
    height, width = plane.shape
    return plane.reshape(height // BLOCK, BLOCK, width // BLOCK, BLOCK).swapaxes(1, 2)


def _unblocks(blocks: np.ndarray) -> np.ndarray:
    """The inverse of `_blocks`."""
    rows, columns = blocks.shape[:2]
    return blocks.swapaxes(1, 2).reshape(rows * BLOCK, columns * BLOCK)


def _halve(plane: np.ndarray) -> np.ndarray:
    """`plane` with every 2 x 2 patch of samples replaced by its mean: half the height and half the width."""
    height, width = plane.shape
    return plane.reshape(height // _SUBSAMPLING, _SUBSAMPLING, width // _SUBSAMPLING, _SUBSAMPLING).mean(axis=(1, 3))
