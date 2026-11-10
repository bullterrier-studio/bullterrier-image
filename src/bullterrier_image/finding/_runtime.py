"""Where the ONNX Runtime library is, for the core, which loads it at run time and bundles none."""

import importlib.util
import sys
from pathlib import Path


def onnxruntime_library() -> Path | None:
    """The library file of the installed `onnxruntime` package, or None when it is not installed (the core then says
    to set `BT_IMAGE_ONNX_LIB` or install the package). `BT_IMAGE_ONNX_LIB`, when set, wins: the core reads it."""
    spec = importlib.util.find_spec("onnxruntime")
    if spec is None or not spec.submodule_search_locations:
        return None
    capi = Path(next(iter(spec.submodule_search_locations))) / "capi"
    patterns = {"darwin": "libonnxruntime*.dylib", "win32": "onnxruntime.dll"}
    return next(capi.glob(patterns.get(sys.platform, "libonnxruntime.so*")), None)
