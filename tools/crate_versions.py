"""The two published crates carry the package's version.

    python tools/crate_versions.py VERSION   # write it: [workspace.package] version, the derive requirement, the lock
                                             # file (`poe release` runs this as its `after-bump` hook)
    python tools/crate_versions.py           # check: the crates, the derive requirement and pyproject.toml agree

VERSION is PEP 440 as release.yml accepts it (0.2.0, 0.2.0rc1); the crates spell a pre-release as SemVer does
(0.2.0-rc.1). Run from the repository root.
"""

import re
import subprocess
import sys
import tomllib
from pathlib import Path

PYPROJECT = Path("pyproject.toml")
CARGO = Path("Cargo.toml")
PRE_RELEASE = {"a": "alpha", "b": "beta", "rc": "rc"}
PEP440 = re.compile(r"(?P<release>\d+\.\d+\.\d+)(?:(?P<kind>a|b|rc)(?P<number>\d+))?")
WORKSPACE_VERSION = re.compile(r'(?m)^(version = ")[^"]+(")')
DERIVE_REQUIREMENT = re.compile(r'(?m)^(bullterrier-image-derive = \{[^}\n]*version = "=)[^"]+(")')


def semver(version: str) -> str:
    """The SemVer spelling of a release version: 0.2.0 stays, 0.2.0rc1 is 0.2.0-rc.1."""
    found = PEP440.fullmatch(version)
    if found is None:
        raise ValueError(f"{version!r} is not MAJOR.MINOR.PATCH with an optional aN, bN or rcN")
    if found["kind"] is None:
        return found["release"]
    return f"{found['release']}-{PRE_RELEASE[found['kind']]}.{found['number']}"


def problems() -> list[str]:
    """What disagrees: the crates' version and the derive requirement against pyproject.toml's version."""
    expected = semver(tomllib.loads(PYPROJECT.read_text(encoding="utf-8"))["project"]["version"])
    text = CARGO.read_text(encoding="utf-8")
    cargo = tomllib.loads(text)
    found = {
        "[workspace.package] version": cargo["workspace"]["package"]["version"],
        "the derive dependency's version": cargo["workspace"]["dependencies"]["bullterrier-image-derive"]["version"],
    }
    return [
        f"{what} is {value!r}, pyproject.toml's version is {expected!r}"
        for what, value in found.items()
        if value not in (expected, f"={expected}")
    ]


def write(version: str) -> None:
    """Put `version` (PEP 440) in the crates' version, the derive requirement and the lock file."""
    spelled = semver(version)
    text = CARGO.read_text(encoding="utf-8")
    text, changed = WORKSPACE_VERSION.subn(rf"\g<1>{spelled}\g<2>", text, count=1)
    text, required = DERIVE_REQUIREMENT.subn(rf"\g<1>{spelled}\g<2>", text, count=1)
    if not (changed and required):
        raise SystemExit(
            "Cargo.toml has no [workspace.package] version or no versioned bullterrier-image-derive dependency"
        )
    CARGO.write_text(text, encoding="utf-8", newline="\n")
    subprocess.run(["cargo", "update", "--workspace", "--offline", "--quiet"], check=True)  # noqa: S607  cargo from PATH, as every tool


def main(argv: list[str]) -> int:
    """Write the version given, or check the files agree."""
    if argv:
        write(argv[0])
        return 0
    found = problems()
    print("\n".join(found), file=sys.stderr)
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
