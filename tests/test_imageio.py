import numpy as np
from PIL import Image

from portrait_fx import imageio


def test_save_load_roundtrip_within_8_bit_precision(photo, tmp_path):
    imageio.save(photo, tmp_path / "a.png")
    back = imageio.load(tmp_path / "a.png")
    assert back.dtype == np.float32 and back.shape == photo.shape
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
        assert img.getpixel((0, 0))[3] == 0 and img.getpixel((64, 48))[3] == 255


def test_resize_keeps_aspect_ratio_and_range(photo, matte):
    small = imageio.resize(photo, 64)
    assert small.shape == (48, 64, 3)
    assert 0.0 <= small.min() and small.max() <= 1.0
    assert imageio.resize(matte, 64).shape == (48, 64)


def test_resize_to_same_size_is_a_no_op(photo):
    assert imageio.resize(photo, 128) is photo
