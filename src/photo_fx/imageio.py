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

import numpy as np
from PIL import Image, ImageCms, ImageOps

from photo_fx.ops.resample import resize  # also re-exported: `imageio.resize` was public before it moved to ops (M1)

__all__ = ["from_pil", "load", "resize", "resize_video", "save", "save_video", "to_pil"]

_SRGB = ImageCms.createProfile("sRGB")


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
                img = ImageCms.profileToProfile(img.convert("RGB"), source, _SRGB, outputMode="RGB")
            except ImageCms.PyCMSError:
                pass  # unreadable profile: treat the pixels as sRGB, which is what most viewers do too
        return from_pil(img)


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

    .mp4 / .mov: H.264 through ffmpeg, which must be installed; frames are streamed, so a long clip never has to
    fit in memory. .gif: no extra tools, but every frame is held in memory and colours are limited to 256, so use it
    for small, short clips. Returns the number of frames written.

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
    ffmpeg = shutil.which("ffmpeg")
    if ffmpeg is None:
        raise RuntimeError("resizing a video needs ffmpeg on the PATH (brew install ffmpeg)")
    shrink = f"scale=w='if(gt(iw,ih),{max_side},-2)':h='if(gt(iw,ih),-2,{max_side})':flags=lanczos"
    result = subprocess.run(
        [ffmpeg, "-y", "-loglevel", "error", "-i", str(source), "-an", "-vf", shrink,
         "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "16", "-preset", "medium", "-movflags", "+faststart",
         "-g", "30", str(target)],
        capture_output=True, text=True,
    )  # fmt: skip
    if result.returncode != 0:
        raise RuntimeError(f"ffmpeg failed: {result.stderr.strip()}")


def _save_with_ffmpeg(frames: Iterable[np.ndarray], path: Path, fps: int, max_side: int | None) -> int:
    ffmpeg = shutil.which("ffmpeg")
    if ffmpeg is None:
        raise RuntimeError("writing .mp4 needs ffmpeg on the PATH (brew install ffmpeg); .gif works without it")
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
    count = 0
    try:
        for frame in chain([first], frames):
            process.stdin.write(_to_uint8(frame).tobytes())
            count += 1
    except BrokenPipeError:
        pass  # ffmpeg stopped early; its error message below says why
    finally:
        process.stdin.close()
    error = process.stderr.read().decode(errors="replace").strip()
    if process.wait() != 0:
        raise RuntimeError(f"ffmpeg failed: {error}")
    return count


def from_pil(img: Image.Image) -> np.ndarray:
    """Convert a Pillow image to RGB float32 0..1 (transparency is dropped)."""
    return np.asarray(img.convert("RGB"), dtype=np.float32) / 255.0
