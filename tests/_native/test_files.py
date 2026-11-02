"""The Rust extension's image files, `_native._core.read`, `read_map` and `write`, against Pillow as an independent
oracle: pictures Pillow writes are read by the core and compared with what Pillow decodes of them, and pictures the core
writes are decoded by Pillow. Lossless files must agree exactly; a JPEG decodes within a few levels of Pillow (the two
decoders upsample and transform with slightly different arithmetic).
"""

import numpy as np
import pytest
from PIL import Image, ImageOps

from bullterrier_image._native import _core  # noqa: PLC2701  architecture: the boundary imageio calls
from bullterrier_image.core import native
from bullterrier_image.errors import BackendUnavailableError, ImageReadError, ImageWriteError

JPEG_LEVELS = 6  # a lossy decode may differ from Pillow's by this many 8-bit levels: IDCT and chroma upsampling differ


def pillow_rgb(path) -> np.ndarray:
    """What Pillow decodes of `path`, upright, as float32 (h, w, 3) levels 0..1, a transparent file over nothing."""
    with Image.open(path) as opened:
        img = ImageOps.exif_transpose(opened)
        return np.asarray(img.convert("RGB"), np.float32) / 255


def blocks() -> np.ndarray:
    """A 20x30 RGB picture of four flat colour blocks and a gradient strip: flat areas keep a JPEG's error small and
    make every orientation distinguishable."""
    img = np.zeros((20, 30, 3), np.uint8)
    img[:10, :15] = (250, 10, 10)
    img[:10, 15:] = (10, 250, 10)
    img[10:, :15] = (10, 10, 250)
    img[10:, 15:] = (240, 240, 20)
    img[16:, 15:] = np.linspace(0, 255, 15, dtype=np.uint8)[:, None]
    return img


@pytest.mark.parametrize("name", ["a.png", "a.webp", "a.tif", "a.bmp"])
def test_a_lossless_file_reads_as_pillow_decodes_it(tmp_path, name):
    Image.fromarray(blocks()).save(tmp_path / name, lossless=True)  # the option only WebP reads
    levels = pillow_rgb(tmp_path / name)
    got = _core.read(tmp_path / name)
    assert got.dtype == np.float32
    assert got.flags.c_contiguous
    np.testing.assert_allclose(got, levels, atol=1e-6)


def test_a_jpeg_reads_within_a_few_levels_of_pillow(tmp_path, photo):
    Image.fromarray((photo * 255 + 0.5).astype(np.uint8)).save(tmp_path / "a.jpg", quality=90)
    difference = np.abs(_core.read(tmp_path / "a.jpg") - pillow_rgb(tmp_path / "a.jpg")) * 255
    assert difference.max() <= JPEG_LEVELS


def test_a_gif_reads_with_an_alpha_channel_of_ones_when_no_entry_is_transparent(tmp_path):
    Image.fromarray(blocks()).save(tmp_path / "a.gif")  # the core counts a GIF as able to be transparent
    got = _core.read(tmp_path / "a.gif")
    np.testing.assert_allclose(got[..., :3], pillow_rgb(tmp_path / "a.gif"), atol=1e-6)
    np.testing.assert_array_equal(got[..., 3], 1)


GREYS = [  # (mode, pixels, dtype, expected level of each pixel)
    pytest.param("L", [0, 51, 204, 255], np.uint8, [0, 0.2, 0.8, 1], id="8-bit grey over 255"),
    pytest.param("1", [False, True], bool, [0, 1], id="1-bit"),
    pytest.param("I;16", [0, 16384, 65535], np.uint16, [0, 16384 / 65535, 1], id="16-bit grey over 65535"),
]


@pytest.mark.parametrize(("mode", "pixels", "dtype", "expected"), GREYS)
def test_a_grey_png_reads_as_equal_channels_at_its_level_over_its_maximum(tmp_path, mode, pixels, dtype, expected):
    Image.fromarray(np.array([pixels], dtype)).save(tmp_path / "g.png")
    got = _core.read(tmp_path / "g.png")
    np.testing.assert_allclose(got, np.repeat(np.array([expected], np.float32)[..., None], 3, axis=2), atol=1e-6)


@pytest.mark.parametrize(
    ("pixels", "dtype", "expected"),
    [
        pytest.param([0, 32768, 65535], np.int32, [0, 32768 / 65535, 1], id="32-bit integers over 65535"),
        pytest.param([0.25, 1.5, -0.5], np.float32, [0.25, 1, 0], id="floats, clipped"),
    ],
)
def test_a_tiff_of_wide_levels_reads_as_grey_clipped_to_0_to_1(tmp_path, pixels, dtype, expected):
    Image.fromarray(np.array([pixels], dtype)).save(tmp_path / "w.tif")
    got = _core.read(tmp_path / "w.tif")
    np.testing.assert_allclose(got[..., 0], [expected], atol=1e-6)
    np.testing.assert_array_equal(got[..., 0], got[..., 2])


def test_a_palette_png_reads_as_its_colours_and_a_transparent_entry_gives_alpha(tmp_path):
    img = Image.new("P", (2, 1))
    img.putpalette([0, 255, 0, 255, 0, 0])
    img.putdata([0, 1])
    img.save(tmp_path / "opaque.png")
    img.save(tmp_path / "clear.png", transparency=0)
    np.testing.assert_allclose(_core.read(tmp_path / "opaque.png"), [[[0, 1, 0], [1, 0, 0]]], atol=1e-6)
    np.testing.assert_allclose(_core.read(tmp_path / "clear.png"), [[[0, 1, 0, 0], [1, 0, 0, 1]]], atol=1e-6)


def test_a_file_with_transparency_reads_four_channels_with_straight_alpha(tmp_path):
    rgba = np.array([[(255, 0, 0, 0), (255, 0, 0, 128), (0, 0, 255, 255)]], np.uint8)
    Image.fromarray(rgba, "RGBA").save(tmp_path / "a.png")
    np.testing.assert_allclose(_core.read(tmp_path / "a.png"), rgba / 255, atol=1e-6)


@pytest.mark.parametrize("orientation", range(1, 9))
def test_every_exif_orientation_reads_upright_as_pillow_shows_it(tmp_path, orientation):
    exif = Image.Exif()
    exif[0x0112] = orientation
    Image.fromarray(blocks()).save(tmp_path / "o.jpg", quality=100, subsampling=0, exif=exif)
    levels = pillow_rgb(tmp_path / "o.jpg")
    got = _core.read(tmp_path / "o.jpg")
    assert got.shape == levels.shape
    assert (np.abs(got - levels) * 255).max() <= JPEG_LEVELS


@pytest.mark.parametrize(
    ("make", "expected"),
    [
        pytest.param(lambda: np.array([[0, 128, 255]], np.uint8), [0, 128 / 255, 1], id="grey"),
        pytest.param(lambda: np.array([[(255, 255, 255, 0), (0, 0, 0, 255)]], np.uint8), [0, 1], id="alpha from RGBA"),
        pytest.param(lambda: np.array([[(0, 0, 0, 255), (255, 255, 255, 255)]], np.uint8), [0, 1], id="brightness"),
        pytest.param(lambda: np.array([[0, 16384, 65535]], np.uint16), [0, 16384 / 65535, 1], id="16-bit keeps range"),
    ],
)
def test_a_map_reads_as_its_alpha_when_it_has_transparency_else_its_brightness(tmp_path, make, expected):
    Image.fromarray(make()).save(tmp_path / "m.png")
    got = _core.read_map(tmp_path / "m.png")
    assert got.dtype == np.float32
    np.testing.assert_allclose(got, [expected], atol=1e-6)


def test_a_map_of_an_opaque_alpha_channel_reads_as_brightness(tmp_path):
    rgba = np.array([[(0, 0, 0, 255), (255, 255, 255, 255)]], np.uint8)
    Image.fromarray(rgba, "RGBA").save(tmp_path / "m.png")
    np.testing.assert_allclose(_core.read_map(tmp_path / "m.png"), [[0, 1]], atol=1e-6)


@pytest.mark.parametrize("dtype", [np.float32, np.int32])
def test_a_map_of_levels_with_no_zero_to_one_is_refused_naming_the_mode(tmp_path, dtype):
    Image.fromarray(np.array([[0, 70000]], dtype)).save(tmp_path / "m.tif")
    with pytest.raises(ValueError, match=r"mode '(F|I)'"):
        _core.read_map(tmp_path / "m.tif")


def broken(path, how):
    if how == "text":
        path.write_text("just some text", encoding="utf-8")
    elif how == "folder":
        path.mkdir()
    elif how == "truncated":
        Image.fromarray(np.random.default_rng(0).integers(0, 256, (64, 64, 3), np.uint8)).save(path)
        path.write_bytes(path.read_bytes()[:400])


@pytest.mark.parametrize("how", ["missing", "text", "folder", "truncated"])
@pytest.mark.parametrize("function", [native.read, native.read_map])
def test_a_file_that_cannot_be_read_is_an_image_read_error_naming_the_path(tmp_path, function, how):
    path = tmp_path / "broken-photo.png"
    broken(path, how)
    with pytest.raises(ImageReadError, match=r"broken-photo\.png"):
        function(path)


def test_a_path_is_taken_as_a_string_or_a_path_object(tmp_path):
    Image.fromarray(blocks()).save(tmp_path / "a.png")
    np.testing.assert_array_equal(_core.read(str(tmp_path / "a.png")), _core.read(tmp_path / "a.png"))


# --- write ---------------------------------------------------------------------------------------------------------


def test_a_png_is_written_losslessly_at_8_bits_and_pillow_decodes_it(tmp_path, photo):
    _core.write(tmp_path / "a.png", photo)
    expected = (photo * 255 + 0.5).astype(np.uint8)
    with Image.open(tmp_path / "a.png") as img:
        assert img.mode == "RGB"
        np.testing.assert_array_equal(np.asarray(img), expected)


def test_a_webp_is_written_lossless(tmp_path, photo):
    _core.write(tmp_path / "a.webp", photo)
    np.testing.assert_array_equal(
        (pillow_rgb(tmp_path / "a.webp") * 255).round().astype(np.uint8), (photo * 255 + 0.5).astype(np.uint8)
    )


@pytest.mark.parametrize("name", ["a.png", "a.webp"])
def test_alpha_is_written_as_a_straight_alpha_channel(tmp_path, photo, subject, name):
    _core.write(tmp_path / name, np.dstack([photo, subject]))
    with Image.open(tmp_path / name) as img:
        assert img.mode == "RGBA"
        alpha = np.asarray(img)[..., 3]
    np.testing.assert_array_equal(alpha, (subject * 255 + 0.5).astype(np.uint8))


@pytest.mark.parametrize("name", ["a.jpg", "a.JPEG"])
def test_a_jpeg_decodes_in_pillow_close_to_the_picture_and_quality_trades_size_for_error(tmp_path, photo, name):
    size, error = {}, {}
    for quality in (30, 95):
        path = tmp_path / f"{quality}{name}"
        _core.write(path, photo, quality)
        size[quality] = path.stat().st_size
        error[quality] = (np.abs(pillow_rgb(path) - photo) * 255).mean()
    assert size[30] < size[95]
    assert error[30] > error[95]
    assert error[95] < 4  # noisy photo: mean error in levels at quality 95


@pytest.mark.parametrize("alpha", [False, True], ids=["RGB", "RGBA"])
def test_a_png_the_core_wrote_reads_back_as_its_8_bit_levels_exactly(tmp_path, photo, subject, alpha):
    """Its own sRGB profile must not move a level: the file is lossless, so the read is exactly what was written."""
    picture = np.dstack([photo, subject]) if alpha else photo
    _core.write(tmp_path / "a.png", picture)
    np.testing.assert_allclose(_core.read(tmp_path / "a.png"), (picture * 255 + 0.5).astype(np.uint8) / 255, atol=1e-6)


@pytest.mark.parametrize(
    ("name", "picture", "error", "match"),
    [
        pytest.param("a.jpg", "alpha", BackendUnavailableError, "transparen", id="a JPEG with alpha"),
        pytest.param("a.tif", "rgb", BackendUnavailableError, r"\.tif", id="an extension with no writer"),
        pytest.param("a", "rgb", BackendUnavailableError, "no extension", id="no extension"),
        pytest.param("missing/a.png", "rgb", ImageWriteError, r"a\.png", id="a folder that does not exist"),
    ],
)
def test_a_file_that_cannot_be_written_is_refused_with_the_error_of_its_kind(
    tmp_path, photo, name, picture, error, match
):
    image = np.dstack([photo, photo[..., 0]]) if picture == "alpha" else photo
    with pytest.raises(error, match=match):
        native.write(tmp_path / name, image)
    assert list(tmp_path.iterdir()) == []


@pytest.mark.parametrize("quality", [0, 101, -1, 300])
def test_a_quality_outside_1_to_100_is_a_value_error_and_writes_nothing(tmp_path, photo, quality):
    with pytest.raises(ValueError, match="quality"):
        _core.write(tmp_path / "a.jpg", photo, quality)
    assert list(tmp_path.iterdir()) == []


@pytest.mark.parametrize(
    "image",
    [
        pytest.param(np.zeros((4, 4, 3), np.float64), id="float64"),
        pytest.param(np.zeros((4, 4, 2), np.float32), id="two channels"),
        pytest.param(np.zeros((4, 4), np.float32), id="no channel axis"),
        pytest.param(np.zeros((4, 4, 3), np.float32)[:, ::2], id="not contiguous"),
    ],
)
def test_an_array_that_is_no_picture_is_refused_naming_what_it_is(tmp_path, image):
    with pytest.raises(ValueError, match="float32"):
        _core.write(tmp_path / "a.png", image)
