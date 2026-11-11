"""Command line: `bt-image render|sheet|sweep|cutout|run|steps|init|effects`.

The command line is the Rust core's (`crates/cli`); this is the entry point that starts it. Effects written
in Python are loaded first, so a recipe may name them.
"""

import os
import signal
import sys

from . import __version__
from .core import native, registry
from .finding._runtime import onnxruntime_library


def run(argv: list[str]) -> int:
    """Run `bt-image` with `argv` (the arguments after the program's name) and return its exit code: 0 success, 1 the
    command failed (`error: <what is wrong>` on stderr), 2 wrong usage. Results go to stdout, progress and errors to
    stderr, as the process's own."""
    registry.load_plugins()
    return native.main(argv, __version__)


def main() -> None:
    """The console script `bt-image`: run the command line with `sys.argv` and exit with its code. Ctrl-C ends it at
    once (the shell reports 130): Python's handler would wait for the Rust code, which does not look for it."""
    signal.signal(signal.SIGINT, signal.SIG_DFL)
    if (
        library := onnxruntime_library()
    ) is not None:  # the library of the installed package, until `install onnx` is all
        os.environ.setdefault("BT_IMAGE_ONNX_LIB", str(library))
    sys.exit(run(sys.argv[1:]))
