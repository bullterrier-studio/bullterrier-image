"""The `byte_corrupt` effect: damaged coefficients in the picture's JPEG, as a corrupted file shows them."""

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.jpeg import BLOCK, decode, encode


class ByteCorruptParams(EffectParams):
    """The settings of `byte_corrupt`: the codec's quality, how many hits, and the random seed."""

    quality: int = param(
        20,
        unit="count",
        min=1,
        max=100,
        doc="The JPEG quality the picture is stored at before it is damaged, 1 to 100, as in `jpeg_crush`: the "
        "quantisation step of every coefficient, so a lower quality makes each hit a larger jump in the picture.",
    )
    hits: int = param(
        40,
        unit="count",
        min=0,
        max=5000,
        doc="How many coefficients are damaged, 0 to 5000, each at a random place in a random 8 x 8 block of the "
        "brightness or one of the two colour planes; 0 gives the plain JPEG of `jpeg_crush`. Raising it spreads more "
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


@effect(category="glitch", params=ByteCorruptParams)
def byte_corrupt(rgb: np.ndarray, params: ByteCorruptParams) -> np.ndarray:
    """Damage `hits` quantised JPEG coefficients of the picture at random, then decode it.

    The picture goes through the JPEG steps of `jpeg_crush` up to the quantised coefficients. Each hit then picks a
    block at random from all those of the brightness and the two colour planes, and a coefficient in it (the DC
    value with chance `dc_share`, otherwise one of the 63 others), and adds or subtracts between 1 and `magnitude`
    quantisation steps. A hit on a DC value also moves the DC value of every later block of its row in that plane,
    as a damaged difference does in the file. With no hits the result is exactly that of `jpeg_crush` at `quality`.
    """
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
