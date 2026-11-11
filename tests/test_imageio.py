import struct

import numpy as np
import pytest
from PIL import Image

import bullterrier_image
from bullterrier_image import (
    BackendUnavailableError,
    InvalidImageError,
    NonFiniteImageError,
    RecipeError,
    fx,
    imageio,
)


def test_save_load_roundtrip_within_8_bit_precision(photo, tmp_path):
    imageio.save(photo, tmp_path / "a.png")
    back = imageio.load(tmp_path / "a.png")
    assert back.dtype == np.float32
    assert back.shape == photo.shape
    assert np.abs(back - photo).max() <= 1 / 255


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


@pytest.mark.parametrize(
    ("shape", "side", "expected"), [((40, 80, 3), 20, (10, 20, 3)), ((30, 20, 4), 60, (60, 40, 4))]
)
def test_resize_gives_the_picture_a_longest_side_and_keeps_its_channels(shape, side, expected):
    picture = np.random.default_rng(2).random(shape, dtype=np.float32)
    out = imageio.resize(picture, side)
    assert out.shape == expected
    assert out.dtype == np.float32
    assert out.min() >= 0
    assert out.max() <= 1


def test_resize_returns_a_picture_of_that_size_as_it_is_and_refuses_what_is_no_picture():
    picture = np.random.default_rng(2).random((12, 16, 3), dtype=np.float32)
    np.testing.assert_array_equal(imageio.resize(picture, 16), picture)
    with pytest.raises(InvalidImageError, match="array"):
        imageio.resize([[1.0]], 16)  # type: ignore[arg-type]  # the refusal is the point


def test_palette_gives_hex_strings_largest_share_first_that_gradient_map_accepts():
    picture = np.zeros((10, 4, 3), np.float32)
    picture[:6] = (1.0, 0.0, 0.0)  # 6 rows red, 3 green, 1 blue
    picture[6:9] = (0.0, 1.0, 0.0)
    picture[9:] = (0.0, 0.0, 1.0)
    stops = imageio.palette(picture, 3)
    assert stops == ["#ff0000", "#00ff00", "#0000ff"]
    assert imageio.palette(picture, 3) == stops  # the same seed, the same list
    fx.gradient_map(stops=stops)  # a step is made: the core accepts what palette gives
    assert bullterrier_image.palette is imageio.palette


@pytest.mark.parametrize("colors", [0, -2, 257])
def test_palette_refuses_a_number_of_colours_outside_1_to_256_naming_the_value(colors):
    with pytest.raises(RecipeError, match=rf"colors.*{colors}"):
        imageio.palette(np.zeros((2, 2, 3), np.float32), colors)


def test_palette_refuses_what_is_no_picture():
    with pytest.raises(InvalidImageError, match="array"):
        imageio.palette([[1.0]])  # type: ignore[arg-type]  # the refusal is the point


@pytest.mark.parametrize(("seed", "shown"), [(-1, "-1"), (2**32, "4294967296"), (2**70, "1.18059")])
def test_palette_refuses_a_seed_outside_0_to_4294967295_naming_the_value(seed, shown):
    with pytest.raises(RecipeError, match=rf"seed.*{shown}"):
        imageio.palette(np.zeros((2, 2, 3), np.float32), 1, seed=seed)


@pytest.mark.parametrize("seed", [0, 2**32 - 1])
def test_palette_takes_the_smallest_and_largest_seed(seed):
    assert imageio.palette(np.zeros((2, 2, 3), np.float32), 1, seed=seed) == ["#000000"]
