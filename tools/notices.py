"""THIRD-PARTY-NOTICES.md: the Rust crates the wheel is built from and the text of each licence they come under.

`cargo about` (pinned here) writes it from Cargo.lock, `about.toml` and `about.hbs`. The gate (in `project-gates`) fails
when the file is not what it would write, when a licence block has no text, or when the tool is not the pinned version;
`--write` writes the file.

Interface:
    empty_licences(text) -> the headings of the licence blocks of `text` that hold no licence text
    main(argv)           -> exit code: 0 the file is right (or was written), 1 a problem, named on stderr
"""

import argparse
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TARGET = ROOT / "THIRD-PARTY-NOTICES.md"
CARGO_ABOUT = "0.9.3"
INSTALL = f"cargo install --locked cargo-about@{CARGO_ABOUT} --features cli"
# a licence's text is longer than any sentence; the shortest in use (ISC) is about 700 characters, a stub is not
SHORTEST_TEXT = 200
BLOCK = re.compile(r"^## (?P<name>.+?)\n\nUsed by:\n.*?```text\n(?P<text>.*?)```", re.MULTILINE | re.DOTALL)


def empty_licences(text: str) -> list[str]:
    """The headings of the licence blocks of `text` (as the template writes them) whose text is missing or too short
    to be a licence."""
    return [block["name"] for block in BLOCK.finditer(text) if len(block["text"].strip()) < SHORTEST_TEXT]


def installed_version() -> str | None:
    """The version of `cargo about` on the PATH, or None when it is not there."""
    if shutil.which("cargo-about") is None:
        return None
    done = subprocess.run(["cargo-about", "--version"], capture_output=True, encoding="utf-8", check=False)  # noqa: S607  (a program found on PATH by its fixed name)
    found = re.search(r"\b(\d+\.\d+\.\d+)\b", done.stdout)
    return found.group(1) if found else None


def generate() -> str:
    """The notices `cargo about` writes for this checkout."""
    done = subprocess.run(
        ["cargo", "about", "generate", "--workspace", "--locked", "about.hbs"],  # noqa: S607
        cwd=ROOT,
        capture_output=True,
        encoding="utf-8",
        check=False,
    )
    if done.returncode != 0:
        raise RuntimeError(f"`cargo about generate` failed:\n{done.stderr.strip()}")
    return done.stdout


def main(argv: list[str] | None = None) -> int:
    """Check THIRD-PARTY-NOTICES.md, or with `--write` write it."""
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0] if __doc__ else None)
    parser.add_argument("--write", action="store_true", help="write the file instead of checking it")
    args = parser.parse_args(argv)
    found = installed_version()
    if found != CARGO_ABOUT:
        state = "is not installed" if found is None else f"is {found}"
        print(f"error: cargo-about {state}, the gate needs {CARGO_ABOUT}: run `{INSTALL}`", file=sys.stderr)
        return 1
    try:
        text = generate()
    except RuntimeError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    empty = empty_licences(text)
    if empty:
        print(f"error: no licence text for {', '.join(empty)}: a clarification in about.toml gives it", file=sys.stderr)
        return 1
    if args.write:
        TARGET.write_text(text, encoding="utf-8")
        return 0
    if not TARGET.is_file() or TARGET.read_text(encoding="utf-8") != text:
        print(
            "error: THIRD-PARTY-NOTICES.md is out of date: run `uv run python tools/notices.py --write`",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
