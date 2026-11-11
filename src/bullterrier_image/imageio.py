"""Reading and writing pictures.

Every other module works on a single image representation: a float32 array of shape (height, width, 3),
RGB, values 0..1, sRGB-encoded (display-referred, not linear). This module is the one door for picture files: the
Rust core reads and writes them (formats, EXIF rotation, ICC profiles, 8-bit conversion), so none of that leaks into
effects. It also resizes pictures and reads `.cube` colour tables.
"""

from pathlib import Path

import numpy as np

from bullterrier_image.core import native
from bullterrier_image.core.values import Lut, read_lut
from bullterrier_image.ops.image import as_image

__all__ = ["load", "load_lut", "load_map", "load_picture", "resize", "save"]

_RGBA = 4  # the channels of RGBA


def load(path: str | Path) -> np.ndarray:
    """Read a photo as RGB float32 0..1, upright and in sRGB: what a viewer shows.

    Phone photos carry an EXIF rotation and often a Display P3 profile; both are applied here so that what you see in
    Preview is what the effects get. The format is found by content: PNG, JPEG (including CMYK), GIF, WebP, TIFF and
    BMP; a palette TIFF is not read. An embedded ICC profile (RGB, grey or CMYK) is converted to sRGB; pixels are taken
    as sRGB, quietly, when there is none, when it is unreadable or describes another colour space, and for 16-bit and
    floating-point greyscale. 16-bit greyscale keeps its precision (level / 65535); floating-point levels are clipped
    to 0..1. Transparency (an alpha channel, a transparent palette entry or colour key) is composited over white, so
    a colour hidden under a transparent pixel never shows. An animation (GIF, APNG, a multi-page TIFF) gives its first
    frame. ImageReadError, naming the path, when the file is missing, not an image, truncated, in a layout that is
    not read, or has more than 178,956,970 pixels (checked from its header, before anything is decoded). A JPEG
    decodes to within a few levels (of 255) of what other decoders give.
    """
    image = native.read(path)
    if image.shape[2] == _RGBA:  # straight alpha over white paper: colour * alpha + 1 * (1 - alpha)
        alpha = image[..., 3:]
        rgb = image[..., :3] * alpha
        rgb += 1.0 - alpha
        return rgb
    return image


def load_map(path: str | Path) -> np.ndarray:
    """Read a map of a photo someone already has (its subject, its depth, a drawing's silhouette): float32 (height,
    width), 0..1, one value per pixel of the photo it was made for.

    A picture with transparency (a cut-out PNG, a drawing: some pixel not fully opaque) gives its alpha channel; any
    other gives its brightness, so a black-and-white mask or a greyscale depth map (white = subject, white = nearest)
    works as it is, and so does one an editor saved with an opaque alpha channel (so a cut-out whose subject fills the
    whole frame reads as brightness: the cut-out of a subject that covers everything is no cut-out). 8- and 16-bit files
    keep their full range, so the same map saved in either reads the same; an embedded colour profile is ignored (a map
    is data, not colour); float levels, or integers beyond 16 bits, are ImageReadError naming the mode. The formats,
    EXIF rotation, first-frame rule and ImageReadError are as in `load`, so the map lines up with the photo it was
    made for.
    """
    return native.read_map(path)


def load_picture(path: str | Path) -> np.ndarray:
    """Read a picture to use inside an effect (a texture, a logo, a cut-out): RGBA float32 (height, width, 4), 0..1.

    The colour is what `load` gives (upright, in sRGB, EXIF and colour profile applied) but the transparency is kept
    as straight alpha instead of being composited over white; an opaque file gets alpha 1. The formats, first-frame
    rule and ImageReadError, naming the path, are as in `load`.
    """
    image = native.read(path)
    if image.shape[2] == _RGBA:
        return image
    return np.concatenate([image, np.ones((*image.shape[:2], 1), np.float32)], axis=2)


def load_lut(path: str | Path) -> Lut:
    """Read a 3D colour lookup table from an Adobe/Resolve `.cube` file (UTF-8 text); the Rust core reads it.

    Understood: `TITLE`, `DOMAIN_MIN` and `DOMAIN_MAX` (three numbers each, default 0 and 1), `LUT_3D_SIZE` (2 to 256)
    and `LUT_3D_INPUT_RANGE` (a minimum and a maximum for all channels), `#` comments, and then the size cubed rows of
    three numbers, red changing fastest. The `Lut` is labelled with `path` as given. At most 64 MiB is read. LutError
    naming the file, and the number (never the text) of the line of a fault in it, for a file that is missing, over
    64 MiB, not UTF-8, 1D (`LUT_1D_SIZE`), has a keyword or row it cannot read, or has too few or too many rows.
    """
    return read_lut(path)


def save(image: np.ndarray, path: str | Path, quality: int = 95) -> None:
    """Write `image` to `path`; the extension picks the format: `.png`, `.jpg` or `.jpeg`, `.webp`.

    `image` is (height, width, 3) RGB, or (height, width, 4) RGB with straight alpha as the last channel, which
    gives the file a transparency channel, so use .png or .webp (a JPEG cannot hold one: BackendUnavailableError).
    Its levels are floats 0..1 or unsigned integers (uint8 over 255, uint16 over 65535); anything else is an
    InvalidImageError naming what was found (values outside 0..1 are not clipped), NaN or an infinity a
    NonFiniteImageError naming the first such pixel.
    Levels are rounded to 8 bits. PNG and WebP are lossless (WebP is always written lossless); `quality` (1..100,
    default 95, ValueError outside it) sets JPEG compression only, and JPEGs keep full-resolution colour (4:4:4, no
    chroma subsampling). The file embeds a small sRGB profile (under a kilobyte), so every viewer shows the colours the
    effects made, and carries no other metadata: nothing from the photo it came from, so no EXIF and no GPS position;
    the same picture gives the same bytes every time. BackendUnavailableError, naming the extension, for any other
    extension (TIFF, GIF, BMP are read, not written). ImageWriteError, naming the path, when the file cannot be created
    (no such folder, no permission) or the picture is too large for the format; no file is left behind.
    """
    native.write(path, as_image(image, alpha=True), quality)


def resize(image: np.ndarray, longest_side: int) -> np.ndarray:
    """`image` scaled so its longest side is `longest_side` px, the aspect kept: a new float32 array of the new shape
    (alpha, if there is one, scaled with the colour).

    Lanczos for a shrink and bicubic for a growth, on floats, so a soft edge stays soft; the result is within 0..1. An
    image already that size comes back as it is. The picture is checked as `save` checks it; InvalidImageError for
    another array, RecipeError for a size beyond what a picture may have.
    """
    return native.resize(as_image(image, alpha=True), longest_side)
