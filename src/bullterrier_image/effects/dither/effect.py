"""The `dither` effect: the picture redrawn in a few colours, found in it or chosen, its tones kept as a dot pattern."""

from itertools import product
from typing import Literal

import numpy as np
from pydantic import TypeAdapter

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.cluster import kmeans, nearest
from bullterrier_image.ops.dither import choose, in_space

# The named palettes are data, as their authors publish them (sRGB hex):
# - gameboy: the four greens widely used to draw a Game Boy screen, darkest to lightest (a community palette; the real
#   DMG panel's colours differ and depend on the display).
# - cga: IBM CGA graphics mode 4, palette 1, high intensity: black, cyan, magenta, white.
# - ega: the 16 default colours of the IBM EGA (and VGA text mode).
# - pico8: the 16 colours of the PICO-8 fantasy console.
# - cube2 to cube6: every combination of n evenly spaced levels per channel (0, 1/(n-1) ... 1), n^3 colours; matched in
#   RGB (`space`) it rounds each channel on its own.
_PALETTES: dict[str, tuple[str | tuple[float, ...], ...]] = {
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
# constant: the cubes offered, from 2 levels (8 colours) to 6 (216)
_PALETTES |= {f"cube{n}": tuple(product([i / (n - 1) for i in range(n)], repeat=3)) for n in range(2, 7)}
_COLOUR = TypeAdapter(Color)  # reads the hex codes above like a recipe's `Color` field does


class DitherParams(EffectParams):
    """The settings of `dither`: the colours, how tones become a dot pattern, and how coarse the dots are."""

    method: Literal["floyd_steinberg", "atkinson", "bayer", "blue_noise"] = param(
        "floyd_steinberg",
        doc="How tones become a pattern of dots. 'floyd_steinberg' and 'atkinson' pass each pixel's error on to its "
        "neighbours (organic grain; Floyd-Steinberg keeps every tone, Atkinson drops a quarter of the error, so "
        "highlights and shadows go clean and the contrast is higher). 'bayer' is a regular crosshatch of an 8 x 8 "
        "matrix. 'blue_noise' is an even, grainless scatter from a repeating 64 x 64 tile, the one with no visible "
        "pattern. Both of those mix colours of any palette, in steps of a sixteenth.",
    )
    palette: (
        Literal["photo", "gameboy", "cga", "ega", "pico8", "cube2", "cube3", "cube4", "cube5", "cube6"]
        | tuple[Color, ...]
    ) = param(
        "photo",
        doc="Where the colours come from. 'photo' finds `colors` colours in the picture itself (k-means); 'gameboy' "
        "(the 4 greens widely used to draw a Game Boy screen), 'cga' (black, cyan, magenta, white), 'ega' (the 16 IBM "
        "EGA colours), 'pico8' (the 16 PICO-8 colours) and 'cube2', 'cube3', 'cube4', 'cube5' and 'cube6' (every "
        "mix of 2 to 6 evenly spaced levels of red, green and blue: 8 to 216 colours) are fixed palettes; or give your "
        "own list of at least two colours, in any order ('#rrggbb', '#rgb' or three numbers 0 to 1).",
        min_length=2,
    )
    colors: int = param(
        8,
        unit="count",
        min=2,
        max=256,
        doc="How many colours to find when `palette` is 'photo' (ignored for any other palette). Raising it keeps more "
        "detail and gradation; fewer than this many are used if the picture has fewer distinct colours. 'bayer' and "
        "'blue_noise' take longer with more colours.",
    )
    cell: float = param(
        0.0,
        unit="px",
        min=0,
        max=48,
        doc="The size of one dot in px at the recipe's size (rounded to whole pixels): the picture is dithered on a "
        "grid of this size and shown with hard square dots. 0, the default, is one dot per pixel at any render size "
        "(so a `pattern` of 0 recolours pixel by pixel); 4 is a chunky dot on a 480 px picture and 10 or more a "
        "pixel-art look. Raising it gives coarser, larger dots.",
    )
    pattern: float = param(
        1.0,
        unit="fraction",
        min=0,
        max=1,
        doc="How much of the dot pattern is used: 1 = the method as designed; lower passes less error on "
        "(floyd_steinberg, atkinson) or mixes fewer colours per dot (bayer, blue_noise), so flatter areas; 0 gives "
        "each dot its nearest palette colour, plain flat colours with no pattern, whatever the method.",
    )
    space: Literal["oklab", "rgb"] = param(
        "oklab",
        doc="The colour space in which dots are matched to palette colours, the 'photo' colours are searched, and "
        "errors are measured. 'oklab' is where equal distances look equally different, so colours match as the eye "
        "sees them; 'rgb' is the picture's own red, green and blue values, where each channel is matched on its own: "
        "with a 'cube' palette and `pattern` 0 it snaps every channel to a few levels, hard colour separations.",
        advanced=True,
    )
    init: Literal["k-means++", "random"] = param(
        "k-means++",
        doc="How the search for colours starts (palette 'photo'): 'k-means++' spreads the starting colours out "
        "(Arthur and Vassilvitskii, 2007) and usually finds a better set; 'random' starts from `colors` random pixels.",
        advanced=True,
    )
    iterations: int = param(
        20,
        unit="count",
        min=1,
        max=100,
        doc="The most k-means rounds to run when `palette` is 'photo'. Each round moves every colour to the average of "
        "the pixels nearest to it; raising it lets the search settle further.",
        advanced=True,
    )
    tolerance: float = param(
        0.001,
        unit="fraction",
        min=0,
        max=1,
        doc="Stop the k-means rounds early once no colour has moved farther than this in `space` (in Oklab black to "
        "white is 1). 0 runs all `iterations`; raising it stops sooner with a rougher result.",
        advanced=True,
    )
    sample: int = param(
        10000,
        unit="count",
        min=1,
        max=1000000,
        doc="How many randomly chosen dots the colours are searched on when `palette` is 'photo'; every dot is still "
        "repainted. A picture with fewer dots is searched whole. Raising it costs time and can find rare colours; "
        "lowering it is faster but may miss small areas of colour.",
        advanced=True,
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed for the choice of sample dots and starting colours when `palette` is 'photo': the same seed "
        "always gives the same picture, another seed may find slightly different colours.",
        advanced=True,
    )


def _colours(dots: np.ndarray, params: DitherParams) -> np.ndarray:
    """The palette as float32 sRGB rows: the list given, a named palette, or `colors` found among `dots`, each the
    average of the dots nearest to its k-means centre (the dots are then painted with the palette colour nearest to
    them, which can differ for a few dots at the border between two colours)."""
    if isinstance(params.palette, tuple):
        return np.asarray(params.palette, np.float32)
    if params.palette != "photo":
        return np.array([_COLOUR.validate_python(c) for c in _PALETTES[params.palette]], np.float32)
    flat = dots.reshape(dots.shape[0] * dots.shape[1], dots.shape[2])
    lab = in_space(flat, params.space)
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
    members = np.bincount(labels, minlength=len(centres))
    sums = np.stack(
        [np.bincount(labels, weights=flat[:, c], minlength=len(centres)) for c in range(flat.shape[1])], axis=1
    )
    used = members > 0  # a centre nobody is nearest to has no colour to give
    return (sums[used] / members[used, None]).astype(np.float32)


@effect(category="stylised", params=DitherParams)
def dither(rgb: np.ndarray, params: DitherParams) -> np.ndarray:
    """Redraw the picture in a few colours, its tones kept as dots of `cell` size: ordered or error-diffusion dither,
    1-bit black and white, retro palettes, and the presets `posterize`, `threshold` and `palette_quantize`.

    The picture is averaged onto a grid of `cell`-sized squares, each of which becomes one palette colour, so seen from
    afar the dots add up to the original tones; the result is shown with hard square dots. `palette` picks the colours
    (found in the photo, a named set or your own), `method` the pattern and `pattern` how much of it is used: 0 gives
    plain flat colours. Colours are matched, and errors measured, in `space`.
    """
    height, width = rgb.shape[:2]
    cell = max(1, round(params.cell))
    row_starts, column_starts = np.arange(height)[::cell], np.arange(width)[::cell]
    row_sizes, column_sizes = np.diff(row_starts, append=height), np.diff(column_starts, append=width)
    sums = np.add.reduceat(np.add.reduceat(rgb, row_starts, axis=0), column_starts, axis=1)
    dots = sums / (row_sizes[:, None] * column_sizes[None, :])[..., None]  # the average colour of each square
    palette = _colours(dots, params)
    chosen = choose(dots, palette, method=params.method, amount=params.pattern, space=params.space)
    return np.repeat(np.repeat(palette[chosen], cell, axis=0), cell, axis=1)[:height, :width]
