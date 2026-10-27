"""Tests of `bullterrier_image.finding.depth`: the default depth finder, Depth Anything V2 Small on ONNX Runtime.

The model is an external boundary: tests give it tiny ONNX models built here and in conftest (`depth_echo_model`) with
the real model's input and output, whose answers follow from the documented model contract, so the expected maps are
worked out by hand. The real model runs only when it is already in the cache; no test downloads it."""

import sys
from pathlib import Path

import numpy as np
import onnx
import pytest
from onnx import TensorProto, helper

from bullterrier_image import BackendUnavailableError, Band, DepthNotFoundError, Recipe, Step, imageio, render
from bullterrier_image.finding.depth import depth_finder, depth_map, depth_model_path, inverse_depth
from bullterrier_image.ops.resample import resize

EXAMPLE_PHOTO = Path(__file__).parents[2] / "examples" / "photos" / "bullterrier.jpeg"


def picture(red: np.ndarray, green: float = 0.3, blue: float = 0.6) -> np.ndarray:
    """An RGB picture whose red channel is `red` (h, w) and whose other channels are flat."""
    return np.dstack([red, np.full_like(red, green), np.full_like(red, blue)]).astype(np.float32)


def ramp(height: int = 60, width: int = 80) -> np.ndarray:
    """Red falling from 0.9 at the left to 0.1 at the right: with the echo model, near on the left, far on the right."""
    return picture(np.tile(np.linspace(0.9, 0.1, width, dtype=np.float32), (height, 1)))


# --- the map: the model's answer at the photo's size, scaled per photo, 1 = nearest ---------------------------------


def test_the_depth_is_the_models_answer_at_the_photos_size_scaled_so_the_nearest_is_1(depth_echo_model):
    """The echo model answers with the red channel as it receives it ((v/255 - mean) / std, rising with v), so the
    depth is the red channel scaled to 0..1 over the photo: bright red is near."""
    depth_echo_model(0)
    photo = ramp()
    depth = depth_map(photo)
    assert depth.shape == (60, 80)
    assert depth.dtype == np.float32
    assert depth.min() == 0.0
    assert depth.max() == 1.0
    red = photo[..., 0]
    expected = (red - red.min()) / (red.max() - red.min())
    # through the model's size and back: a straight ramp survives but at the left and right borders
    np.testing.assert_allclose(depth[:, 3:-3], expected[:, 3:-3], atol=0.02)


def test_the_raw_depth_is_the_models_answer_at_the_photos_size_on_the_models_own_scale(depth_echo_model):
    """The echo model answers with the red channel as it receives it, an affine function of it. A photo of one flat red
    is answered by one number, and a redder photo by a larger one, whatever else is in the picture: the scale is the
    model's, not the photo's, so photos can be compared (and scaled alike) by the caller."""
    depth_echo_model(0)
    answers = {red: inverse_depth(picture(np.full((60, 80), red, np.float32))) for red in (0.3, 0.5, 0.7)}
    for answer in answers.values():
        assert answer.shape == (60, 80)
        assert answer.dtype == np.float32
        assert answer.max() - answer.min() < 1e-4  # one flat answer
    low, middle, high = (float(answers[red].mean()) for red in (0.3, 0.5, 0.7))
    assert low < middle < high
    assert middle == pytest.approx((low + high) / 2, abs=1e-4)  # 0.5 is half way between 0.3 and 0.7: affine


def test_the_depth_is_the_raw_depth_scaled_to_0_1_over_the_photo(depth_echo_model):
    """The same picture gives the same map whether the finder scales it or the caller scales the raw values."""
    depth_echo_model(0)
    photo = ramp()
    raw = inverse_depth(photo)
    by_the_caller = (raw - raw.min()) / (raw.max() - raw.min())
    np.testing.assert_allclose(depth_map(photo), by_the_caller, atol=1e-6)


def test_photos_scaled_by_one_shared_range_keep_their_depths_where_scaling_each_would_not(depth_echo_model):
    """A dim and a bright photo, each with a little texture: scaled alike by the caller the bright one is nearer
    everywhere; scaled one by one (`depth_map`) both fill 0..1."""
    depth_echo_model(0)
    texture = np.tile(np.linspace(0, 1, 80, dtype=np.float32), (60, 1))
    dim, bright = picture(0.2 + 0.1 * texture), picture(0.6 + 0.1 * texture)
    raws = [inverse_depth(dim), inverse_depth(bright)]
    low, high = min(raw.min() for raw in raws), max(raw.max() for raw in raws)
    shared_dim, shared_bright = ((raw - low) / (high - low) for raw in raws)
    assert shared_dim.max() < shared_bright.min()
    assert (depth_map(dim).max(), depth_map(bright).max()) == (1.0, 1.0)


def test_the_bright_half_of_the_answer_is_near_and_the_dark_half_far(depth_echo_model):
    depth_echo_model(0)
    halves = picture(np.where(np.arange(80) < 40, 0.9, 0.1).astype(np.float32) * np.ones((60, 1), np.float32))
    depth = depth_map(halves)
    # the resampling rings a little at the step, so its peaks, not the halves, are exactly 1 and 0
    assert depth[:, :30].min() > 0.95  # the bright left half: nearest
    assert depth[:, 50:].max() < 0.05  # the dark right half: farthest


@pytest.mark.parametrize(
    "photo",
    [
        pytest.param(np.full((30, 40, 3), 0.4, np.float32), id="a flat photo"),
        pytest.param(picture(np.full((30, 40), 0.4, np.float32), green=0.9), id="a flat channel, the rest not"),
    ],
)
def test_a_flat_answer_says_everything_is_equally_far_all_zeros(depth_echo_model, photo):
    """Nothing to tell near from far: zeros, not NaN from dividing by a zero range (the suite fails on any warning)."""
    depth_echo_model(0)
    np.testing.assert_array_equal(depth_map(photo), np.zeros((30, 40), np.float32))


def test_the_depth_is_repeatable_and_leaves_the_photo_alone(depth_echo_model):
    depth_echo_model(0)
    photo = ramp()
    before = photo.copy()
    first = depth_map(photo)
    np.testing.assert_array_equal(depth_map(photo), first)
    np.testing.assert_array_equal(photo, before)


@pytest.mark.parametrize(("channel", "nearest"), [(1, "top"), (2, "bottom")])
def test_each_channel_reaches_the_model_in_rgb_order(depth_echo_model, channel, nearest):
    """Green rises downwards and blue upwards: the echo of channel 1 is nearest at the bottom, of channel 2 at the top
    (red is flat, so a swapped order would give a flat answer or the opposite side)."""
    depth_echo_model(channel)
    rows = np.linspace(0.1, 0.9, 40, dtype=np.float32)[:, None] * np.ones((1, 30), np.float32)
    photo = np.dstack([np.full_like(rows, 0.5), rows, rows[::-1]])
    depth = depth_map(photo)
    top, bottom = depth[:5].mean(), depth[-5:].mean()
    assert (top < bottom) if nearest == "top" else (top > bottom)


def test_a_model_answering_nan_is_refused_by_the_render_not_taken_for_a_flat_answer(tmp_path, monkeypatch):
    """A broken model's NaN is not 'everything equally far': it reaches the runner's check of the map and is refused."""
    graph = helper.make_graph(
        [
            helper.make_node("Gather", ["pixel_values", "zero_index"], ["red"], axis=1),
            helper.make_node("Mul", ["red", "nan"], ["predicted_depth"]),
        ],
        "nan",
        [helper.make_tensor_value_info("pixel_values", TensorProto.FLOAT, ["batch", 3, "height", "width"])],
        [helper.make_tensor_value_info("predicted_depth", TensorProto.FLOAT, ["batch", "height", "width"])],
        initializer=[
            helper.make_tensor("zero_index", TensorProto.INT64, [], [0]),
            helper.make_tensor("nan", TensorProto.FLOAT, [], [float("nan")]),
        ],
    )
    model = tmp_path / "nan.onnx"
    onnx.save(helper.make_model(graph, opset_imports=[helper.make_opsetid("", 17)], ir_version=8), model)
    monkeypatch.setenv("BT_IMAGE_DEPTH_MODEL", str(model))
    near = Recipe(
        "near", (Step("gradient", {"colors": ["#000000"]}, "near"),), size=None, regions={"near": Band(of="depth")}
    )
    with pytest.raises(DepthNotFoundError, match="not finite"):
        render(ramp(), near)


# --- the model's input: short side 518 px, both sides multiples of 14 ----------------------------------------------


def size_model(path: Path) -> Path:
    """A stand-in with the real model's input and output whose answer tells the size it was given: where the red
    channel it receives is below -1 (red 0 after (v/255 - 0.485) / 0.229) it answers 0, above 1 (red 1) its input's
    width, in between its input's height. Scaled per photo, the depth there reads 0, height / longest and width /
    longest."""
    graph = helper.make_graph(
        [
            helper.make_node("Gather", ["pixel_values", "zero_index"], ["red"], axis=1),
            helper.make_node("Shape", ["pixel_values"], ["shape"]),
            helper.make_node("Gather", ["shape", "height_index"], ["height_count"]),
            helper.make_node("Gather", ["shape", "width_index"], ["width_count"]),
            helper.make_node("Cast", ["height_count"], ["height"], to=TensorProto.FLOAT),
            helper.make_node("Cast", ["width_count"], ["width"], to=TensorProto.FLOAT),
            helper.make_node("Less", ["red", "minus_one"], ["black"]),
            helper.make_node("Greater", ["red", "one"], ["full"]),
            helper.make_node("Where", ["full", "width", "height"], ["side"]),
            helper.make_node("Where", ["black", "zero", "side"], ["predicted_depth"]),
        ],
        "size",
        [helper.make_tensor_value_info("pixel_values", TensorProto.FLOAT, ["batch", 3, "height", "width"])],
        [helper.make_tensor_value_info("predicted_depth", TensorProto.FLOAT, ["batch", "height", "width"])],
        initializer=[
            helper.make_tensor("zero_index", TensorProto.INT64, [], [0]),
            helper.make_tensor("height_index", TensorProto.INT64, [], [2]),
            helper.make_tensor("width_index", TensorProto.INT64, [], [3]),
            helper.make_tensor("minus_one", TensorProto.FLOAT, [], [-1.0]),
            helper.make_tensor("one", TensorProto.FLOAT, [], [1.0]),
            helper.make_tensor("zero", TensorProto.FLOAT, [], [0.0]),
        ],
    )
    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 17)], ir_version=8)
    onnx.checker.check_model(model)
    onnx.save(model, path)
    return path


@pytest.mark.parametrize(
    ("photo_size", "model_size"),
    [
        # short side to 518, the other side by the same scale, then to the nearest multiple of 14 (518 = 37 x 14)
        pytest.param((300, 1000), (518, 1722), id="1726.7 rounds down to 123 x 14"),
        pytest.param((300, 1010), (518, 1750), id="1743.9 rounds up to 125 x 14"),
        pytest.param((1010, 300), (1750, 518), id="upright: the width is the short side"),
        pytest.param((60, 80), (518, 686), id="a small photo is enlarged: 690.7 to 49 x 14"),
        pytest.param((1200, 900), (686, 518), id="a large photo is reduced"),
    ],
)
def test_the_model_sees_the_short_side_at_518_px_and_both_sides_in_multiples_of_14(
    tmp_path, monkeypatch, photo_size, model_size
):
    monkeypatch.setenv("BT_IMAGE_DEPTH_MODEL", str(size_model(tmp_path / "size.onnx")))
    height, width = photo_size
    third = width // 3
    red = np.ones((height, width), np.float32)
    red[:, :third], red[:, third : 2 * third] = 0.0, 128 / 255  # black, mid-grey and full red, a third each
    depth = depth_map(picture(red))
    middle = height // 2
    longest = max(model_size)
    assert depth[middle, third // 2] == 0.0
    assert depth[middle, third + third // 2] == pytest.approx(model_size[0] / longest, rel=1e-5)
    assert depth[middle, 2 * third + third // 2] == pytest.approx(model_size[1] / longest, rel=1e-5)


# --- depth_finder: the depth model as a Finder, named by its model --------------------------------------------------


def test_the_finder_answers_as_depth_map(depth_echo_model):
    depth_echo_model(0)
    photo = ramp()
    np.testing.assert_array_equal(depth_finder().find(photo), depth_map(photo))


def test_the_identity_follows_the_content_of_the_model_not_its_place(depth_echo_model, monkeypatch, tmp_path):
    red = depth_echo_model(0)
    identity = depth_finder().identity
    assert depth_finder().identity == identity  # same model, same identity
    copy = tmp_path / "elsewhere.onnx"
    copy.write_bytes(red.read_bytes())
    monkeypatch.setenv("BT_IMAGE_DEPTH_MODEL", str(copy))
    assert depth_finder().identity == identity  # the same model at another path is the same finder
    depth_echo_model(1)
    assert depth_finder().identity != identity  # another model, another identity


def test_the_standard_model_has_an_identity_of_its_own_without_being_downloaded(
    depth_echo_model, monkeypatch, tmp_path
):
    home = tmp_path / "home"  # where the standard model would be fetched to: it must stay empty
    home.mkdir()
    for variable in ("HOME", "USERPROFILE"):
        monkeypatch.setenv(variable, str(home))
    monkeypatch.setenv("XDG_CACHE_HOME", str(home / "cache"))
    monkeypatch.setenv("LOCALAPPDATA", str(home / "local"))
    monkeypatch.delenv("BT_IMAGE_DEPTH_MODEL")
    standard = depth_finder().identity
    assert standard == depth_finder().identity
    assert not any(home.rglob("*"))  # nothing was fetched to find out
    depth_echo_model(0)
    assert depth_finder().identity != standard


# --- when the model cannot run ---------------------------------------------------------------------------------------


@pytest.mark.parametrize("ask", [depth_map, lambda photo: depth_finder()], ids=["depth_map", "depth_finder"])
def test_a_missing_local_model_file_is_named_with_the_variable_that_points_at_it(tmp_path, monkeypatch, ask):
    missing = tmp_path / "depth.onnx"
    monkeypatch.setenv("BT_IMAGE_DEPTH_MODEL", str(missing))
    with pytest.raises(BackendUnavailableError, match="BT_IMAGE_DEPTH_MODEL") as caught:
        ask(ramp())
    assert str(missing) in str(caught.value)
    assert not missing.exists()  # pointing at a local file never downloads


def test_an_unwritable_cache_folder_says_how_to_get_the_model_by_hand(tmp_path, monkeypatch):
    """The user's cache folder lies under a file, so it cannot be made: the advice comes before any download."""
    blocker = tmp_path / "not-a-home"
    blocker.write_text("a file where the home folder should be", encoding="utf-8")
    for variable in ("HOME", "USERPROFILE", "XDG_CACHE_HOME", "LOCALAPPDATA"):
        monkeypatch.setenv(variable, str(blocker))
    monkeypatch.delenv("BT_IMAGE_DEPTH_MODEL")
    with pytest.raises(BackendUnavailableError, match="by hand") as caught:
        depth_map(ramp())
    assert "BT_IMAGE_DEPTH_MODEL" in str(caught.value)
    assert "--map depth=" in str(caught.value)


def test_without_onnxruntime_the_depth_model_is_unavailable_saying_what_to_install(depth_echo_model, monkeypatch):
    depth_echo_model(0)  # a new file: no session of it is loaded yet
    monkeypatch.setitem(sys.modules, "onnxruntime", None)  # the library is not installed: importing it fails
    with pytest.raises(BackendUnavailableError, match="onnxruntime"):
        depth_map(ramp())


def test_the_model_path_is_the_local_file_when_one_is_given(tmp_path, monkeypatch):
    monkeypatch.setenv("BT_IMAGE_DEPTH_MODEL", str(tmp_path / "mine.onnx"))
    assert depth_model_path() == tmp_path / "mine.onnx"


# --- the real model, when it is in the cache --------------------------------------------------------------------------


def test_the_real_model_puts_the_dog_nearer_than_the_window(monkeypatch):
    """The pinned model itself, when it has been downloaded before. The test never downloads: it uses the cached file
    only if it is the right one (the finder's identity is the pinned digest for the standard model, the file's digest
    for a named one), and through `BT_IMAGE_DEPTH_MODEL`, which is never fetched."""
    monkeypatch.delenv("BT_IMAGE_DEPTH_MODEL")
    pinned = depth_finder().identity
    cached = depth_model_path()
    if not cached.is_file():
        pytest.skip(f"the depth model is not in the cache ({cached}); a render that needs depth downloads it")
    monkeypatch.setenv("BT_IMAGE_DEPTH_MODEL", str(cached))
    if depth_finder().identity != pinned:
        pytest.skip(f"the cached model {cached} is not the pinned file; delete it to download it again")
    photo = resize(imageio.load(EXAMPLE_PHOTO), 320)
    depth = depth_map(photo)
    height, width = depth.shape
    chest = depth[height // 2 - 10 : height // 2 + 10, width // 2 - 10 : width // 2 + 10].mean()
    window = depth[: height // 10, : width // 10].mean()  # top left
    # the sofa's front edge at the bottom is nearest (1); the dog sits behind it, well in front of the window
    assert chest > 0.3
    assert window < 0.2
    np.testing.assert_array_equal(depth_map(photo), depth)
