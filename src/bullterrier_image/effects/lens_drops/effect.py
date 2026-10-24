"""The `lens_drops` effect: water drops on the lens, each showing the scene upside down and blurred."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.blur import gaussian_blur
from bullterrier_image.ops.warp import remap


class LensDropsParams(EffectParams):
    """Lens drops: round drops of water on the lens, each a small upside-down, softened view of the scene."""

    count: int = param(
        14,
        unit="count",
        min=0,
        max=200,
        doc="How many drops there are, whatever the size of the picture (a number, not a density). 0 = none (the "
        "picture unchanged).",
    )
    radius: float = param(
        24.0,
        unit="px",
        min=1,
        max=200,
        doc="The radius in px of the biggest drops; the smallest are smaller by `size_variation`. Tuned for a "
        "picture about 480 px high. Larger = bigger drops.",
    )
    size_variation: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="How much the drops differ in size: each has a random share of `radius` from 1 - size_variation to 1. "
        "0 = every drop alike, 1 = from the full radius down to specks.",
    )
    refraction: float = param(
        1.5,
        unit="ratio",
        min=0.25,
        max=4,
        doc="How strongly a drop bends the scene: it shows the picture turned upside down about the drop's middle and "
        "this many times as wide. 1 = the same scale, turned round; above 1 a wider view in miniature, as in a real "
        "drop; below 1 a magnified view.",
    )
    blur: float = param(
        1.5,
        unit="px",
        min=0,
        max=20,
        doc="How much the scene inside a drop is blurred, the sigma of a Gaussian in px, as a drop is never quite in "
        "focus. 0 = sharp.",
    )
    highlight: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="How bright the drops' shine is: white light screened along the rim of each drop, strongest at its top "
        "(light from above) and fading to nothing at its bottom and in its middle. 0 = none, so a flat picture stays "
        "flat; 0.5 is a soft glint.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always puts the same drops in the same places, another seed others.",
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where the blur's Gaussian is cut off, in sigmas (only when blur > 0). Larger is more exact and slower.",
        advanced=True,
    )
    interpolation: Literal["nearest", "bilinear", "bicubic"] = param(
        "bilinear",
        doc="How a point between pixels is read from the picture inside a drop: 'nearest' takes the closest pixel "
        "(fastest, keeps every value exactly as it is but edges turn jagged), 'bilinear' blends the four pixels "
        "around it (smooth, slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest, "
        "a little slower, and may ring beside hard edges).",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What a drop sees beyond the picture's edge (a drop near the border or a strong refraction reads from "
        "outside it), and the blur with it: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the "
        "edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge).",
        advanced=True,
    )


@effect(category="atmosphere", params=LensDropsParams)
def lens_drops(rgb: np.ndarray, params: LensDropsParams) -> np.ndarray:
    """Put `count` water drops on the picture, each showing the blurred scene turned upside down by `refraction`.

    A drop at a random place has a random radius from (1 - `size_variation`) x `radius` to all of it. The pixel at
    offset d from its middle shows the picture, blurred by `blur`, at offset -`refraction` x d from it, read by
    `interpolation` and `edge`, and the drop's edge is antialiased over a pixel. The `highlight` is white screened over
    the drop, `highlight` x (distance / radius)² x (1 - dy / distance) / 2 with dy the pixel's row below the middle.
    Later drops cover earlier ones. Pixels outside every drop are exactly as they were; the same `seed` always gives the
    same drops.
    """
    if params.count == 0:
        return rgb.copy()
    height, width = rgb.shape[:2]
    # constant: three random numbers per drop, its x, y and size
    x, y, size = np.random.default_rng(params.seed).random((params.count, 3)).T
    scene = gaussian_blur(rgb, params.blur, truncate=params.truncate, edge=params.edge)
    # every drop's pixels, one after the other: where they are, how much the drop covers each, how it shines
    places, covers, shines, sources = [], [], [], []
    for middle_x, middle_y, reach in zip(
        x * width, y * height, params.radius * (1 - params.size_variation * (1 - size)), strict=True
    ):
        top, bottom = max(int(middle_y - reach) - 1, 0), min(int(middle_y + reach) + 2, height)
        left, right = max(int(middle_x - reach) - 1, 0), min(int(middle_x + reach) + 2, width)
        rows, cols = np.mgrid[top:bottom, left:right]
        down, across = rows - middle_y, cols - middle_x
        distance = np.hypot(down, across)
        cover = np.clip(reach + 0.5 - distance, 0.0, 1.0)  # a pixel is a unit box: antialiased over one pixel
        drop = cover > 0
        places.append((rows[drop], cols[drop]))
        covers.append(cover[drop])
        shines.append((params.highlight * distance * (distance - down) / (2 * reach**2))[drop])
        sources.append((middle_y - params.refraction * down[drop], middle_x - params.refraction * across[drop]))
    seen = remap(
        scene,
        np.concatenate([row for row, _ in sources]),
        np.concatenate([col for _, col in sources]),
        interpolation=params.interpolation,
        edge=params.edge,
    )
    out = rgb.copy()
    shown = np.split(seen, np.cumsum([len(cover) for cover in covers[:-1]]))  # what each drop shows, in its order
    for (rows, cols), cover, shine, colours in zip(places, covers, shines, shown, strict=True):
        colours = colours + shine[:, None] * (1 - colours)  # the shine is screened over what the drop shows
        out[rows, cols] = out[rows, cols] * (1 - cover[:, None]) + colours * cover[:, None]
    return out.astype(np.float32)
