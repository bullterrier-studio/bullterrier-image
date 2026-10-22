"""Reading and writing pictures and videos.

Every other module works on a single image representation: a float32 array of shape (height, width, 3),
RGB, values 0..1, sRGB-encoded (display-referred, not linear). This module is the only place that knows
about file formats, EXIF rotation, ICC profiles and 8-bit conversion, so none of that leaks into effects.
`resize` is `photo_fx.ops.resample.resize`, kept here because `imageio.resize` was public before it moved.
"""

import io
import shutil
import subprocess
from collections.abc import Iterable
from itertools import chain
from pathlib import Path
from typing import IO, cast

import numpy as np
from PIL import Image, ImageCms, ImageOps

from photo_fx.ops.resample import resize  # also re-exported: `imageio.resize` was public before it moved to ops (M1)

__all__ = ["from_pil", "load", "load_matte", "resize", "resize_video", "save", "save_video", "to_pil"]

_SRGB = ImageCms.createProfile("sRGB")
_EIGHT_BIT = 255.0  # the largest 8-bit value
_SIXTEEN_BIT = 65535.0  # the largest 16-bit value
_ALPHA = 3  # the alpha channel of RGBA


def load(path: str | Path) -> np.ndarray:
    """Read a photo as RGB float32 0..1, upright and in sRGB.

    Phone photos carry an EXIF rotation and often a Display P3 profile; both are applied here so that
    what you see in Preview is what the effects get.
    """
    with Image.open(path) as opened:
        img = ImageOps.exif_transpose(opened)
        profile = img.info.get("icc_profile")
        if profile:
            try:
                source = ImageCms.ImageCmsProfile(io.BytesIO(profile))
                # typed `Image | None` by Pillow, but None only with inPlace=True, which is not used here
                converted = ImageCms.profileToProfile(img.convert("RGB"), source, _SRGB, outputMode="RGB")
                img = cast(Image.Image, converted)
            except ImageCms.PyCMSError:
                pass  # unreadable profile: treat the pixels as sRGB, which is what most viewers do too
        return from_pil(img)


def load_matte(path: str | Path) -> np.ndarray:
    """Read a subject matte someone already has: float32 (height, width), 1 = subject, 0 = background.

    A picture with transparency (a cut-out PNG) gives its alpha channel; any other gives its brightness, so a
    black-and-white mask works as it is. 8- and 16-bit files keep their full range; EXIF rotation is applied, as in
    `load`, so the matte lines up with the photo it was made for.
    """
    with Image.open(path) as opened:
        img = ImageOps.exif_transpose(opened)
        if img.mode.startswith("I"):  # 16-bit greyscale ("I;16" and its variants, or "I" from older Pillow)
            return np.clip(np.asarray(img, np.float32) / _SIXTEEN_BIT, 0.0, 1.0)
        if img.mode in {"RGBA", "LA", "PA"} or "transparency" in img.info:
            return np.asarray(img.convert("RGBA"), np.float32)[..., _ALPHA] / _EIGHT_BIT
        return np.asarray(img.convert("L"), np.float32) / _EIGHT_BIT


def save(rgb: np.ndarray, path: str | Path, alpha: np.ndarray | None = None, quality: int = 95) -> None:
    """Write `rgb` to `path`; the extension picks the format.

    With `alpha` (height, width, 0..1) the file gets a transparency channel, so use .png or .webp.
    """
    img = to_pil(rgb, alpha)
    suffix = Path(path).suffix.lower()
    if alpha is None and suffix in {".jpg", ".jpeg"}:
        img.save(path, quality=quality, subsampling=0)
    elif suffix == ".webp":
        img.save(path, quality=quality)
    else:
        img.save(path)


def to_pil(rgb: np.ndarray, alpha: np.ndarray | None = None) -> Image.Image:
    """Convert to a Pillow image (RGB, or RGBA when `alpha` is given)."""
    return Image.fromarray(_to_uint8(rgb if alpha is None else np.dstack([rgb, alpha])))


def _to_uint8(data: np.ndarray) -> np.ndarray:
    return (np.clip(data, 0.0, 1.0) * 255.0 + 0.5).astype(np.uint8)


def save_video(frames: Iterable[np.ndarray], path: str | Path, fps: int = 30, max_side: int | None = None) -> int:
    """Write `frames` (RGB images of one size) as a video or animation; the extension picks the format.

    .mp4 / .mov: H.264 through ffmpeg (the one on the PATH, else the one the `[video]` extra installs:
    `pip install "photo-fx[video]"`); frames are streamed, so a long clip never has to fit in memory. .gif: no extra
    tools, but every frame is held in memory and colours are limited to 256, so use it for small, short clips.
    Returns the number of frames written. RuntimeError when ffmpeg is needed and found nowhere, or fails.

    `max_side` shrinks frames larger than that (longest side, in px) while keeping the aspect ratio: 1600 px square
    needs an unusually high H.264 level that some players struggle with, 1080 is playable almost everywhere.
    """
    path = Path(path)
    suffix = path.suffix.lower()
    if suffix == ".gif":
        images = [
            to_pil(frame if max_side is None or max(frame.shape[:2]) <= max_side else resize(frame, max_side))
            for frame in frames
        ]
        if not images:
            raise ValueError("no frames to write")
        images[0].save(path, save_all=True, append_images=images[1:], duration=round(1000 / fps), loop=0)
        return len(images)
    if suffix in {".mp4", ".mov"}:
        return _save_with_ffmpeg(frames, path, fps, max_side)
    raise ValueError(f"cannot write {suffix!r} videos: use .mp4, .mov or .gif")


def resize_video(source: str | Path, target: str | Path, max_side: int) -> None:
    """Write a copy of the video `source` whose longest side is `max_side` px (same settings as `save_video`).

    Cheaper than rendering an animation again at a second size.
    """
    ffmpeg = _ffmpeg("resizing a video")
    shrink = f"scale=w='if(gt(iw,ih),{max_side},-2)':h='if(gt(iw,ih),-2,{max_side})':flags=lanczos"
    result = subprocess.run(
        [ffmpeg, "-y", "-loglevel", "error", "-i", str(source), "-an", "-vf", shrink,
         "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "16", "-preset", "medium", "-movflags", "+faststart",
         "-g", "30", str(target)],
        capture_output=True, text=True, check=False,  # the return code is checked below, with the message
    )  # fmt: skip
    if result.returncode != 0:
        raise RuntimeError(f"ffmpeg failed: {result.stderr.strip()}")


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
        raise RuntimeError(f"ffmpeg failed: {error}")
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
    raise RuntimeError(
        f'{task} needs ffmpeg: install the video extra (pip install "photo-fx[video]"), or install ffmpeg so it is on '
        "the PATH (macOS: brew install ffmpeg; Debian or Ubuntu: sudo apt install ffmpeg; Windows: winget install "
        "ffmpeg). Writing .gif needs neither."
    )


def from_pil(img: Image.Image) -> np.ndarray:
    """Convert a Pillow image to RGB float32 0..1 (transparency is dropped)."""
    return np.asarray(img.convert("RGB"), dtype=np.float32) / 255.0
