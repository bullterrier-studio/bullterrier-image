"""Reading and writing pictures.

Every other module works on a single image representation: a float32 array of shape (height, width, 3),
RGB, values 0..1, sRGB-encoded (display-referred, not linear). This module is the only place that knows
about file formats, EXIF rotation, ICC profiles and 8-bit conversion, so none of that leaks into effects.
"""

import io
import struct
from collections.abc import Iterator
from contextlib import contextmanager
from pathlib import Path
from typing import cast

import numpy as np
from PIL import Image, ImageCms, ImageOps

from bullterrier_image.errors import BackendUnavailableError, ImageReadError, NonFiniteImageError

__all__ = ["from_pil", "load", "load_matte", "load_picture", "save", "to_pil"]

_SRGB = ImageCms.createProfile("sRGB")
# littlecms's built-in sRGB profile, 588 bytes, embedded in every saved still. littlecms stamps the time it was made
# into the header's creation date (bytes 24..35: year, month, day, hours, minutes, seconds as big-endian 16-bit
# numbers, ICC.1:2022 section 7.2.1); a fixed date keeps a saved file's bytes the same whenever it is saved.
# The profile ID (bytes 84..99), a checksum that would cover the date, is left unset by littlecms: nothing to redo.
_GENERATED_ICC = ImageCms.ImageCmsProfile(_SRGB).tobytes()
_FIXED_DATE = struct.pack(">6H", 2000, 1, 1, 0, 0, 0)  # constant: any valid date will do; 2000-01-01 00:00:00
_SRGB_ICC = _GENERATED_ICC[:24] + _FIXED_DATE + _GENERATED_ICC[36:]  # constant: 24..35, the ICC header's date field
_EIGHT_BIT = 255.0  # the largest 8-bit value
_SIXTEEN_BIT = 65535.0  # the largest 16-bit value
_ALPHA = 3  # the alpha channel of RGBA
_RGBA = 4  # the channels of RGBA


def load(path: str | Path) -> np.ndarray:
    """Read a photo as RGB float32 0..1, upright and in sRGB: what a viewer shows (see `from_pil` for every format).

    Phone photos carry an EXIF rotation and often a Display P3 profile; both are applied here so that
    what you see in Preview is what the effects get. An animation (GIF, APNG, a multi-page TIFF) gives its first
    frame. ImageReadError, naming the path, when the file is missing, not an image, truncated, or larger than
    Pillow's decompression-bomb limit allows (twice `PIL.Image.MAX_IMAGE_PIXELS`, about 179 megapixels by default;
    raise that limit yourself to read bigger files you trust).
    """
    with _picture(path) as img:
        return from_pil(img)


def load_matte(path: str | Path) -> np.ndarray:
    """Read a subject matte someone already has: float32 (height, width), 1 = subject, 0 = background.

    A picture with transparency (a cut-out PNG) gives its alpha channel; any other gives its brightness, so a
    black-and-white mask works as it is. 8- and 16-bit files keep their full range; EXIF rotation, the first-frame
    rule and ImageReadError are as in `load`, so the matte lines up with the photo it was made for.
    """
    with _picture(path) as img:
        if img.mode.startswith("I"):  # 16-bit greyscale ("I;16" and its variants, or "I" from older Pillow)
            return np.clip(np.asarray(img, np.float32) / _SIXTEEN_BIT, 0.0, 1.0)
        if img.mode in {"RGBA", "LA", "PA"} or "transparency" in img.info:
            return np.asarray(img.convert("RGBA"), np.float32)[..., _ALPHA] / _EIGHT_BIT
        return np.asarray(img.convert("L"), np.float32) / _EIGHT_BIT


def load_picture(path: str | Path) -> np.ndarray:
    """Read a picture to use inside an effect (a texture, a logo, a cut-out): RGBA float32 (height, width, 4), 0..1.

    The colour is what `load` gives (upright, in sRGB, EXIF and colour profile applied) but the transparency is kept
    as straight alpha instead of being composited over white; an opaque file gets alpha 1. The first-frame rule and
    ImageReadError, naming the path, are as in `load`.
    """
    with _picture(path) as img:
        colour, alpha = _split_alpha(img)
        rgb = _srgb(colour, img.info.get("icc_profile"))
        if alpha is None:
            alpha = np.ones((*rgb.shape[:2], 1), np.float32)
        return np.concatenate([rgb, alpha], axis=2)


@contextmanager
def _picture(path: str | Path) -> Iterator[Image.Image]:
    """The picture at `path`, upright, its first frame if it has several (the one Pillow opens on).

    A failure to read it, also while its pixels are decoded inside the block, is raised as ImageReadError naming the
    path. Pillow's decompression-bomb limit is kept, not disabled: a file that claims huge dimensions is refused
    before its pixels are decoded.
    """
    try:
        with Image.open(path) as opened:
            ImageOps.exif_transpose(opened, in_place=True)  # in place: an upright photo is not copied
            yield opened
    except (OSError, ValueError, EOFError, Image.DecompressionBombError) as error:
        raise ImageReadError(f"cannot read {path} as an image: {error}") from error


def save(image: np.ndarray, path: str | Path, quality: int = 95) -> None:
    """Write `image` to `path`; the extension picks the format.

    `image` is float (height, width, 3) RGB, or (height, width, 4) RGB with straight alpha as the last channel, which
    gives the file a transparency channel, so use .png or .webp (a JPEG cannot hold one: BackendUnavailableError).
    Values outside 0..1 are clipped; NaN or an infinity raises NonFiniteImageError, naming the first such pixel.
    `quality` (1..100, default 95) sets JPEG and WebP compression; JPEGs keep full-resolution colour (4:4:4, no chroma
    subsampling) and PNG is lossless. The file embeds a small sRGB profile (under a kilobyte), so every viewer shows the
    colours the effects made, and carries no other metadata: nothing from the photo it came from, so no EXIF and no GPS
    position. BackendUnavailableError, naming the extension, when no writer exists for it.
    """
    suffix = Path(path).suffix.lower()
    if Image.registered_extensions().get(suffix) not in Image.SAVE:
        raise BackendUnavailableError(
            f"cannot write {suffix or 'a file with no extension'!r} pictures: use .png, .jpg or .webp"
        )
    jpeg = suffix in {".jpg", ".jpeg"}
    if jpeg and image.shape[2] == _RGBA:
        raise BackendUnavailableError(f"cannot write {suffix!r} pictures with transparency: use .png or .webp")
    img = to_pil(image)
    if jpeg:
        img.save(path, quality=quality, subsampling=0, icc_profile=_SRGB_ICC)
    elif suffix == ".webp":
        img.save(path, quality=quality, icc_profile=_SRGB_ICC)
    else:
        img.save(path, icc_profile=_SRGB_ICC)


def to_pil(image: np.ndarray) -> Image.Image:
    """Convert to a Pillow image: RGB for (height, width, 3), RGBA (straight alpha last) for (height, width, 4)."""
    return Image.fromarray(_to_uint8(image))


def _to_uint8(data: np.ndarray) -> np.ndarray:
    """0..1 floats to 8-bit levels, rounded; out-of-range values clip. NonFiniteImageError for NaN or infinity."""
    finite = np.isfinite(data)
    if not finite.all():
        row, column = np.argwhere(~finite)[0][:2]
        raise NonFiniteImageError(
            f"cannot write a picture with NaN or infinite values: the first is at row {row}, column {column}"
        )
    return (np.clip(data, 0.0, 1.0) * 255.0 + 0.5).astype(np.uint8)


def from_pil(img: Image.Image) -> np.ndarray:
    """Convert a Pillow image of any mode to RGB float32 0..1 in sRGB: the colours a viewer shows.

    - 8-bit modes (RGB, greyscale L, 1-bit, palette P, CMYK) give level / 255. 16-bit greyscale (I;16 and its byte
      orders, and I, which Pillow uses for 16-bit TIFFs) keeps its precision: level / 65535. Float greyscale (F) is
      read as 0..1. Values outside 0..1 are clipped.
    - Transparency (an alpha channel, a transparent palette entry or colour key) is composited over white, so a
      colour hidden under a transparent pixel never shows.
    - An embedded ICC profile (`img.info["icc_profile"]`, e.g. Display P3, a CMYK press profile or a grey gamma) is
      converted to sRGB from the image's own mode. Pixels are taken as sRGB, the web's rule for untagged pictures,
      when there is no profile, when it is unreadable or describes another colour space (quietly: no warning), and
      for 16-bit and float greyscale, which littlecms cannot convert to 8-bit RGB.
    """
    colour, alpha = _split_alpha(img)
    rgb = _srgb(colour, img.info.get("icc_profile"))
    if alpha is not None:  # straight alpha over white paper: colour * alpha + 1 * (1 - alpha)
        rgb *= alpha
        rgb += 1.0 - alpha
    return rgb


def _split_alpha(img: Image.Image) -> tuple[Image.Image, np.ndarray | None]:
    """The colour channels of `img`, and its opacity as (height, width, 1) float32 0..1 (None when it is opaque)."""
    if img.mode == "1":
        img = img.convert("L")
    if img.mode in {"P", "PA"} or "transparency" in img.info:
        img = img.convert("LA" if img.mode == "L" else "RGBA")
    elif img.mode in {"La", "RGBa"}:  # premultiplied: Pillow un-premultiplies on conversion
        img = img.convert("LA" if img.mode == "La" else "RGBA")
    if img.mode not in {"LA", "RGBA"}:
        return img, None
    alpha = np.asarray(img.getchannel("A"), dtype=np.float32)[..., np.newaxis]
    alpha /= _EIGHT_BIT
    return img.convert(img.mode[:-1]), alpha


def _srgb(colour: Image.Image, profile: bytes | None) -> np.ndarray:
    """The opaque image `colour` as RGB float32 0..1 in sRGB, its embedded `profile` applied."""
    if colour.mode.startswith("I") or colour.mode == "F":
        grey = np.asarray(colour, dtype=np.float32)
        if colour.mode != "F":
            grey /= _SIXTEEN_BIT
        return np.repeat(np.clip(grey, 0.0, 1.0)[..., np.newaxis], 3, axis=2)
    img = _profiled(colour, profile) if profile else None
    if img is None:
        img = colour if colour.mode == "RGB" else colour.convert("RGB")
    rgb = np.asarray(img, dtype=np.float32)
    rgb /= _EIGHT_BIT
    return rgb


def _profiled(colour: Image.Image, profile: bytes) -> Image.Image | None:
    """`colour` converted from its embedded `profile` to sRGB RGB, or None when the profile cannot be used.

    The transform starts from the image's own mode (CMYK inks, grey levels or RGB), as ICC.1 colour management
    requires: converting to RGB first would feed CMYK or grey data to a profile that describes something else.
    An unreadable profile, or one for another colour space, leaves the pixels to be taken as sRGB, as most viewers do.
    """
    try:
        source = ImageCms.ImageCmsProfile(io.BytesIO(profile))
        # typed `Image | None` by Pillow, but None only with inPlace=True, which is not used here
        return cast(Image.Image, ImageCms.profileToProfile(colour, source, _SRGB, outputMode="RGB"))
    except (OSError, ImageCms.PyCMSError):  # OSError: not a profile at all; PyCMSError: no transform for this mode
        return None
