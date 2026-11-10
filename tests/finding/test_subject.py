"""Tests of `bullterrier_image.finding.subject`: choosing a backend, the portable ONNX backend, and `refine_edge`.

The models are external boundaries: the portable backend is given tiny ONNX models built in the test (`echo_model` in
conftest), whose answer follows from the documented model contract, so the expected mattes are worked out by hand.
Apple Vision runs for real where it exists (macOS). Expected values for `refine_edge` are worked out by hand: erosion
by a disc on axis-aligned shapes, and the normal distribution for a blurred straight edge."""

import sys
from pathlib import Path

import numpy as np
import pytest

from bullterrier_image import BackendUnavailableError, SubjectNotFoundError
from bullterrier_image.finding.subject import refine_edge, subject_finder, subject_matte

EXAMPLE_PHOTO = Path(__file__).parents[2] / "examples" / "photos" / "bullterrier.jpeg"


def found(photo: np.ndarray, segmenter: str | None = None) -> np.ndarray:
    """The subject as the backend answers, without the edge clean-up."""
    return subject_matte(photo, segmenter=segmenter, shrink=0.0, feather=0.0)


def picture(red: np.ndarray, green: float = 0.9, blue: float = 0.2) -> np.ndarray:
    """An RGB picture whose red channel is `red` (h, w) and whose other channels are flat."""
    return np.dstack([red, np.full_like(red, green), np.full_like(red, blue)]).astype(np.float32)


def ramp(height: int = 60, width: int = 90) -> np.ndarray:
    """Red rising from 0.1 at the left to 0.9 at the right: a non-square picture whose subject is 'the right part'."""
    return picture(np.tile(np.linspace(0.1, 0.9, width, dtype=np.float32), (height, 1)))


@pytest.fixture
def echo_red(echo_model, monkeypatch):
    """The portable backend answers with the red channel (see `echo_model`)."""
    monkeypatch.setenv("BT_IMAGE_SUBJECT_MODEL", str(echo_model(0)))


# --- choosing the backend: explicit > BT_IMAGE_SEGMENTER > auto ----------------------------------------------------


def test_the_onnx_backend_reads_the_picture_as_the_model_contract_says(echo_red):
    """The model is fed RGB in that order, scaled v/255 - 0.5 (the echo model returns red, so the matte is red), at
    1024 x 1024, and its answer comes back at the photo's own, non-square size."""
    photo = ramp()
    alpha = found(photo, "onnx")
    assert alpha.shape == (60, 90)
    assert alpha.dtype == np.float32
    # resized to 1024 and back: a straight ramp survives to within a step of 8-bit rounding and the resampling at the
    # left and right borders
    np.testing.assert_allclose(alpha[:, 2:-2], photo[:, 2:-2, 0], atol=0.01)


@pytest.mark.parametrize(("channel", "expected"), [(1, 230 / 255), (2, 153 / 255)])
def test_each_channel_reaches_the_model_in_rgb_order_as_its_8_bit_value(echo_model, monkeypatch, channel, expected):
    """Flat channels are exact 8-bit values, so no resampling blurs them: the model sees exactly v/255 - 0.5."""
    monkeypatch.setenv("BT_IMAGE_SUBJECT_MODEL", str(echo_model(channel)))
    alpha = found(picture(np.zeros((20, 30), np.float32), green=230 / 255, blue=153 / 255), "onnx")
    np.testing.assert_allclose(alpha, expected, atol=1e-6)


def test_the_onnx_matte_stays_in_range_is_repeatable_and_leaves_the_photo_alone(echo_red):
    photo = ramp()
    before = photo.copy()
    first = subject_matte(photo, segmenter="onnx")
    np.testing.assert_array_equal(subject_matte(photo, segmenter="onnx"), first)
    assert first.min() >= 0.0
    assert first.max() <= 1.0
    np.testing.assert_array_equal(photo, before)


def test_the_onnx_matte_gets_the_same_edge_clean_up_as_any_matte(echo_red):
    photo = picture(np.where(np.arange(90) >= 45, 0.9, 0.1).astype(np.float32) * np.ones((60, 1), np.float32))
    raw = found(photo, "onnx")
    np.testing.assert_array_equal(subject_matte(photo, segmenter="onnx", shrink=2, feather=1), refine_edge(raw, 2, 1))


def test_a_picture_where_nothing_stands_out_is_an_error(echo_red):
    flat = picture(np.full((20, 30), 0.3, np.float32))  # the model's answer stays below one half everywhere
    with pytest.raises(SubjectNotFoundError, match="no subject"):
        subject_matte(flat, segmenter="onnx")


def test_an_explicit_segmenter_wins_over_the_environment(echo_red, monkeypatch):
    monkeypatch.setenv("BT_IMAGE_SEGMENTER", "vision")  # would fail off macOS, and differ from the echo on macOS
    photo = ramp()
    np.testing.assert_allclose(found(photo, "onnx")[:, 2:-2], photo[:, 2:-2, 0], atol=0.01)


@pytest.mark.parametrize("value", ["onnx", " ONNX "])
def test_the_environment_variable_chooses_when_no_segmenter_is_given(echo_red, monkeypatch, value):
    monkeypatch.setenv("BT_IMAGE_SEGMENTER", value)
    photo = ramp()
    np.testing.assert_allclose(found(photo)[:, 2:-2], photo[:, 2:-2, 0], atol=0.01)


def test_an_unknown_segmenter_is_refused_naming_the_known_ones():
    with pytest.raises(BackendUnavailableError, match="coreml") as caught:
        subject_matte(ramp(), segmenter="coreml")
    assert "vision" in str(caught.value)
    assert "onnx" in str(caught.value)


def test_an_unknown_segmenter_in_the_environment_is_refused_naming_the_variable(monkeypatch):
    monkeypatch.setenv("BT_IMAGE_SEGMENTER", "coreml")
    with pytest.raises(BackendUnavailableError, match="BT_IMAGE_SEGMENTER") as caught:
        subject_matte(ramp())
    assert "coreml" in str(caught.value)
    assert "onnx" in str(caught.value)


# --- subject_finder: the segmenter that would run, named by an identity the pipeline remembers subjects by ---------


def test_the_identity_follows_the_content_of_the_model_not_its_place(echo_model, monkeypatch, tmp_path):
    red = echo_model(0)
    monkeypatch.setenv("BT_IMAGE_SUBJECT_MODEL", str(red))
    identity = subject_finder("onnx").identity
    assert subject_finder("onnx").identity == identity  # same inputs, same identity
    copy = tmp_path / "elsewhere.onnx"
    copy.write_bytes(red.read_bytes())
    monkeypatch.setenv("BT_IMAGE_SUBJECT_MODEL", str(copy))
    assert subject_finder("onnx").identity == identity  # the same model at another path is the same segmenter
    monkeypatch.setenv("BT_IMAGE_SUBJECT_MODEL", str(echo_model(1)))
    assert subject_finder("onnx").identity != identity  # another model, another identity


def test_the_standard_model_has_an_identity_of_its_own_without_being_downloaded(echo_model, monkeypatch, tmp_path):
    home = tmp_path / "home"  # where the standard model would be fetched to: it must stay empty
    home.mkdir()
    monkeypatch.setenv("HOME", str(home))
    monkeypatch.setenv("XDG_CACHE_HOME", str(home / "cache"))
    monkeypatch.setenv("LOCALAPPDATA", str(home / "local"))
    monkeypatch.delenv("BT_IMAGE_SUBJECT_MODEL")
    standard = subject_finder("onnx").identity
    assert standard == subject_finder("onnx").identity
    assert not any(home.rglob("*"))  # nothing was fetched to find out
    monkeypatch.setenv("BT_IMAGE_SUBJECT_MODEL", str(echo_model(0)))
    assert subject_finder("onnx").identity != standard


def test_the_identity_is_chosen_like_the_backend_is(echo_red, monkeypatch):
    explicit = subject_finder("onnx").identity
    monkeypatch.setenv("BT_IMAGE_SEGMENTER", "onnx")
    assert subject_finder().identity == explicit  # the environment variable chooses when nothing is given
    monkeypatch.setenv("BT_IMAGE_SEGMENTER", "coreml")
    assert subject_finder("onnx").identity == explicit  # and an explicit choice wins over it
    with pytest.raises(BackendUnavailableError, match="BT_IMAGE_SEGMENTER"):
        subject_finder()


def test_a_finder_of_a_segmenter_that_cannot_run_is_refused_saying_why_as_subject_matte_does():
    with pytest.raises(BackendUnavailableError, match="coreml") as caught:
        subject_finder("coreml")
    assert "vision, onnx" in str(caught.value)  # the choices
    with pytest.raises(
        BackendUnavailableError, match="no-such-model"
    ):  # the suite's BT_IMAGE_SUBJECT_MODEL names no file
        subject_finder("onnx")


def test_the_finder_answers_as_subject_matte_with_the_segmenter_chosen_when_it_was_made(echo_red, monkeypatch):
    photo = ramp()
    monkeypatch.setenv("BT_IMAGE_SEGMENTER", "onnx")
    finder = subject_finder()
    monkeypatch.setenv("BT_IMAGE_SEGMENTER", "coreml")  # refused if it were read again
    np.testing.assert_array_equal(finder.find(photo), subject_matte(photo, segmenter="onnx"))


@pytest.mark.skipif(sys.platform != "darwin", reason="Apple Vision exists only on macOS")
def test_on_macos_vision_and_the_portable_model_have_different_identities(echo_red, monkeypatch):
    assert subject_finder("vision").identity != subject_finder("onnx").identity
    monkeypatch.setenv("BT_IMAGE_SEGMENTER", "vision")
    assert subject_finder().identity == subject_finder("vision").identity
    monkeypatch.setenv("BT_IMAGE_SEGMENTER", "onnx")
    assert subject_finder().identity == subject_finder("onnx").identity


@pytest.mark.skipif(sys.platform == "darwin", reason="Apple Vision exists on macOS, so it is not refused there")
def test_off_macos_the_identity_of_apple_vision_is_refused_like_its_use():
    with pytest.raises(BackendUnavailableError, match="macOS"):
        subject_finder("vision")


@pytest.mark.skipif(sys.platform == "darwin", reason="Apple Vision exists on macOS, so it is not refused there")
def test_apple_vision_is_refused_off_macos_saying_what_to_use_instead():
    with pytest.raises(BackendUnavailableError, match="macOS") as caught:
        subject_matte(ramp(), segmenter="vision")
    assert "onnx" in str(caught.value)


@pytest.mark.skipif(sys.platform == "darwin", reason="macOS defaults to Apple Vision")
def test_off_macos_the_default_is_the_portable_model(echo_red, monkeypatch):
    monkeypatch.delenv("BT_IMAGE_SEGMENTER")
    photo = ramp()
    np.testing.assert_array_equal(subject_matte(photo), subject_matte(photo, segmenter="onnx"))
    assert subject_finder().identity == subject_finder("onnx").identity


@pytest.mark.skipif(sys.platform != "darwin", reason="Apple Vision exists only on macOS")
def test_on_macos_the_default_is_apple_vision(echo_red, monkeypatch):
    """Vision cuts cleaner than the portable model, so a Mac uses it unless told otherwise."""
    monkeypatch.delenv("BT_IMAGE_SEGMENTER")
    assert subject_finder().identity == subject_finder("vision").identity
    assert subject_finder().identity != subject_finder("onnx").identity


@pytest.mark.skipif(sys.platform == "darwin", reason="macOS defaults to Apple Vision, which needs no model file")
def test_a_default_that_cannot_run_names_the_model_it_needs(monkeypatch):
    monkeypatch.delenv("BT_IMAGE_SEGMENTER")
    with pytest.raises(BackendUnavailableError, match="no-such-model") as caught:  # the suite's BT_IMAGE_SUBJECT_MODEL
        subject_matte(ramp())
    assert "default" in str(caught.value)


def test_a_missing_local_model_file_is_named_with_the_variable_that_points_at_it(tmp_path, monkeypatch):
    missing = tmp_path / "isnet.onnx"
    monkeypatch.setenv("BT_IMAGE_SUBJECT_MODEL", str(missing))
    with pytest.raises(BackendUnavailableError, match="BT_IMAGE_SUBJECT_MODEL") as caught:
        subject_matte(ramp(), segmenter="onnx")
    assert str(missing) in str(caught.value)
    assert not missing.exists()  # pointing at a local file never downloads


# --- refine_edge: its rows are the core's (crates/core/tests/it/matte.rs); here only the facade ---------------------


def square(size: int = 40, start: int = 10, stop: int = 30) -> np.ndarray:
    alpha = np.zeros((size, size), np.float32)
    alpha[start:stop, start:stop] = 1.0
    return alpha


def test_the_result_is_float32_and_the_input_is_not_modified():
    alpha = square()
    before = alpha.copy()
    out = refine_edge(alpha, 2.0, 1.0)
    assert out.dtype == np.float32
    assert out.shape == alpha.shape
    np.testing.assert_array_equal(alpha, before)
