import re
import shutil
import subprocess
import sys

import numpy as np
import pytest
from PIL import GifImagePlugin, Image

from photo_fx import imageio


def test_save_load_roundtrip_within_8_bit_precision(photo, tmp_path):
    imageio.save(photo, tmp_path / "a.png")
    back = imageio.load(tmp_path / "a.png")
    assert back.dtype == np.float32
    assert back.shape == photo.shape
    assert np.abs(back - photo).max() <= 1 / 255


def test_load_applies_exif_rotation(tmp_path):
    img = Image.new("RGB", (40, 20), "red")
    exif = Image.Exif()
    exif[0x0112] = 6  # rotate 90 degrees clockwise when displayed
    img.save(tmp_path / "r.jpg", exif=exif)
    assert imageio.load(tmp_path / "r.jpg").shape[:2] == (40, 20)


# --- every pixel format loads as sRGB float RGB 0..1 ----------------------------------------------------------------
# Each case is a one-row picture Pillow writes in that mode, with the RGB values (0..1) a viewer shows for each pixel,
# worked out by hand: a grey level v is (v, v, v); a 16-bit level is level / 65535; a pixel with alpha a over the
# white background is colour * a + 1 * (1 - a).


def palette_row(indices, colours, **info) -> Image.Image:
    img = Image.new("P", (len(indices), 1))
    img.putpalette([channel for colour in colours for channel in colour])
    img.putdata(indices)
    img.info.update(info)
    return img


def row(pixels, dtype, mode=None) -> Image.Image:
    return Image.fromarray(np.array([pixels], dtype), mode)


PIXEL_FORMATS = [  # (file suffix, picture, keyword arguments for saving it, expected RGB per pixel)
    pytest.param(".png", row([0, 51, 204, 255], np.uint8), {}, [0, 0.2, 0.8, 1], id="L 8-bit grey"),
    pytest.param(".png", row([False, True], bool), {}, [0, 1], id="1-bit"),
    pytest.param(".png", row([0, 16384, 65535], np.uint16), {}, [0, 16384 / 65535, 1], id="I;16 PNG"),
    pytest.param(".tif", row([0, 16384, 65535], np.uint16), {}, [0, 16384 / 65535, 1], id="I;16 TIFF"),
    pytest.param(".tif", row([0, 32768, 65535], np.int32), {}, [0, 32768 / 65535, 1], id="I 32-bit TIFF"),
    pytest.param(".tif", row([0.25, 1.5, -0.5], np.float32), {}, [0.25, 1, 0], id="F float TIFF, clipped"),
    pytest.param(".png", palette_row([0, 1], [(255, 0, 0), (0, 0, 255)]), {}, [(1, 0, 0), (0, 0, 1)], id="P"),
    pytest.param(
        ".png", palette_row([0, 1], [(0, 255, 0), (255, 0, 0)]), {"transparency": 0}, [1, (1, 0, 0)],
        id="P, transparent index over white",
    ),
    pytest.param(
        ".png", row([(0, 0), (0, 255), (0, 51)], np.uint8, "LA"), {}, [1, 0, 0.8], id="LA over white",
    ),
    pytest.param(
        ".png", row([(0, 255, 0, 0), (255, 0, 0, 128), (0, 0, 255, 255)], np.uint8, "RGBA"), {},
        [1, (1, 127 / 255, 127 / 255), (0, 0, 1)], id="RGBA over white",
    ),
    pytest.param(
        ".png", row([(0, 255, 0), (255, 0, 0)], np.uint8), {"transparency": (0, 255, 0)}, [1, (1, 0, 0)],
        id="RGB, transparent colour key",
    ),
    pytest.param(".png", row([0, 102], np.uint8), {"transparency": 0}, [1, 0.4], id="L, transparent grey key"),
    pytest.param(
        ".tif", row([(0, 255, 255, 0), (0, 0, 0, 255), (0, 0, 0, 0)], np.uint8, "CMYK"), {}, [(1, 0, 0), 0, 1],
        id="CMYK without a profile",
    ),
    pytest.param(".png", row([(255, 0, 0), (0, 102, 255)], np.uint8), {}, [(1, 0, 0), (0, 0.4, 1)], id="RGB"),
]  # fmt: skip


@pytest.mark.parametrize(("suffix", "picture", "save", "expected"), PIXEL_FORMATS)
def test_every_pixel_format_loads_as_the_colours_a_viewer_shows(tmp_path, suffix, picture, save, expected):
    path = tmp_path / f"picture{suffix}"
    picture.save(path, **save)
    rgb = imageio.load(path)
    assert rgb.dtype == np.float32
    assert rgb.shape == (1, len(expected), 3)
    np.testing.assert_allclose(rgb[0], [np.broadcast_to(value, 3) for value in expected], atol=1e-6)


def test_the_colour_under_a_transparent_pixel_never_shows(tmp_path):
    hidden = np.zeros((4, 6, 4), np.uint8)
    hidden[..., :3] = np.random.default_rng(3).integers(0, 256, (4, 6, 3))  # any colour, alpha 0 everywhere
    Image.fromarray(hidden, "RGBA").save(tmp_path / "clear.png")
    np.testing.assert_array_equal(imageio.load(tmp_path / "clear.png"), np.ones((4, 6, 3), np.float32))


def test_save_with_alpha_writes_transparency(photo, matte, tmp_path):
    imageio.save(photo, tmp_path / "cut.png", alpha=matte)
    with Image.open(tmp_path / "cut.png") as img:
        assert img.mode == "RGBA"
        alpha = np.asarray(img)[..., 3]
        assert alpha[0, 0] == 0
        assert alpha[48, 64] == 255


def _frames(count=4, height=47, width=63):
    rng = np.random.default_rng(0)
    return [rng.random((height, width, 3), dtype=np.float32) for _ in range(count)]


def test_save_video_gif_keeps_every_frame(tmp_path):
    assert imageio.save_video(iter(_frames(5)), tmp_path / "a.gif", fps=10) == 5
    with Image.open(tmp_path / "a.gif") as gif:
        assert isinstance(gif, GifImagePlugin.GifImageFile)
        assert gif.n_frames == 5
        assert gif.size == (63, 47)


@pytest.mark.skipif(shutil.which("ffmpeg") is None, reason="needs ffmpeg")
def test_save_video_mp4_streams_frames_and_copes_with_odd_sizes(tmp_path):
    assert imageio.save_video(iter(_frames(6)), tmp_path / "a.mp4", fps=12) == 6  # 63x47 is odd; H.264 needs even
    width, height, frames = map(int, _probe(tmp_path / "a.mp4", "stream=width,height,nb_frames"))
    assert (width, height, frames) == (64, 48, 6)  # every frame, made even by adding a row and a column


def test_save_video_rejects_unknown_formats_and_no_frames(tmp_path):
    with pytest.raises(ValueError, match=r"use \.mp4"):
        imageio.save_video(_frames(), tmp_path / "a.avi")
    with pytest.raises(ValueError, match="no frames"):
        imageio.save_video([], tmp_path / "a.gif")


def _probe(path, entries):
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", entries, "-of", "csv=p=0:s=,", str(path)],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()
    return out.split(",")


@pytest.mark.skipif(shutil.which("ffmpeg") is None, reason="needs ffmpeg")
def test_save_video_mp4_has_a_keyframe_every_second_so_it_is_seekable(tmp_path):
    imageio.save_video(iter(_frames(70, height=48, width=64)), tmp_path / "k.mp4", fps=10)
    flags = subprocess.run(
        [
            "ffprobe",
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "packet=flags",
            "-of",
            "csv=p=0",
            str(tmp_path / "k.mp4"),
        ],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split()
    assert sum("K" in f for f in flags) >= 7  # 70 frames at 10 fps = 7 s: one keyframe per second, not one per clip


@pytest.mark.skipif(shutil.which("ffmpeg") is None, reason="needs ffmpeg")
@pytest.mark.parametrize(
    ("shape", "expected"),  # shape is (height, width); the result is (width, height), rounded to even numbers
    [((120, 160), (100, 76)), ((160, 120), (76, 100)), ((60, 80), (80, 60))],
)
def test_save_video_max_side_shrinks_big_frames_only_and_keeps_the_aspect(tmp_path, shape, expected):
    imageio.save_video(iter(_frames(3, *shape)), tmp_path / "s.mp4", fps=10, max_side=100)
    assert tuple(map(int, _probe(tmp_path / "s.mp4", "stream=width,height"))) == expected


def test_save_video_gif_max_side_shrinks_frames(tmp_path):
    imageio.save_video(iter(_frames(3, 100, 200)), tmp_path / "s.gif", fps=10, max_side=50)
    with Image.open(tmp_path / "s.gif") as gif:
        assert gif.size == (50, 25)


# --- which ffmpeg: the one on the PATH, else the `[video]` extra's (imageio-ffmpeg, a dev dependency here) -----------
# PATH and IMAGEIO_FFMPEG_EXE (imageio-ffmpeg's documented override) are set through the environment; ffmpeg itself is
# the external boundary, stood in for by a script where its choice must be seen.


def is_mp4(path) -> bool:
    return path.read_bytes()[4:8] == b"ftyp"  # an MP4 file starts with its 'ftyp' box (ISO/IEC 14496-12)


@pytest.mark.skipif(sys.platform == "win32", reason="the stand-in ffmpeg is a POSIX shell script")
def test_an_ffmpeg_on_the_path_is_used_before_the_bundled_one(tmp_path, monkeypatch):
    folder = tmp_path / "bin"
    folder.mkdir()
    stand_in = folder / "ffmpeg"
    stand_in.write_text("#!/bin/sh\necho 'the ffmpeg on the PATH ran' >&2\nexit 3\n")
    stand_in.chmod(0o755)
    monkeypatch.setenv("PATH", str(folder))
    with pytest.raises(RuntimeError, match="the ffmpeg on the PATH ran"):
        imageio.save_video(iter(_frames(2)), tmp_path / "a.mp4", fps=10)


def test_without_ffmpeg_on_the_path_the_video_extras_ffmpeg_writes_the_clip(tmp_path, monkeypatch):
    monkeypatch.setenv("PATH", str(tmp_path / "empty"))
    monkeypatch.delenv("IMAGEIO_FFMPEG_EXE", raising=False)
    assert imageio.save_video(iter(_frames(6)), tmp_path / "a.mp4", fps=12) == 6
    assert is_mp4(tmp_path / "a.mp4")
    imageio.resize_video(tmp_path / "a.mp4", tmp_path / "small.mp4", 32)
    assert is_mp4(tmp_path / "small.mp4")


def test_with_no_ffmpeg_anywhere_video_names_the_extra_and_gif_still_works(tmp_path, monkeypatch):
    monkeypatch.setenv("PATH", str(tmp_path / "empty"))
    monkeypatch.setenv("IMAGEIO_FFMPEG_EXE", str(tmp_path / "no-ffmpeg-here"))
    with pytest.raises(RuntimeError, match=re.escape('pip install "photo-fx[video]"')):
        imageio.save_video(iter(_frames(2)), tmp_path / "a.mp4", fps=10)
    with pytest.raises(RuntimeError, match=re.escape("photo-fx[video]")):
        imageio.resize_video(tmp_path / "a.mp4", tmp_path / "b.mp4", 32)
    assert not (tmp_path / "a.mp4").exists()
    assert imageio.save_video(iter(_frames(3)), tmp_path / "a.gif", fps=10) == 3


# --- bring your own matte: imageio.load_matte ----------------------------------------------------------------------


def halves(height=20, width=30, low=0, high=255) -> np.ndarray:
    """uint8: `low` in the left half, `high` in the right half."""
    values = np.full((height, width), low, np.uint8)
    values[:, width // 2 :] = high
    return values


def test_a_transparent_png_gives_its_alpha_channel_whatever_its_colours(tmp_path):
    rgba = np.dstack([np.full((20, 30, 3), 200, np.uint8), halves(low=51, high=255)])
    Image.fromarray(rgba, "RGBA").save(tmp_path / "m.png")
    matte = imageio.load_matte(tmp_path / "m.png")
    assert matte.dtype == np.float32
    assert matte.shape == (20, 30)
    np.testing.assert_allclose(matte[:, :15], 0.2, atol=1e-6)  # 51 / 255
    np.testing.assert_allclose(matte[:, 15:], 1.0, atol=1e-6)


def test_a_greyscale_png_gives_its_grey_levels(tmp_path):
    Image.fromarray(halves(low=0, high=102), "L").save(tmp_path / "m.png")
    matte = imageio.load_matte(tmp_path / "m.png")
    np.testing.assert_allclose(matte[:, :15], 0.0)
    np.testing.assert_allclose(matte[:, 15:], 0.4, atol=1e-6)  # 102 / 255


def test_a_black_and_white_rgb_picture_gives_its_brightness(tmp_path):
    Image.fromarray(np.dstack([halves()] * 3), "RGB").save(tmp_path / "m.jpg", quality=100)
    matte = imageio.load_matte(tmp_path / "m.jpg")
    assert matte[:, :10].max() < 0.02
    assert matte[:, 20:].min() > 0.98


def test_a_16_bit_greyscale_png_keeps_its_full_range(tmp_path):
    levels = halves().astype(np.uint16) * 0
    levels[:, 15:] = 16384  # a quarter of 65535, rounded
    Image.fromarray(levels).save(tmp_path / "m.png")
    matte = imageio.load_matte(tmp_path / "m.png")
    np.testing.assert_allclose(matte[:, 15:], 16384 / 65535, atol=1e-6)
    np.testing.assert_allclose(matte[:, :15], 0.0)


def test_a_missing_matte_file_is_named(tmp_path):
    with pytest.raises(FileNotFoundError, match=r"nope\.png"):
        imageio.load_matte(tmp_path / "nope.png")
