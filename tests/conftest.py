"""What tests in every folder share: the small test picture and subject map, the effect-registry helpers, stand-ins for
the subject and depth models, the files git tracks, a copy of the project to inject one fault into, and the
environment `uv run` gives a command. Helpers used by one folder live in that folder's conftest.py."""

import os
import shutil
import subprocess
import sys
from collections.abc import Callable, Iterator
from contextlib import contextmanager
from pathlib import Path

import numpy as np
import onnx
import pytest
from onnx import TensorProto, helper

from bullterrier_image.core import registry
from bullterrier_image.core.params import EffectParams, Picture, param
from bullterrier_image.core.registry import effect, unregister

ROOT = Path(__file__).resolve().parents[1]


@pytest.fixture(autouse=True)
def no_enclosing_git(monkeypatch: pytest.MonkeyPatch) -> None:
    """Run every test as if no git were running it. A git hook (the gate that runs before a push) passes the repository
    it serves in `GIT_*` variables, `GIT_DIR` among them, which would send the git of a test, in this process or in a
    subprocess, to that repository instead of the throwaway one: its `git init`, `git config` and `git add` would land
    in the real repository."""
    for name in [name for name in os.environ if name.startswith("GIT_")]:
        monkeypatch.delenv(name)


@pytest.fixture(scope="session")
def repository_files() -> Callable[[Path], list[str]]:
    """`repository_files(root)` lists the files of the git repository at `root` as paths relative to it: those git
    tracks or has staged, and those it does not know yet unless .gitignore excludes them (so a new file counts before
    it is `git add`ed). A test that asks for it is skipped where git cannot tell: without the git command, or in a
    source archive, which is no git checkout."""

    def listing(root: Path) -> list[str]:
        if shutil.which("git") is None:
            pytest.skip("reads the repository with git: needs the git command")
        listed = subprocess.run(
            ["git", "ls-files", "--cached", "--exclude-standard", "--others", "-z"],
            cwd=root,
            capture_output=True,
            encoding="utf-8",
        )
        if listed.returncode != 0:
            pytest.skip("reads the repository with git: needs a git checkout")
        return [relative for relative in listed.stdout.split("\0") if relative]

    return listing


@pytest.fixture(scope="session")
def tracked_files(repository_files) -> list[str]:
    """Every file of this checkout that is tracked, staged or new and not ignored, as paths relative to its root (see
    `repository_files`)."""
    return repository_files(ROOT)


@contextmanager
def empty_registry() -> Iterator[None]:
    """Inside the block the effect registry is empty; afterwards, even if the block raised, whatever was registered in
    it is removed and the real effects are back, in their original order. Uses only the registry's public interface:
    `unregister`, and registering the same function again, which the registry allows."""
    real = [(name, fn, registry.category(name), registry.opacity(name)) for name, fn in registry.all_effects().items()]
    for name, *_ in real:
        unregister(name)
    try:
        yield
    finally:
        for name in registry.all_effects():
            unregister(name)
        for _, fn, category, opacity in real:
            effect(category=category, params=fn.params_model, opacity=opacity)(fn)


@pytest.fixture
def fresh_registry():
    """An empty effect registry for one test, so it can build exactly the effects and categories it needs, whatever
    real effects exist (see `empty_registry`)."""
    with empty_registry():
        yield


@pytest.fixture
def registry_emptied():
    """The `empty_registry` context manager itself, for the tests of that helper."""
    return empty_registry


class ProbeParams(EffectParams):
    """The one parameter of a probe effect."""

    amount: float = param(0.5, unit="fraction", min=0, max=1, doc="Unused: a probe returns the picture unchanged.")


@pytest.fixture
def register_probe():
    """`register_probe(name, category)` registers a do-nothing effect with that name. Use it inside `fresh_registry`
    or `throwaway_effects`, which remove it again."""

    def register(name: str, category: str = "utilities") -> None:
        def probe(rgb, params: ProbeParams):
            return rgb

        probe.__name__ = probe.__qualname__ = name
        probe.__doc__ = f"The {name} probe."
        effect(category=category, params=ProbeParams)(probe)

    return register


class LayerParams(EffectParams):
    """The one parameter of the layer probe: a picture."""

    image: Picture = param("#808080", doc="The picture whose average colour fills the photo.")


@pytest.fixture
def layer_probe(throwaway_effects):
    """Registers `layer_probe`, an effect with a Picture parameter: it returns the photo's shape filled with the
    average colour of the image's RGB (alpha ignored), so its output tells which picture it received."""

    def layer_probe(rgb: np.ndarray, params: LayerParams) -> np.ndarray:
        """Fills the photo with the average colour of a picture."""
        mean = params.image.pixels[..., :3].mean(axis=(0, 1))
        return np.broadcast_to(mean, rgb.shape).astype(np.float32)

    return effect(category="compositing", params=LayerParams)(layer_probe)


@pytest.fixture
def partition_probe(throwaway_effects):
    """Registers `partition_probe`, an effect that takes the injected input `partition` (int32 labels, one per pixel):
    it returns a grey picture of `label / 255`, so its output shows which piece each pixel was given."""

    def partition_probe(rgb: np.ndarray, params: ProbeParams, *, partition: np.ndarray | None = None) -> np.ndarray:
        """Paints each pixel with its piece number over 255."""
        assert partition is not None
        return np.repeat((partition / 255).astype(np.float32)[..., None], 3, axis=2)

    return effect(category="utilities", params=ProbeParams)(partition_probe)


@pytest.fixture
def depth_probe(throwaway_effects):
    """Registers `depth_probe`, an effect that takes the injected input `depth` (float32 (h, w), 1 = nearest): it
    returns the depth as a grey picture, so its output shows the depth map it was given."""

    def depth_probe(rgb: np.ndarray, params: ProbeParams, *, depth: np.ndarray | None = None) -> np.ndarray:
        """Paints each pixel with its depth."""
        assert depth is not None
        return np.repeat(depth[..., None], 3, axis=2)

    return effect(category="utilities", params=ProbeParams)(depth_probe)


@pytest.fixture
def apply_as_rendered() -> Callable[..., np.ndarray]:
    """`apply_as_rendered(name, rgb, **values)`: the effect `name` on `rgb` as a recipe renders it at the picture's own
    size: every `px` value (given or default) is written for 480 and multiplied by the longest side over 480, and the
    result is mixed into `rgb` by the effect's default step opacity. Arrays among the values are the effect's inputs (a
    field), the rest its parameters."""

    def apply(name: str, rgb: np.ndarray, **values: object) -> np.ndarray:
        function = registry.get(name)
        inputs = {key: value for key, value in values.items() if isinstance(value, np.ndarray)}
        params = function.validate({key: value for key, value in values.items() if key not in inputs})
        out = function(rgb, params.scaled(max(rgb.shape[:2]) / 480), **inputs)
        return rgb + np.float32(function.opacity) * (out - rgb)

    return apply


@pytest.fixture
def throwaway_effects():
    """Lets a test register effects of its own: whatever it registered is removed again afterwards, even if the test
    fails, so no other test ever sees it."""
    before = set(registry.all_effects())
    yield
    for name in set(registry.all_effects()) - before:
        unregister(name)


@pytest.fixture
def photo() -> np.ndarray:
    """A small, deterministic, non-trivial picture: colour gradients plus noise."""
    rng = np.random.default_rng(1)
    y, x = np.mgrid[0:96, 0:128].astype(np.float32)
    img = np.stack([x / 127, y / 95, (x + y) / 222], axis=-1) * 0.8 + rng.random((96, 128, 3), dtype=np.float32) * 0.2
    return img.astype(np.float32)


@pytest.fixture
def subject() -> np.ndarray:
    """A disc in the middle of the 96x128 picture: 1 inside, 0 outside, soft edge."""
    y, x = np.mgrid[0:96, 0:128].astype(np.float32)
    return np.clip(30 - np.hypot(x - 64, y - 48), 0, 1).astype(np.float32)


@pytest.fixture(autouse=True)
def no_model_download(monkeypatch):
    """No test may download a model, or depend on the settings of the person running the tests:
    `BT_IMAGE_SUBJECT_MODEL` and `BT_IMAGE_DEPTH_MODEL` point at files that do not exist (so the portable subject
    backend and the depth model fail at once, naming them, instead of downloading), and `BT_IMAGE_SEGMENTER` is `onnx`,
    so the subject is found the same way on every OS (macOS would default to Apple Vision). A test that needs a model
    sets the variable itself."""
    monkeypatch.setenv("BT_IMAGE_SUBJECT_MODEL", str(Path(__file__).parent / "no-such-model.onnx"))
    monkeypatch.setenv("BT_IMAGE_DEPTH_MODEL", str(Path(__file__).parent / "no-such-depth-model.onnx"))
    monkeypatch.setenv("BT_IMAGE_SEGMENTER", "onnx")


@pytest.fixture
def echo_model(tmp_path) -> Callable[[int], Path]:
    """`echo_model(channel)` writes a tiny ONNX segmentation model and returns its path. It stands in for the real
    model (an external boundary) and obeys the documented contract: input float32 (1, 3, 1024, 1024) holding v/255 - 0.5
    per RGB channel, first output (1, 1, 1024, 1024) the subject probability. It answers with input `channel` plus 0.5,
    so the probability it reports is that channel's own value: a picture whose red channel paints the subject is found
    exactly there, and a wrong channel order, scale or offset in the caller shows in the answer."""

    def write(channel: int) -> Path:
        side = 1024
        graph = helper.make_graph(
            [
                helper.make_node("Gather", ["image", "channel"], ["picked"], axis=1),
                helper.make_node("Add", ["picked", "half"], ["probability"]),
            ],
            "echo",
            [helper.make_tensor_value_info("image", TensorProto.FLOAT, [1, 3, side, side])],
            [helper.make_tensor_value_info("probability", TensorProto.FLOAT, [1, 1, side, side])],
            initializer=[
                helper.make_tensor("channel", TensorProto.INT64, [1], [channel]),
                helper.make_tensor("half", TensorProto.FLOAT, [], [0.5]),
            ],
        )
        model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 17)], ir_version=8)
        onnx.checker.check_model(model)
        path = tmp_path / f"echo-{channel}.onnx"
        onnx.save(model, path)
        return path

    return write


@pytest.fixture
def depth_echo_model(tmp_path, monkeypatch) -> Callable[[int], Path]:
    """`depth_echo_model(channel)` writes a tiny ONNX depth model, points `BT_IMAGE_DEPTH_MODEL` at it and returns its
    path. It stands in for Depth Anything V2 (an external boundary) with the real model's input and output: input
    `pixel_values` float32 (batch, 3, height, width), any height and width, holding (v/255 - mean) / std per RGB
    channel; output `predicted_depth` (batch, height, width), larger = nearer. It answers with input `channel` as it
    receives it, so the depth found is that channel scaled per photo: where the photo is brightest in it is nearest."""

    def write(channel: int) -> Path:
        graph = helper.make_graph(
            [helper.make_node("Gather", ["pixel_values", "channel"], ["predicted_depth"], axis=1)],
            "depth_echo",
            [helper.make_tensor_value_info("pixel_values", TensorProto.FLOAT, ["batch", 3, "height", "width"])],
            [helper.make_tensor_value_info("predicted_depth", TensorProto.FLOAT, ["batch", "height", "width"])],
            initializer=[helper.make_tensor("channel", TensorProto.INT64, [], [channel])],  # a scalar: the axis goes
        )
        model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 17)], ir_version=8)
        onnx.checker.check_model(model)
        path = tmp_path / f"depth-echo-{channel}.onnx"
        onnx.save(model, path)
        monkeypatch.setenv("BT_IMAGE_DEPTH_MODEL", str(path))
        return path

    return write


@pytest.fixture
def project_copy(tmp_path):
    """`project_copy(*names)` copies the project's src/ (without bytecode caches) and the files `names` at its root into
    tmp_path, and returns tmp_path: a scratch project to write one fault into, the real one untouched."""

    def copy(*names: str) -> Path:
        shutil.copytree(ROOT / "src", tmp_path / "src", ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
        for name in names:
            shutil.copy(ROOT / name, tmp_path)
        return tmp_path

    return copy


@pytest.fixture
def uv_run_env() -> dict[str, str]:
    """The environment a command gets under `uv run`: this one, with its executables first on PATH."""
    return {**os.environ, "PATH": os.pathsep.join([str(Path(sys.executable).parent), os.environ.get("PATH", "")])}
