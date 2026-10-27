"""The `jpeg_crush` effect: the picture put through JPEG's lossy steps at a low quality, optionally damaged."""

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.jpeg import BLOCK, decode, encode
from bullterrier_image.ops.resample import box_resize, nearest_resize


class JpegCrushParams(EffectParams):
    """The settings of `jpeg_crush`: the quality of the compression, and how many coefficients are then damaged."""

    quality: int = param(
        6,
        unit="count",
        min=1,
        max=100,
        doc="The JPEG quality, 1 to 100, on the scale JPEG tools use (the Independent JPEG Group's): 100 keeps almost "
        "everything, about 75 is a normal photo, about 6 shows blocks 8 px wide, ringing round edges and colour "
        "bleed, 1 is a few flat blocks. Lowering it coarsens the quantisation of every 8 x 8 block, so each hit is "
        "also a larger jump in the picture.",
    )
    hits: int = param(
        0,
        unit="count",
        min=0,
        max=5000,
        doc="How many coefficients are damaged after the compression, 0 to 5000, each at a random place in a random "
        "8 x 8 block of the brightness or one of the two colour planes, as a corrupted file shows. 0 leaves the "
        "plain crushed picture, and then `seed`, `dc_share` and `magnitude` do nothing. Raising it spreads more "
        "damage over the picture.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed for where the hits land and how large they are: the same seed always gives the same damage, "
        "any other a different one.",
    )
    dc_share: float = param(
        0.3,
        unit="fraction",
        min=0,
        max=1,
        doc="The share of hits, 0 to 1, that land on a block's DC value (its average) rather than on one of its 63 "
        "detail coefficients. A DC hit shifts the block and, as JPEG stores each average as a difference from the "
        "block before, every later block of its row: a streak to the right edge. A detail hit only marks its own "
        "block with a pattern.",
        advanced=True,
    )
    magnitude: int = param(
        8,
        unit="count",
        min=1,
        max=100,
        doc="The largest change a hit makes to a coefficient, 1 to 100 quantisation steps: each hit adds or subtracts "
        "a whole number of steps between 1 and this. Raising it makes the damage louder.",
        advanced=True,
    )


@effect(category="glitch", params=JpegCrushParams)
def jpeg_crush(
    rgb: np.ndarray,
    params: JpegCrushParams,
    *,
    size_ratio: float = 1.0,  # constant: a call outside a recipe is at its design size
) -> np.ndarray:
    """Blocky JPEG artefacts, or a corrupted file: compress the picture with a JPEG codec written as maths at `quality`,
    damage `hits` coefficients, and decode it.

    The steps are the standard's (ITU-T T.81 with JFIF): sRGB to YCbCr, the two colour planes halved in each direction
    (4:2:0), 8 x 8 blocks through the DCT, every coefficient rounded to a multiple of its quantisation step, then all
    of it undone. There is no file and no byte: the result does not depend on any image library, so it is alike on
    every platform. A picture whose sides are not multiples of 16 is extended by repeating its last row and column
    and cropped back.

    Each hit picks a block at random from all those of the brightness and the two colour planes, and a coefficient in
    it (the DC value with chance `dc_share`, otherwise one of the 63 others), and adds or subtracts between 1 and
    `magnitude` quantisation steps before the decoding. A hit on a DC value also moves the DC value of every later
    block of its row in that plane, as a damaged difference does in the file.

    The blocks are 8 design px, so the damage looks the same at any size while the fine detail stays. `size_ratio`
    (injected: working px per design px, 1 outside a recipe) shrinks the picture to its design size by averaging,
    crushes and damages that, and adds the damage (crushed minus shrunk) back to the full picture, each design pixel's
    damage repeated over the working pixels it covers so block edges stay hard.
    """
    height, width = rgb.shape[:2]
    design = box_resize(rgb, max(1, round(height / size_ratio)), max(1, round(width / size_ratio)))
    crushed = _crush(design, params)
    # picture + damage, written so that at a ratio of 1 the picture cancels exactly and the plain crush is returned
    plain = nearest_resize(design, height, width)
    return np.clip(nearest_resize(crushed, height, width) + (rgb - plain), 0, 1)


def _crush(rgb: np.ndarray, params: JpegCrushParams) -> np.ndarray:
    """The JPEG round trip of `rgb` at `params.quality`, with `params.hits` coefficients damaged."""
    coded = encode(rgb, quality=params.quality)
    planes = coded.planes  # arrays `encode` just made, so they are ours to damage in place
    rng = np.random.default_rng(params.seed)
    picks = rng.integers(sum(plane.shape[0] * plane.shape[1] for plane in planes), size=params.hits)
    on_dc = rng.random(params.hits) < params.dc_share
    detail = 1 + rng.integers(BLOCK * BLOCK - 1, size=params.hits)  # which of the 63 others, in row-major order
    steps = (1 + rng.integers(params.magnitude, size=params.hits)) * np.where(rng.random(params.hits) < 0.5, -1, 1)
    for pick, dc, coefficient, change in zip(picks, on_dc, detail, steps, strict=True):
        for plane in planes:  # the pick numbers the blocks of the planes one after the other
            if pick < plane.shape[0] * plane.shape[1]:
                break
            pick -= plane.shape[0] * plane.shape[1]
        row, column = divmod(int(pick), plane.shape[1])
        if dc:
            plane[row, column:, 0, 0] += change  # the later blocks' values are differences from this one
        else:
            plane[row, column][divmod(int(coefficient), BLOCK)] += change
    return decode(coded)
