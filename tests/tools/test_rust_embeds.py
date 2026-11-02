"""A crate's source embeds only files inside the crate's own folder.

An architecture test of the repository layout. `cargo package` builds a crate from its own folder, so an
`include_str!("../../../../../src/...")` that reaches out of it compiles in the checkout and breaks the published crate.
"""

import posixpath
import re
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
EMBED = re.compile(r"""\binclude(?:_str|_bytes)?!\s*\(\s*"([^"]*)"\s*\)""")


def problems(root: Path) -> list[str]:
    """Each `include_str!`, `include_bytes!` or `include!` in `crates/*/src/**/*.rs` under `root` whose path leaves
    its crate's folder, one string per embed; a line that is a comment is not an embed."""
    found = []
    for source in sorted((root / "crates").glob("*/src/**/*.rs")):
        crate = source.relative_to(root).parts[:2]
        lines = source.read_text(encoding="utf-8").splitlines()
        for number, line in enumerate(lines, 1):
            if line.lstrip().startswith("//"):
                continue
            for target in EMBED.findall(line):
                resolved = posixpath.normpath(posixpath.join(source.parent.relative_to(root).as_posix(), target))
                if resolved.split("/")[:2] != list(crate):
                    where = source.relative_to(root).as_posix()
                    found.append(f"{where} line {number}: embeds {target!r}, which is outside {'/'.join(crate)}")
    return found


def embeds(root: Path) -> list[str]:
    """The embedded paths of the crates under `root`, to prove the scan sees them."""
    return sorted(
        target
        for source in (root / "crates").glob("*/src/**/*.rs")
        for target in EMBED.findall(source.read_text(encoding="utf-8"))
    )


def test_no_crate_embeds_a_file_outside_its_own_folder():
    assert problems(ROOT) == []


def test_the_scan_sees_the_files_the_core_embeds():
    assert any(target.endswith("data/regions/shards.toml") for target in embeds(ROOT))


def _tree(root: Path, files: dict[str, str]) -> Path:
    for name, text in files.items():
        (root / name).parent.mkdir(parents=True, exist_ok=True)
        (root / name).write_text(text, encoding="utf-8")
    return root


@pytest.mark.parametrize(
    ("files", "expected"),
    [
        pytest.param(
            {"crates/core/src/recipe/region/presets.rs": 'const A: &str = include_str!("../../../data/a.toml");\n'},
            [],
            id="a file in the crate's own data folder is fine",
        ),
        pytest.param(
            {"crates/core/src/lib.rs": 'static B: &[u8] = include_bytes!("../data/b.icc");\n'},
            [],
            id="include_bytes of a file in the crate is fine",
        ),
        pytest.param(
            {"crates/core/src/recipe/region/p.rs": 'const A: &str = include_str!("../../../../../src/a.toml");\n'},
            ["crates/core/src/recipe/region/p.rs line 1: embeds '../../../../../src/a.toml'"],
            id="a path with enough ../ to leave the crate is refused",
        ),
        pytest.param(
            {"crates/core/src/lib.rs": 'include!("../../derive/src/shared.rs");\n'},
            ["crates/core/src/lib.rs line 1: embeds '../../derive/src/shared.rs'"],
            id="a file of another crate is outside this one",
        ),
        pytest.param(
            {"crates/core/src/lib.rs": '//! `include_str!("../../../x")` shows.\n/// include_bytes!("../../../y")\n'},
            [],
            id="a comment that names an embed is not one",
        ),
    ],
)
def test_the_checker_refuses_an_embed_that_leaves_the_crate_and_accepts_one_inside(tmp_path, files, expected):
    found = problems(_tree(tmp_path, files))
    assert len(found) == len(expected)
    assert all(problem.startswith(start) for problem, start in zip(found, expected, strict=True))
