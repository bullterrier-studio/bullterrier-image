"""Reading and writing pictures and videos.

Every other module works on a single image representation: a float32 array of shape (height, width, 3),
RGB, values 0..1, sRGB-encoded (display-referred, not linear). This module is the only place that knows
about file formats, EXIF rotation, ICC profiles and 8-bit conversion, so none of that leaks into effects.
"""

import io
import shutil
import subprocess
from collections.abc import Iterable, Iterator
from contextlib import contextmanager
from itertools import chain
from pathlib import Path
from typing import IO, cast

import numpy as np
from PIL import Image, ImageCms, ImageOps

from photo_fx.core.errors import BackendUnavailableError, ImageReadError
from photo_fx.ops import resample

__all__ = ["from_pil", "load", "load_matte", "resize_video", "save", "save_video", "to_pil"]

_SRGB = ImageCms.createProfile("sRGB")
_SRGB_ICC = ImageCms.ImageCmsProfile(_SRGB).tobytes()  # littlecms's built-in sRGB profile, 588 bytes
_EIGHT_BIT = 255.0  # the largest 8-bit value
_SIXTEEN_BIT = 65535.0  # the largest 16-bit value
_ALPHA = 3  # the alpha channel of RGBA
# ffmpeg filters ending every H.264 encode: convert RGB to YUV with the BT.709 (HD) matrix, not ffmpeg's BT.601
# default, in limited (TV) range, and tag the stream so, so players neither shift colours nor guess
_BT709 = (
    "scale=out_color_matrix=bt709:out_range=tv,"
    "setparams=colorspace=bt709:color_primaries=bt709:color_trc=bt709:range=tv"
)


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


def save(rgb: np.ndarray, path: str | Path, alpha: np.ndarray | None = None, quality: int = 95) -> None:
    """Write `rgb` to `path`; the extension picks the format.

    With `alpha` (height, width, 0..1) the file gets a transparency channel, so use .png or .webp. Values outside
    0..1 are clipped and NaN is written as 0 (black), silently. `quality` (1..100, default 95) sets JPEG and WebP
    compression; JPEGs keep full-resolution colour (4:4:4, no chroma subsampling) and PNG is lossless. The file
    embeds a small sRGB profile (under a kilobyte), so every viewer shows the colours the effects made, and carries no
    other metadata: nothing from the photo it came from, so no EXIF and no GPS position. BackendUnavailableError,
    naming the extension, when no writer exists for it.
    """
    suffix = Path(path).suffix.lower()
    if Image.registered_extensions().get(suffix) not in Image.SAVE:
        raise BackendUnavailableError(
            f"cannot write {suffix or 'a file with no extension'!r} pictures: use .png, .jpg or .webp"
        )
    img = to_pil(rgb, alpha)
    if alpha is None and suffix in {".jpg", ".jpeg"}:
        img.save(path, quality=quality, subsampling=0, icc_profile=_SRGB_ICC)
    elif suffix == ".webp":
        img.save(path, quality=quality, icc_profile=_SRGB_ICC)
    else:
        img.save(path, icc_profile=_SRGB_ICC)


def to_pil(rgb: np.ndarray, alpha: np.ndarray | None = None) -> Image.Image:
    """Convert to a Pillow image (RGB, or RGBA when `alpha` is given)."""
    return Image.fromarray(_to_uint8(rgb if alpha is None else np.dstack([rgb, alpha])))


def _to_uint8(data: np.ndarray) -> np.ndarray:
    """0..1 floats to 8-bit levels, rounded; out-of-range values clip, NaN (no colour at all) becomes 0, black."""
    finite = np.nan_to_num(data, nan=0.0, posinf=1.0, neginf=0.0)
    return (np.clip(finite, 0.0, 1.0) * 255.0 + 0.5).astype(np.uint8)


def save_video(frames: Iterable[np.ndarray], path: str | Path, fps: int = 30, max_side: int | None = None) -> int:
    """Write `frames` (RGB images of one size) as a video or animation; the extension picks the format.

    .mp4 / .mov: H.264 through ffmpeg (the one on the PATH, else the one the `[video]` extra installs:
    `pip install "photo-fx[video]"`); frames are streamed, so a long clip never has to fit in memory. .gif: no extra
    tools, but every frame is held in memory and colours are limited to 256, so use it for small, short clips.
    Returns the number of frames written. BackendUnavailableError when the extension is none of those, or when
    ffmpeg is needed and found nowhere, or fails.

    `max_side` shrinks frames larger than that (longest side, in px) while keeping the aspect ratio: 1600 px square
    needs an unusually high H.264 level that some players struggle with, 1080 is playable almost everywhere.
    """
    path = Path(path)
    suffix = path.suffix.lower()
    if suffix == ".gif":
        images = [
            to_pil(frame if max_side is None or max(frame.shape[:2]) <= max_side else resample.resize(frame, max_side))
            for frame in frames
        ]
        if not images:
            raise ValueError("no frames to write")
        images[0].save(path, save_all=True, append_images=images[1:], duration=round(1000 / fps), loop=0)
        return len(images)
    if suffix in {".mp4", ".mov"}:
        return _save_with_ffmpeg(frames, path, fps, max_side)
    raise BackendUnavailableError(f"cannot write {suffix!r} videos: use .mp4, .mov or .gif")


def resize_video(source: str | Path, target: str | Path, max_side: int) -> None:
    """Write a copy of the video `source` whose longest side is `max_side` px (same settings as `save_video`).

    Cheaper than rendering an animation again at a second size.
    """
    ffmpeg = _ffmpeg("resizing a video")
    shrink = f"scale=w='if(gt(iw,ih),{max_side},-2)':h='if(gt(iw,ih),-2,{max_side})':flags=lanczos"
    result = subprocess.run(
        [ffmpeg, "-y", "-loglevel", "error", "-i", str(source), "-an", "-vf", f"{shrink},{_BT709}",
         "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "16", "-preset", "medium", "-movflags", "+faststart",
         "-g", "30", str(target)],
        capture_output=True, encoding="utf-8", errors="replace",
        check=False,  # the return code is checked below, with the message
    )  # fmt: skip
    if result.returncode != 0:
        raise BackendUnavailableError(f"ffmpeg failed: {result.stderr.strip()}")


def _save_with_ffmpeg(frames: Iterable[np.ndarray], path: Path, fps: int, max_side: int | None) -> int:
    ffmpeg = _ffmpeg(f"writing {path.suffix.lower()}")
    frames = iter(frames)
    first = next(frames, None)
    if first is None:
        raise ValueError("no frames to write")
    height, width = first.shape[:2]
    filters = "pad=ceil(iw/2)*2:ceil(ih/2)*2"  # H.264 in yuv420p needs even dimensions
    if max_side is not None and max(height, width) > max_side:
        shrink = f"scale=w='if(gt(iw,ih),{max_side},-2)':h='if(gt(iw,ih),-2,{max_side})':flags=lanczos"
        filters = f"{shrink},{filters}"
    filters = f"{filters},{_BT709}"
    command = [
        ffmpeg, "-y", "-loglevel", "error",
        "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{width}x{height}", "-r", str(fps), "-i", "-",
        "-an", "-vf", filters,
        "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "14", "-preset", "medium", "-movflags", "+faststart",
        "-g", str(max(1, fps)),  # a keyframe every second: seekable, and one bad frame cannot spoil the rest
        str(path),
    ]  # fmt: skip
    process = subprocess.Popen(command, stdin=subprocess.PIPE, stderr=subprocess.PIPE)
    # typed `IO[bytes] | None`, but always open: both were asked for with PIPE
    stdin, stderr = cast(IO[bytes], process.stdin), cast(IO[bytes], process.stderr)
    count = 0
    try:
        for frame in chain([first], frames):
            stdin.write(_to_uint8(frame).tobytes())
            count += 1
    except BrokenPipeError:
        pass  # ffmpeg stopped early; its error message below says why
    finally:
        stdin.close()
    error = stderr.read().decode(errors="replace").strip()
    if process.wait() != 0:
        raise BackendUnavailableError(f"ffmpeg failed: {error}")
    return count


def _ffmpeg(task: str) -> str:
    """The ffmpeg program to run: the one on the PATH, else the one the `[video]` extra (imageio-ffmpeg) bundles."""
    found = shutil.which("ffmpeg")
    if found is not None:
        return found
    try:
        import imageio_ffmpeg

        bundled: str | None = imageio_ffmpeg.get_ffmpeg_exe()
    except (ImportError, RuntimeError):  # the extra is not installed, or has no binary for this platform
        bundled = None
    if bundled is not None and Path(bundled).is_file():
        return bundled
    raise BackendUnavailableError(
        f'{task} needs ffmpeg: install the video extra (pip install "photo-fx[video]"), or install ffmpeg so it is on '
        "the PATH (macOS: brew install ffmpeg; Debian or Ubuntu: sudo apt install ffmpeg; Windows: winget install "
        "ffmpeg). Writing .gif needs neither."
    )


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
