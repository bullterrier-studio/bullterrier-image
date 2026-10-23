"""Colour as effects meet it: colour literals from recipes, perceived brightness, and how much ink a mark-making effect
lays down on a given paper colour."""

from collections.abc import Sequence

import numpy as np


def parse_color(color: str | Sequence[float]) -> np.ndarray:
    """'#rrggbb' / '#rgb', or [r, g, b] with values 0..1 -> float32 array of 3. Raises ValueError for anything else.

    (The same forms as `photo_fx.core.types.Color`, spelt out here because ops never import core.)
    """
    if isinstance(color, str):
        h = color.lstrip("#")
        if len(h) == 3:  # constant: the short hex form #rgb
            h = "".join(c * 2 for c in h)
        if len(h) != 6:  # constant: the long hex form #rrggbb
            raise ValueError(f"bad colour {color!r}: use '#rrggbb'")
        # constant: two hex digits (base 16) per channel, at offsets 0, 2 and 4
        return np.array([int(h[i : i + 2], 16) / 255.0 for i in (0, 2, 4)], dtype=np.float32)
    if len(color) != 3:  # constant: red, green and blue
        raise ValueError(f"bad colour {color!r}: need 3 numbers")
    return np.asarray(color, dtype=np.float32)


LUMA = (0.2126, 0.7152, 0.0722)  # constant: Rec. 709 weights of red, green, blue in perceived brightness


def luminance(rgb: np.ndarray) -> np.ndarray:
    """Perceived brightness (height, width) 0..1 (Rec. 709 weights)."""
    return rgb @ np.array(LUMA, dtype=np.float32)


def ink_amount(lum: np.ndarray, background: np.ndarray) -> np.ndarray:
    """How much ink a mark-making effect (ascii, halftone) should lay down for brightness `lum` on `background`.

    Light marks on a dark background need to be dense where the picture is bright; dark marks on a light
    background need to be dense where it is dark. Deciding that here means callers never pass an `invert` flag.
    """
    return lum if float(luminance(background)) < 0.5 else 1.0 - lum


def vivid(colors: np.ndarray, *, floor: float) -> np.ndarray:
    """Brighten colours sampled from a photo so thin marks drawn in them stay readable (never darkens).

    Each colour is scaled until its strongest channel is full, but by at most 1 / `floor` (0 < floor <= 1): a colour
    whose strongest channel is below `floor` is scaled by 1 / `floor`, so near-black stays dark instead of turning into
    a loud, noisy hue. A floor of 1 leaves every colour as it is.
    """
    return np.clip(colors / np.maximum(colors.max(axis=-1, keepdims=True), floor), 0.0, 1.0)
