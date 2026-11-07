import struct

import numpy as np
import pytest
from PIL import Image

from bullterrier_image import BackendUnavailableError, InvalidImageError, NonFiniteImageError, imageio


def test_save_load_roundtrip_within_8_bit_precision(photo, tmp_path):
    imageio.save(photo, tmp_path / "a.png")
    back = imageio.load(tmp_path / "a.png")
    assert back.dtype == np.float32
    assert back.shape == photo.shape
    assert np.abs(back - photo).max() <= 1 / 255


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


def palette_over_white(**info) -> Image.Image:
    return palette_row([0, 1], [(255, 0, 0), (0, 0, 255)], **info)


FROM_PIL = [  # (picture, expected RGB per pixel, by hand): what `from_pil` gives for a Pillow image of each 8-bit mode
    pytest.param(row([(255, 0, 0), (0, 102, 255)], np.uint8), [(1, 0, 0), (0, 0.4, 1)], id="RGB"),
    pytest.param(row([0, 51], np.uint8), [(0, 0, 0), (0.2, 0.2, 0.2)], id="L"),
    pytest.param(row([False, True], bool), [0, 1], id="1-bit"),
    pytest.param(palette_over_white(), [(1, 0, 0), (0, 0, 1)], id="P"),
    pytest.param(palette_over_white(transparency=0), [1, (0, 0, 1)], id="P, transparent index over white"),
    pytest.param(
        row([(255, 0, 0, 0), (255, 0, 0, 128), (0, 0, 255, 255)], np.uint8, "RGBA"),
        [1, (1, 127 / 255, 127 / 255), (0, 0, 1)],
        id="RGBA over white",
    ),
    pytest.param(row([(0, 0), (0, 51)], np.uint8, "LA"), [1, 0.8], id="LA over white"),
    pytest.param(  # alpha 51 = 0.2: the straight grey is premultiplied 10 * 5 = 50, so the sum is exact
        Image.frombytes("La", (2, 1), bytes([0, 0, 10, 51])),
        [1, (10 / 255 + 1 - 51 / 255,) * 3],
        id="La, premultiplied",
    ),
    pytest.param(
        Image.frombytes("RGBa", (2, 1), bytes([0, 0, 0, 0, 10, 0, 20, 51])),
        [1, (10 / 255 + 1 - 51 / 255, 1 - 51 / 255, 20 / 255 + 1 - 51 / 255)],
        id="RGBa, premultiplied",
    ),
]  # fmt: skip


@pytest.mark.parametrize(("picture", "expected"), FROM_PIL)
def test_from_pil_gives_the_8_bit_levels_with_transparency_over_white(picture, expected):
    rgb = imageio.from_pil(picture)
    assert rgb.dtype == np.float32
    assert rgb.shape == (1, len(expected), 3)
    np.testing.assert_allclose(rgb[0], [np.broadcast_to(value, 3) for value in expected], atol=1e-6)


def test_from_pil_applies_no_colour_profile_where_load_does(tmp_path):
    grey = Image.fromarray(np.array([[128]], np.uint8))
    grey.save(tmp_path / "linear.png", icc_profile=linear_grey_profile())
    with Image.open(tmp_path / "linear.png") as opened:
        np.testing.assert_allclose(imageio.from_pil(opened), 128 / 255, atol=1e-6)  # as stored
    assert imageio.load(tmp_path / "linear.png")[0, 0, 0] > 0.7  # load applies the profile


def test_from_pil_clips_a_16_bit_level_it_does_not_support_instead_of_refusing_it():
    """Documented limit: Pillow's conversion saturates 16-bit levels at white, so use `load` for them."""
    half = row([32768], np.uint16)
    np.testing.assert_allclose(imageio.from_pil(half), 1.0)


def test_the_colour_under_a_transparent_pixel_never_shows(tmp_path):
    hidden = np.zeros((4, 6, 4), np.uint8)
    hidden[..., :3] = np.random.default_rng(3).integers(0, 256, (4, 6, 3))  # any colour, alpha 0 everywhere
    Image.fromarray(hidden, "RGBA").save(tmp_path / "clear.png")
    np.testing.assert_array_equal(imageio.load(tmp_path / "clear.png"), np.ones((4, 6, 3), np.float32))


def test_a_picture_keeps_its_transparency_instead_of_taking_white_paper_under_it(tmp_path):
    pixels = np.array([[(255, 0, 0, 255), (0, 0, 255, 128), (10, 20, 30, 0)]], np.uint8)
    Image.fromarray(pixels, "RGBA").save(tmp_path / "logo.png")
    picture = imageio.load_picture(tmp_path / "logo.png")
    assert picture.dtype == np.float32
    assert picture.shape == (1, 3, 4)
    np.testing.assert_allclose(picture[0, 0], [1, 0, 0, 1], atol=1e-6)
    np.testing.assert_allclose(picture[0, 1], [0, 0, 1, 128 / 255], atol=1e-6)  # straight alpha: colour untouched
    assert picture[0, 2, 3] == 0


def test_an_opaque_picture_loads_with_alpha_one_and_the_colours_load_gives(photo, tmp_path):
    imageio.save(photo, tmp_path / "a.png")
    picture = imageio.load_picture(tmp_path / "a.png")
    np.testing.assert_array_equal(picture[..., :3], imageio.load(tmp_path / "a.png"))
    np.testing.assert_array_equal(picture[..., 3], 1.0)


# --- colour profiles: converted to sRGB from the picture's own mode -----------------------------------------------
# Real CMYK and greyscale profiles are large licensed files, so the tests build minimal ICC v2 profiles (ICC.1:2001)
# by hand. The expected values come from the standards, not from littlecms: CIE L* to luminance Y (CIE 15) and the
# sRGB transfer function (IEC 61966-2-1).

D50 = (0.9642, 1.0, 0.8249)  # the ICC profile connection space white point


def s15fixed16(value: float) -> bytes:
    return struct.pack(">i", round(value * 65536))


def minimal_profile(device_class: bytes, colour_space: bytes, pcs: bytes, tag: bytes, body: bytes) -> bytes:
    """An ICC v2 profile with a white point and one transform tag: the 128-byte header, the tag table, the tags."""
    tags = {b"wtpt": b"XYZ " + bytes(4) + b"".join(map(s15fixed16, D50)), tag: body}
    start = 128 + 4 + 12 * len(tags)
    table, data = b"", b""
    for signature, content in tags.items():
        padded = content + bytes(-len(content) % 4)
        table += signature + struct.pack(">II", start + len(data), len(padded))
        data += padded
    header = (
        struct.pack(">I", start + len(data)) + bytes(4) + bytes([2, 0x10, 0, 0]) + device_class + colour_space + pcs
        + bytes(12) + b"acsp" + bytes(28) + b"".join(map(s15fixed16, D50)) + bytes(48)
    )  # fmt: skip
    return header + struct.pack(">I", len(tags)) + table + data


def linear_grey_profile() -> bytes:
    """A greyscale profile whose levels are linear light (tone curve gamma 1.0)."""
    curve = b"curv" + bytes(4) + struct.pack(">IH", 1, 256)  # one entry: gamma as u8Fixed8, 1.0
    return minimal_profile(b"mntr", b"GRAY", b"XYZ ", b"kTRC", curve)


# --- animations and unreadable files ----------------------------------------------------------------------------


@pytest.mark.parametrize("name", ["cut.jpg", "cut.jpeg", "CUT.JPG"])
def test_a_picture_with_transparency_cannot_be_saved_as_a_jpeg(photo, subject, tmp_path, name):
    with pytest.raises(BackendUnavailableError, match="transparen"):
        imageio.save(np.dstack([photo, subject]), tmp_path / name)
    assert not (tmp_path / name).exists()
    imageio.save(photo, tmp_path / name)  # known good: the same picture without its alpha
    assert (tmp_path / name).exists()


# --- what saved files carry: an sRGB profile, nothing from the input's metadata ------------------------------------


def test_save_names_a_file_type_it_cannot_write_and_writes_a_known_one(tmp_path):
    rgb = np.full((4, 5, 3), 0.5, np.float32)
    with pytest.raises(BackendUnavailableError, match=r"\.xyz"):
        imageio.save(rgb, tmp_path / "a.xyz")
    assert not (tmp_path / "a.xyz").exists()
    imageio.save(rgb, tmp_path / "a.png")  # the known-good side
    assert imageio.load(tmp_path / "a.png").shape == (4, 5, 3)


@pytest.mark.parametrize(
    ("levels", "top"),
    [
        pytest.param(np.uint8, 255, id="8-bit levels are divided by 255"),
        pytest.param(np.uint16, 65535, id="16-bit levels are divided by 65535"),
    ],
)
def test_a_picture_of_whole_levels_saves_as_its_levels_over_the_types_largest(tmp_path, levels, top):
    picture = (np.arange(4 * 5 * 3).reshape(4, 5, 3) * (top // 59)).astype(levels)
    imageio.save(picture, tmp_path / "levels.png")
    imageio.save((picture / np.float64(top)).astype(np.float32), tmp_path / "floats.png")
    np.testing.assert_array_equal(imageio.load(tmp_path / "levels.png"), imageio.load(tmp_path / "floats.png"))


@pytest.mark.parametrize(
    ("picture", "complaint"),
    [
        pytest.param(np.full((2, 3, 3), 1.25, np.float32), r"max 1\.25", id="above 1 names the largest, not clipped"),
        pytest.param(np.full((2, 3, 3), -0.5, np.float32), r"min -0\.5", id="below 0 names the least, not clipped"),
        pytest.param(np.zeros((2, 3), np.float32), r"np\.dstack", id="greyscale names the fix"),
        pytest.param(np.zeros((2, 3, 3), bool), "bool", id="bool is no colour level"),
        pytest.param(Image.new("RGB", (3, 2)), r"np\.asarray", id="a Pillow image names the fix"),
    ],
)
def test_saving_refuses_what_is_no_picture_naming_it_and_writes_nothing(tmp_path, picture, complaint):
    with pytest.raises(InvalidImageError, match=complaint):
        imageio.save(picture, tmp_path / "bad.png")
    assert not (tmp_path / "bad.png").exists()


@pytest.mark.parametrize("bad", [np.nan, np.inf, -np.inf], ids=["nan", "infinity", "negative infinity"])
def test_saving_refuses_nan_and_infinity_naming_the_pixel_and_writes_nothing(tmp_path, bad):
    rgb = np.full((2, 4, 3), 0.4, np.float32)
    rgb[1, 2, 1] = bad
    with pytest.raises(NonFiniteImageError, match="row 1, column 2"):
        imageio.save(rgb, tmp_path / "odd.png")
    assert not (tmp_path / "odd.png").exists()
