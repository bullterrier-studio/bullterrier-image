"""The files the facade hands to the Rust extension: errors arrive as the package's exceptions, a path may be text, and
an array that is no picture is refused. Decoding, encoding and metadata are tested in Rust (`crates/core/tests/it`)."""

import numpy as np
import pytest
from PIL import Image

from bullterrier_image._native import _core  # noqa: PLC2701  architecture: the boundary imageio calls
from bullterrier_image.core import native
from bullterrier_image.errors import BackendUnavailableError, ImageReadError, ImageWriteError


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
