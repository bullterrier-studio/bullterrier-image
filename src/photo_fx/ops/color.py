"""Colour as effects meet it: perceived brightness, and how much ink a mark-making effect lays down on a given paper
colour."""

import numpy as np

LUMA = (0.2126, 0.7152, 0.0722)  # constant: Rec. 709 weights of red, green, blue in perceived brightness


def luminance(rgb: np.ndarray) -> np.ndarray:
    """Perceived brightness (height, width) 0..1 (Rec. 709 weights)."""
    return rgb @ np.array(LUMA, dtype=np.float32)


def ink_amount(lum: np.ndarray, background: np.ndarray, *, threshold: float) -> np.ndarray:
    """How much ink a mark-making effect (ascii, halftone) should lay down for brightness `lum` on `background`.

    Light marks on a dark background need to be dense where the picture is bright; dark marks on a light
    background need to be dense where it is dark. A background is dark when its luminance is below `threshold`
    (0..1; 0 = every background is light, 1 = every one but white is dark). Deciding that here means callers never
    pass an `invert` flag.
    """
    return lum if float(luminance(background)) < threshold else 1.0 - lum


def vivid(colors: np.ndarray, *, floor: float) -> np.ndarray:
    """Brighten colours sampled from a photo so thin marks drawn in them stay readable (never darkens).

    Each colour is scaled until its strongest channel is full, but by at most 1 / `floor` (0 < floor <= 1): a colour
    whose strongest channel is below `floor` is scaled by 1 / `floor`, so near-black stays dark instead of turning into
    a loud, noisy hue. A floor of 1 leaves every colour as it is.
    """
    return np.clip(colors / np.maximum(colors.max(axis=-1, keepdims=True), floor), 0.0, 1.0)
