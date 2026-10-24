"""A picture read again and again, zoomed or spun about a centre, and added up: the core of radial blurs and rays."""

from collections.abc import Sequence
from typing import Literal

import numpy as np
from scipy import ndimage

from bullterrier_image.ops.warp import SCIPY_EDGE, SPLINE_ORDER


def accumulate(
    image: np.ndarray,
    *,
    centre: tuple[float, float],
    kind: Literal["zoom", "spin"],
    offsets: Sequence[float] | np.ndarray,
    weights: Sequence[float],
    interpolation: Literal["nearest", "bilinear", "bicubic"],
    edge: Literal["reflect", "mirror", "nearest", "wrap"],
) -> np.ndarray:
    """The sum over k of `weights[k]` times `image` read again about `centre` as `offsets[k]` says; each colour channel
    on its own.

    `image` is a float32 (height, width, channels) picture and `centre` the (row, column) of the point everything
    turns about, in px (it may lie outside the picture). A pixel of the k-th reading holds the picture at
    `centre + M (pixel - centre)`: for 'zoom' M is the scale `1 + offsets[k]` (0 reads the picture as it is, -0.5 reads
    each pixel from half as far from the centre, so the picture is enlarged about it; +1 doubles the distance), for
    'spin' M turns the pixel's offset by the angle `offsets[k]` in radians, counterclockwise as seen on the picture
    (+pi/2 carries what lies to the right of the centre to above it). The centre itself never moves. `interpolation`
    is how a point between pixels is read ('nearest' pixel, 'bilinear', 'bicubic' spline) and `edge` what lies beyond
    the picture's edge ('reflect', 'mirror', 'nearest', 'wrap', as in `bullterrier_image.ops.blur`).

    `offsets` and `weights` have one entry each; the weights are not normalised or
    clipped, so the result can leave 0..1 and the caller scales it. Returns a new float32 array of the input's shape,
    black when there are no readings; the input is left alone.
    """
    centre_point = np.asarray(centre, dtype=np.float64)
    planes = [np.ascontiguousarray(image[..., channel]) for channel in range(image.shape[2])]
    totals = [np.zeros_like(plane) for plane in planes]
    for offset, weight in zip(offsets, weights, strict=True):
        if kind == "zoom":
            # the sample of a pixel p is centre + scale (p - centre): the picture scaled about the centre
            matrix = np.full(2, 1.0 + offset)  # a diagonal matrix: scipy's fast path
            shift = centre_point - matrix * centre_point
        else:
            # centre + R (p - centre), R the rotation of the pixel's offset (row, column) by the angle `offset`
            matrix = np.array([[np.cos(offset), np.sin(offset)], [-np.sin(offset), np.cos(offset)]])
            shift = centre_point - matrix @ centre_point
        for total, plane in zip(totals, planes, strict=True):
            total += float(weight) * ndimage.affine_transform(
                plane,
                matrix,
                offset=shift,
                order=SPLINE_ORDER[interpolation],
                mode=SCIPY_EDGE[edge],
            )
    return np.stack(totals, axis=-1)
