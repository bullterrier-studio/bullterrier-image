"""Tests of `bullterrier_image.finding._onnx` through the finders that run ONNX models: a model file onnxruntime cannot
use is a BackendUnavailableError naming the variable that points at it, never onnxruntime's own exception.

The model files are external boundaries, built here: a file that is no ONNX model, and a valid ONNX model that breaks
the finders' contract (its input has a fixed size of 8 x 8 px)."""

from pathlib import Path

import onnx
import pytest
from onnx import TensorProto, helper

from bullterrier_image import BackendUnavailableError
from bullterrier_image.finding.depth import depth_finder, depth_map
from bullterrier_image.finding.subject import subject_matte


def junk(folder: Path) -> Path:
    path = folder / "junk.onnx"
    path.write_bytes(b"not an onnx model")
    return path


def fixed_size(folder: Path) -> Path:
    """A valid ONNX model whose one input is (1, 3, 8, 8): no photo the finders feed it fits."""
    shape = [1, 3, 8, 8]
    graph = helper.make_graph(
        [helper.make_node("Identity", ["image"], ["answer"])],
        "fixed",
        [helper.make_tensor_value_info("image", TensorProto.FLOAT, shape)],
        [helper.make_tensor_value_info("answer", TensorProto.FLOAT, shape)],
    )
    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 17)], ir_version=8)
    onnx.checker.check_model(model)
    path = folder / "fixed.onnx"
    onnx.save(model, path)
    return path


FINDERS = {
    "the subject": ("BT_IMAGE_SUBJECT_MODEL", lambda photo: subject_matte(photo, segmenter="onnx")),
    "the depth": ("BT_IMAGE_DEPTH_MODEL", depth_map),
    "the depth finder": ("BT_IMAGE_DEPTH_MODEL", lambda photo: depth_finder().find(photo)),
}
FILES = {"a file that is no ONNX model": junk, "an ONNX model of another input size": fixed_size}


@pytest.mark.parametrize("write", FILES.values(), ids=FILES.keys())
@pytest.mark.parametrize(("variable", "find"), FINDERS.values(), ids=FINDERS.keys())
def test_a_model_file_onnxruntime_cannot_use_is_unavailable_naming_its_variable_and_file(
    tmp_path, monkeypatch, photo, variable, find, write
):
    model = write(tmp_path)
    monkeypatch.setenv(variable, str(model))
    with pytest.raises(BackendUnavailableError, match=variable) as caught:
        find(photo)
    assert str(model) in str(caught.value)
