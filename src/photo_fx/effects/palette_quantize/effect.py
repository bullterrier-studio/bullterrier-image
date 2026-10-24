"""The `palette_quantize` effect: reduce the picture to a few colours, found by k-means or from a named palette."""

from typing import Literal

import numpy as np
from pydantic import TypeAdapter

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.cluster import kmeans, nearest
from photo_fx.ops.colorspace import srgb_to_linear

# The named palettes are data, as their authors publish them (sRGB hex):
# - gameboy: the four greens widely used to draw a Game Boy screen, darkest to lightest (a community palette; the real
#   DMG panel's colours differ and depend on the display).
# - cga: IBM CGA graphics mode 4, palette 1, high intensity: black, cyan, magenta, white.
# - ega: the 16 default colours of the IBM EGA (and VGA text mode).
# - pico8: the 16 colours of the PICO-8 fantasy console.
_PALETTES: dict[str, tuple[str, ...]] = {
    "gameboy": ("#0f380f", "#306230", "#8bac0f", "#9bbc0f"),
    "cga": ("#000000", "#55ffff", "#ff55ff", "#ffffff"),
    "ega": (
        "#000000", "#0000aa", "#00aa00", "#00aaaa", "#aa0000", "#aa00aa", "#aa5500", "#aaaaaa",
        "#555555", "#5555ff", "#55ff55", "#55ffff", "#ff5555", "#ff55ff", "#ffff55", "#ffffff",
    ),
    "pico8": (
        "#000000", "#1d2b53", "#7e2553", "#008751", "#ab5236", "#5f574f", "#c2c3c7", "#fff1e8",
        "#ff004d", "#ffa300", "#ffec27", "#00e436", "#29adff", "#83769c", "#ff77a8", "#ffccaa",
    ),
}  # fmt: skip
_COLOUR = TypeAdapter(Color)  # reads the hex codes above like a recipe's `Color` field does


class PaletteQuantizeParams(EffectParams):
    """Which colours the picture is reduced to, and how colours are found in it."""

    palette: Literal["photo", "gameboy", "cga", "ega", "pico8"] = param(
        "photo",
        doc="Where the colours come from. 'photo' finds `colors` colours in the picture itself (k-means); the others "
        "are fixed palettes: 'gameboy' (the 4 greens widely used to draw a Game Boy screen), 'cga' (black, cyan, "
        "magenta, white), 'ega' (the 16 IBM EGA colours) and 'pico8' (the 16 PICO-8 colours). Every pixel gets the "
        "nearest colour in Oklab, a space where equal distances look equally different.",
    )
    colors: int = param(
        8,
        unit="count",
        min=2,
        max=256,
        doc="How many colours to find when `palette` is 'photo' (ignored for a fixed palette). Raising it keeps more "
        "detail and gradation; fewer than this many are used if the picture has fewer distinct colours.",
    )
    init: Literal["k-means++", "random"] = param(
        "k-means++",
        doc="How the search for colours starts (palette 'photo'): 'k-means++' spreads the starting colours out "
        "(Arthur and Vassilvitskii, 2007) and usually finds a better set; 'random' starts from `colors` random pixels.",
    )
    iterations: int = param(
        20,
        unit="count",
        min=1,
        max=100,
        doc="The most k-means rounds to run when `palette` is 'photo'. Each round moves every colour to the average of "
        "the pixels nearest to it; raising it lets the search settle further.",
    )
    tolerance: float = param(
        0.001,
        unit="fraction",
        min=0,
        max=1,
        doc="Stop the k-means rounds early once no colour has moved farther than this in Oklab (where black to white "
        "is 1). 0 runs all `iterations`; raising it stops sooner with a rougher result.",
    )
    sample: int = param(
        10000,
        unit="count",
        min=1,
        max=1000000,
        doc="How many randomly chosen pixels the colours are searched on when `palette` is 'photo'; every pixel is "
        "still repainted. A picture with fewer pixels is searched whole. Raising it costs time and can find rare "
        "colours; lowering it is faster but may miss small areas of colour.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed for the choice of sample pixels and starting colours: the same seed always gives the same "
        "picture, another seed may find slightly different colours.",
    )


def _oklab(rgb: np.ndarray) -> np.ndarray:
    """Oklab (Björn Ottosson, 2020) of sRGB rows `(n, 3)` in 0..1: float32 rows of lightness, green-red, blue-yellow."""
    # constant: Ottosson's matrix from linear sRGB to cone responses
    to_cones = np.array(
        [
            [0.4122214708, 0.5363325363, 0.0514459929],
            [0.2119034982, 0.6806995451, 0.1073969566],
            [0.0883024619, 0.2817188376, 0.6299787005],
        ],
        dtype=np.float32,
    )
    # constant: Ottosson's matrix from compressed cone responses to lightness and the two colour axes
    to_lab = np.array(
        [
            [0.2104542553, 0.7936177850, -0.0040720468],
            [1.9779984951, -2.4285922050, 0.4505937099],
            [0.0259040371, 0.7827717662, -0.8086757660],
        ],
        dtype=np.float32,
    )
    return np.cbrt(srgb_to_linear(rgb) @ to_cones.T) @ to_lab.T


@effect(category="colour", params=PaletteQuantizeParams)
def palette_quantize(rgb: np.ndarray, params: PaletteQuantizeParams) -> np.ndarray:
    """Reduce the picture to a few colours: `colors` found in it by k-means, or a named `palette`.

    Distances are measured in Oklab, so a pixel gets the colour that looks closest to it, not the one with the nearest
    red, green and blue numbers. With palette 'photo' the colours are searched on `sample` random pixels, then every
    pixel is given its nearest colour and each colour becomes the average of the pixels it replaced, so the picture's
    overall colour is kept. With a named palette the pixels are painted in the palette's exact colours.
    """
    flat = rgb.reshape(rgb.shape[0] * rgb.shape[1], rgb.shape[2])
    lab = _oklab(flat)
    if params.palette != "photo":
        colours = np.array([_COLOUR.validate_python(c) for c in _PALETTES[params.palette]], dtype=np.float32)
        return colours[nearest(lab, _oklab(colours))].reshape(rgb.shape)
    search = lab
    if params.sample < len(flat):
        chooser = np.random.default_rng(params.seed + 1)  # another stream than the one kmeans starts from
        search = lab[chooser.choice(len(flat), size=params.sample, replace=False)]
    centres = kmeans(
        search,
        params.colors,
        init=params.init,
        iterations=params.iterations,
        tolerance=params.tolerance,
        seed=params.seed,
    )
    labels = nearest(lab, centres)
    members = np.maximum(np.bincount(labels, minlength=len(centres)), 1)  # a colour nobody got is never looked up
    averages = np.stack(
        [np.bincount(labels, weights=flat[:, c], minlength=len(centres)) / members for c in range(flat.shape[1])],
        axis=1,
    )
    return averages.astype(np.float32)[labels].reshape(rgb.shape)
