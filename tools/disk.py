"""Keep the disk from filling: delete this checkout's incremental build output when the volume is nearly full.

Incremental compilation makes an edit-test loop 2.7 times faster and grows `target/debug/incremental` by tens of
gigabytes a day; a full disk fails the gate midway. When the volume of `target/` has less than `LEAST_FREE_GB` free,
`prune` deletes `target/debug/incremental` of this checkout only (the next build starts it again, about 40 s) and says
so in one line.

Interface:
    prune(free, least, folder) -> True if `folder` was deleted (it is there and `free` is below `least`)
    main(argv)                 -> exit code 0; prints one line when it pruned
"""

import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
INCREMENTAL = ROOT / "target" / "debug" / "incremental"
LEAST_FREE_GB = 15
GB = 1_000_000_000


def prune(free: int, least: int, folder: Path) -> bool:
    """Delete `folder` and return True if it exists and `free` bytes are fewer than `least`; else leave it."""
    if free >= least or not folder.is_dir():
        return False
    shutil.rmtree(folder)
    return True


def main(argv: list[str] | None = None) -> int:
    """Prune this checkout's incremental output when the disk is short of room."""
    del argv
    probe = INCREMENTAL if INCREMENTAL.exists() else ROOT
    free = shutil.disk_usage(probe).free
    if prune(free, LEAST_FREE_GB * GB, INCREMENTAL):
        print(
            f"disk: {free / GB:.1f} GB free, below {LEAST_FREE_GB}: deleted {INCREMENTAL.relative_to(ROOT)} "
            "(the next build starts it again, about 40 s)"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
