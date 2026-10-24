"""The `jpeg_crush` effect: the picture put through JPEG's lossy steps at a low quality, as a crushed photo."""

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.jpeg import decode, encode


class JpegCrushParams(EffectParams):
    """The settings of `jpeg_crush`: the quality of the compression."""

    quality: int = param(
        6,
        unit="count",
        min=1,
        max=100,
        doc="The JPEG quality, 1 to 100, on the scale JPEG tools use (the Independent JPEG Group's): 100 keeps almost "
        "everything, about 75 is a normal photo, about 6 shows 8 px blocks, ringing round edges and colour bleed, 1 "
        "is a few flat blocks. Lowering it coarsens the quantisation of every 8 x 8 block.",
    )


@effect(category="glitch", params=JpegCrushParams)
def jpeg_crush(rgb: np.ndarray, params: JpegCrushParams) -> np.ndarray:
    """Compress the picture with a JPEG codec written as maths, at `quality`, and return what it decodes to.

    The steps are the standard's (ITU-T T.81 with JFIF): sRGB to YCbCr, the two colour planes halved in each direction
    (4:2:0), 8 x 8 blocks through the DCT, every coefficient rounded to a multiple of its quantisation step, then all
    of it undone. There is no file and no byte: the result does not depend on any image library, so it is the same on
    every machine. A picture whose sides are not multiples of 16 is extended by repeating its last row and column
    and cropped back.
    """
    return decode(encode(rgb, quality=params.quality))
