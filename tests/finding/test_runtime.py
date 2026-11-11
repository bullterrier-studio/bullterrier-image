"""The ONNX Runtime library the tests run models with is kept by `bt-image install onnx`, never fetched here."""

import pytest

from bullterrier_image import BackendUnavailableError
from bullterrier_image.core import native


def test_an_empty_cache_has_no_library_and_looking_does_not_download_one(tmp_path):
    with pytest.raises(BackendUnavailableError, match=r"bt-image install onnx"):
        native.kept_onnx_runtime(cache=tmp_path)
    assert list(tmp_path.iterdir()) == []  # nothing was fetched to find out
