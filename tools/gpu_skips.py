"""Run the GPU tests and say loudly how many skipped: Rust has no skip at run time, so a GPU test on a machine that says
it has no GPU (`BT_GPU=none`) returns early and passes, and a green run there would look like a proof.

    python tools/gpu_skips.py -- COMMAND [ARG...]

The command runs with `BT_GPU_SKIPS` naming a fresh file, where each GPU test that skips writes its name
(`crates/core/tests/it/gpu/mod.rs`). Then the tool prints how many tests skipped, as a banner when any did, deletes the
file and exits with the command's status.

Interface:
    report(names) -> what the tool prints for the names the skipped tests wrote (one a skip; a test may write twice)
    main(argv)    -> the command's exit status
"""

import os
import subprocess
import sys
import tempfile
from pathlib import Path


def report(names: list[str]) -> str:
    """One line when no test skipped, else a banner counting the distinct tests that did."""
    skipped = len(set(names))
    if skipped == 0:
        return "GPU tests skipped: 0"
    rule = "!" * 78
    said = f"{skipped} GPU tests SKIPPED (BT_GPU=none): they passed without a GPU and prove nothing about it"
    return f"{rule}\n{said}\n{rule}"


def main(argv: list[str]) -> int:
    """Run the command after `--` in `argv` with `BT_GPU_SKIPS` set, then print `report` of what it wrote."""
    if argv[:1] != ["--"] or len(argv) < 2:
        print("usage: python tools/gpu_skips.py -- COMMAND [ARG...]", file=sys.stderr)
        return 2
    handle, name = tempfile.mkstemp(prefix="gpu-skips-", suffix=".txt")
    os.close(handle)
    path = Path(name)
    try:
        status = subprocess.run(argv[1:], env={**os.environ, "BT_GPU_SKIPS": name}, check=False).returncode  # noqa: S603  (the caller's own command, as poe runs it)
        print(report(path.read_text(encoding="utf-8").splitlines()))
    finally:
        path.unlink()
    return status


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
