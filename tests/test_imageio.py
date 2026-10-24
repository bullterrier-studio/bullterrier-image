import io
import itertools
import struct
import warnings

import numpy as np
import pytest
from PIL import Image, ImageCms

from bullterrier_image import BackendUnavailableError, ImageReadError, NonFiniteImageError, imageio


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


def test_a_picture_is_upright_and_in_srgb_as_a_photo_is(tmp_path):
    grey = Image.fromarray(np.array([[(128, 255), (128, 0)]], np.uint8), "LA")
    grey.save(tmp_path / "linear.png", icc_profile=linear_grey_profile())
    picture = imageio.load_picture(tmp_path / "linear.png")
    np.testing.assert_allclose(picture[0, 0, :3], srgb_encode(128 / 255), atol=1 / 255)  # the profile is applied
    np.testing.assert_allclose(picture[0, :, 3], [1.0, 0.0], atol=1e-6)
    img = Image.new("RGB", (40, 20), "red")
    exif = Image.Exif()
    exif[0x0112] = 6
    img.save(tmp_path / "r.jpg", exif=exif)
    assert imageio.load_picture(tmp_path / "r.jpg").shape == (40, 20, 4)


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


def cmyk_profile() -> bytes:
    """A CMYK profile: no ink is white (L* 100), any K is black (L* 0), any other ink is neutral grey L* 128/255*100.

    One 8-bit lookup table (lut8, 2 grid points per ink) from CMYK to CIELAB; a lut8 encodes L* as byte / 255 * 100
    and a*, b* as byte - 128.
    """
    ramp = bytes(range(256))
    grid = b""
    for c, m, y, k in itertools.product((0, 1), repeat=4):  # the first ink varies slowest
        lightness = 0 if k else 255 if not (c or m or y) else 128
        grid += bytes([lightness, 128, 128])
    identity = b"".join(map(s15fixed16, (1, 0, 0, 0, 1, 0, 0, 0, 1)))
    lut = b"mft1" + bytes(4) + bytes([4, 3, 2, 0]) + identity + ramp * 4 + grid + ramp * 3
    return minimal_profile(b"prtr", b"CMYK", b"Lab ", b"A2B0", lut)


def srgb_encode(linear: float) -> float:
    """The sRGB transfer function (IEC 61966-2-1): linear light to the encoded value, both 0..1."""
    return 12.92 * linear if linear <= 0.0031308 else 1.055 * linear ** (1 / 2.4) - 0.055


def luminance(lightness: float) -> float:
    """CIE L* (0..100) to relative luminance Y (0..1), CIE 15 (L* above 8)."""
    return ((lightness + 16) / 116) ** 3


def test_a_greyscale_profile_is_applied_to_the_grey_levels(tmp_path):
    grey = Image.fromarray(np.array([[128, 0]], np.uint8))
    grey.save(tmp_path / "linear.png", icc_profile=linear_grey_profile())
    rgb = imageio.load(tmp_path / "linear.png")
    np.testing.assert_allclose(rgb[0, 0], srgb_encode(128 / 255), atol=1 / 255)  # 0.735, not 0.502 as stored
    np.testing.assert_allclose(rgb[0, 1], 0.0, atol=1 / 255)


def test_a_greyscale_profile_is_applied_under_transparency_too(tmp_path):
    grey = Image.fromarray(np.array([[(128, 255), (128, 0)]], np.uint8), "LA")
    grey.save(tmp_path / "linear.png", icc_profile=linear_grey_profile())
    rgb = imageio.load(tmp_path / "linear.png")
    np.testing.assert_allclose(rgb[0, 0], srgb_encode(128 / 255), atol=1 / 255)
    np.testing.assert_allclose(rgb[0, 1], 1.0, atol=1e-6)  # transparent: white paper


@pytest.mark.parametrize("suffix", [".tif", ".jpg"])
def test_a_cmyk_profile_is_applied_to_the_inks_not_a_naive_conversion(tmp_path, suffix):
    inks = np.array([[(0, 255, 255, 0), (0, 0, 0, 0), (0, 0, 0, 255)]], np.uint8)  # naively red, white, black
    Image.fromarray(np.repeat(inks, 8, axis=1), "CMYK").save(tmp_path / f"cmyk{suffix}", icc_profile=cmyk_profile())
    rgb = imageio.load(tmp_path / f"cmyk{suffix}")
    grey = srgb_encode(luminance(128 / 255 * 100))  # 0.468: the profile's grey
    np.testing.assert_allclose(rgb[0, 4], grey, atol=2 / 255)
    np.testing.assert_allclose(rgb[0, 12], 1.0, atol=2 / 255)
    np.testing.assert_allclose(rgb[0, 20], 0.0, atol=2 / 255)


@pytest.mark.parametrize(
    "profile",
    [b"not a profile at all", linear_grey_profile()],
    ids=["unreadable", "for another colour space"],
)
def test_a_broken_or_mismatched_profile_is_ignored_quietly_and_the_pixels_taken_as_srgb(tmp_path, capfd, profile):
    stored = np.array([[(255, 0, 0), (0, 102, 255)]], np.uint8)
    Image.fromarray(stored).save(tmp_path / "odd.png", icc_profile=profile)
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        rgb = imageio.load(tmp_path / "odd.png")
    np.testing.assert_allclose(rgb, stored / 255, atol=1e-6)
    assert capfd.readouterr() == ("", "")  # neither Python nor littlecms printed anything


# --- animations and unreadable files ----------------------------------------------------------------------------


@pytest.mark.parametrize("suffix", [".gif", ".png"], ids=["animated GIF", "APNG"])
def test_an_animation_loads_as_its_first_frame(tmp_path, suffix):
    red, blue = Image.new("RGB", (4, 3), (255, 0, 0)), Image.new("RGB", (4, 3), (0, 0, 255))
    red.save(tmp_path / f"anim{suffix}", save_all=True, append_images=[blue], duration=100, loop=0)
    with Image.open(tmp_path / f"anim{suffix}") as written:
        assert getattr(written, "n_frames", 1) == 2  # the file really is an animation
    np.testing.assert_array_equal(imageio.load(tmp_path / f"anim{suffix}"), np.tile([1, 0, 0], (3, 4, 1)))


def truncated_png(path):
    Image.fromarray(np.random.default_rng(0).integers(0, 256, (64, 64, 3), np.uint8)).save(path)
    path.write_bytes(path.read_bytes()[:400])


UNREADABLE = [
    pytest.param(lambda path: None, id="missing"),
    pytest.param(lambda path: path.write_text("just some text", encoding="utf-8"), id="not an image"),
    pytest.param(truncated_png, id="truncated"),
    pytest.param(lambda path: path.mkdir(), id="a folder"),
]


@pytest.mark.parametrize("make", UNREADABLE)
@pytest.mark.parametrize(
    "read", [imageio.load, imageio.load_matte, imageio.load_picture], ids=["load", "load_matte", "load_picture"]
)
def test_a_file_that_cannot_be_read_as_an_image_raises_image_read_error_naming_it(tmp_path, read, make):
    path = tmp_path / "broken-photo.png"
    make(path)
    with pytest.raises(ImageReadError, match=r"broken-photo\.png"):
        read(path)


def test_a_decompression_bomb_is_refused_with_image_read_error(tmp_path, monkeypatch):
    Image.new("RGB", (30, 20)).save(tmp_path / "bomb.png")  # 600 px
    monkeypatch.setattr(Image, "MAX_IMAGE_PIXELS", 100)  # Pillow's own limit, lowered: refused above twice the limit
    with pytest.raises(ImageReadError, match=r"bomb\.png"):
        imageio.load(tmp_path / "bomb.png")
    monkeypatch.setattr(Image, "MAX_IMAGE_PIXELS", 600)  # the same file within the limit loads
    assert imageio.load(tmp_path / "bomb.png").shape == (20, 30, 3)


@pytest.mark.parametrize("name", ["cut.png", "cut.webp"])
def test_save_of_four_channels_writes_the_last_as_transparency(photo, matte, tmp_path, name):
    imageio.save(np.dstack([photo, matte]), tmp_path / name, quality=100)
    with Image.open(tmp_path / name) as img:
        assert img.mode == "RGBA"
        alpha = np.asarray(img)[..., 3]
        assert alpha[0, 0] == 0
        assert alpha[48, 64] == 255


@pytest.mark.parametrize("name", ["cut.jpg", "cut.jpeg", "CUT.JPG"])
def test_a_picture_with_transparency_cannot_be_saved_as_a_jpeg(photo, matte, tmp_path, name):
    with pytest.raises(BackendUnavailableError, match="transparen"):
        imageio.save(np.dstack([photo, matte]), tmp_path / name)
    assert not (tmp_path / name).exists()
    imageio.save(photo, tmp_path / name)  # known good: the same picture without its alpha
    assert (tmp_path / name).exists()


# --- what saved files carry: an sRGB profile, nothing from the input's metadata ------------------------------------

GPS_INFO = 0x8825  # the EXIF tag pointing to the GPS block (EXIF 2.32)


@pytest.mark.parametrize("name", ["a.jpg", "a.png", "a.webp", "cut.png"])
def test_a_saved_still_says_it_is_srgb_with_a_small_embedded_profile(photo, matte, tmp_path, name):
    imageio.save(np.dstack([photo, matte]) if name.startswith("cut") else photo, tmp_path / name)
    with Image.open(tmp_path / name) as img:
        profile = img.info["icc_profile"]
    assert "sRGB" in ImageCms.getProfileDescription(ImageCms.ImageCmsProfile(io.BytesIO(profile)))
    assert len(profile) < 1024  # small: under a kilobyte per picture


def test_a_saved_picture_never_carries_the_inputs_exif_or_gps(tmp_path):
    exif = Image.Exif()
    exif[0x010F] = "PhoneMaker"  # Make
    exif.get_ifd(GPS_INFO).update({1: "N", 2: (51.0, 30.0, 0.0), 3: "W", 4: (0.0, 7.0, 0.0)})
    Image.new("RGB", (16, 12), (200, 120, 40)).save(tmp_path / "phone.jpg", exif=exif)
    with Image.open(tmp_path / "phone.jpg") as original:
        assert original.getexif().get_ifd(GPS_INFO)[1] == "N"  # the input really is geotagged
    photo = imageio.load(tmp_path / "phone.jpg")
    for name in ("out.jpg", "out.png", "out.webp"):
        imageio.save(photo, tmp_path / name)
        with Image.open(tmp_path / name) as saved:
            assert dict(saved.getexif()) == {}
            assert "exif" not in saved.info
        assert b"PhoneMaker" not in (tmp_path / name).read_bytes()


def test_a_jpeg_keeps_full_resolution_colour(tmp_path):
    """Columns one pixel wide alternate red and blue: chroma subsampling (4:2:0 or 4:2:2) would average each pair of
    columns into one purple, so every pixel would be off by about half the range."""
    stripes = np.zeros((16, 16, 3), np.float32)
    stripes[:, 0::2, 0] = 1.0
    stripes[:, 1::2, 2] = 1.0
    imageio.save(stripes, tmp_path / "stripes.jpg", quality=100)
    assert np.abs(imageio.load(tmp_path / "stripes.jpg") - stripes).max() < 0.1


def test_a_jpeg_honours_its_quality(photo, tmp_path):
    def error_and_size(quality: int) -> tuple[float, int]:
        path = tmp_path / f"q{quality}.jpg"
        imageio.save(photo, path, quality=quality)
        return float(np.abs(imageio.load(path) - photo).mean()), path.stat().st_size

    (rough, small), (fine, large) = error_and_size(20), error_and_size(95)
    assert fine < rough / 2  # a higher quality is closer to the picture...
    assert large > small  # ...and spends more bytes on it


ICC_FIXED_DATE = bytes.fromhex(
    "07d0 0001 0001 0000 0000 0000"
)  # ICC.1 header bytes 24..35: 2000-01-01 00:00:00 as 6 u16


@pytest.mark.parametrize("name", ["a.jpg", "a.png", "a.webp"])
def test_the_embedded_profile_carries_a_fixed_date_not_the_time_of_saving(photo, tmp_path, name):
    """A file saved twice is the same bytes only if no clock reading enters it; the profile's header has a date."""
    imageio.save(photo, tmp_path / name)
    with Image.open(tmp_path / name) as img:
        assert img.info["icc_profile"][24:36] == ICC_FIXED_DATE


def test_save_names_a_file_type_it_cannot_write_and_writes_a_known_one(tmp_path):
    rgb = np.full((4, 5, 3), 0.5, np.float32)
    with pytest.raises(BackendUnavailableError, match=r"\.xyz"):
        imageio.save(rgb, tmp_path / "a.xyz")
    assert not (tmp_path / "a.xyz").exists()
    imageio.save(rgb, tmp_path / "a.png")  # the known-good side
    assert imageio.load(tmp_path / "a.png").shape == (4, 5, 3)


@pytest.mark.parametrize("bad", [np.nan, np.inf, -np.inf], ids=["nan", "infinity", "negative infinity"])
def test_saving_refuses_nan_and_infinity_naming_the_pixel_and_writes_nothing(tmp_path, bad):
    rgb = np.full((2, 4, 3), 0.4, np.float32)
    rgb[1, 2, 1] = bad
    with pytest.raises(NonFiniteImageError, match="row 1, column 2"):
        imageio.save(rgb, tmp_path / "odd.png")
    assert not (tmp_path / "odd.png").exists()


# --- bring your own matte: imageio.load_matte ----------------------------------------------------------------------


def halves(low, high, dtype=np.uint8) -> np.ndarray:
    """A 20x30 picture: `low` in the left half, `high` in the right half."""
    values = np.full((20, 30), low, dtype)
    values[:, 15:] = high
    return values


PNG, JPEG = ("m.png", 0, 1e-6), ("m.jpg", 5, 0.02)  # file, columns left unchecked beside the middle, tolerance


@pytest.mark.parametrize(
    ("picture", "file", "left", "right"),
    [
        # a transparent PNG gives its alpha channel, whatever its colours: 51 / 255 and 255 / 255
        (Image.fromarray(np.dstack([np.full((20, 30, 3), 200, np.uint8), halves(51, 255)]), "RGBA"), PNG, 0.2, 1.0),
        (Image.fromarray(halves(0, 102), "L"), PNG, 0.0, 0.4),  # a greyscale PNG gives its grey levels
        (Image.fromarray(halves(0, 16384, np.uint16)), PNG, 0.0, 16384 / 65535),  # 16 bits keep their full range
        # a black and white RGB picture gives its brightness (a JPEG: within its rounding, away from the edge)
        (Image.fromarray(np.dstack([halves(0, 255)] * 3), "RGB"), JPEG, 0.0, 1.0),
    ],
    ids=["alpha of a transparent PNG", "greyscale PNG", "16-bit greyscale PNG", "black and white RGB JPEG"],
)
def test_a_matte_file_gives_its_subject_share_per_pixel(tmp_path, picture, file, left, right):
    name, margin, atol = file
    picture.save(tmp_path / name, quality=100)  # (PNG has no quality setting and ignores it)
    matte = imageio.load_matte(tmp_path / name)
    assert matte.dtype == np.float32
    assert matte.shape == (20, 30)
    np.testing.assert_allclose(matte[:, : 15 - margin], left, atol=atol)
    np.testing.assert_allclose(matte[:, 15 + margin :], right, atol=atol)
