import shutil
import subprocess

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
