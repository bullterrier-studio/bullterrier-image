"""A crate's Rust integration tests are one binary: `tests/it/main.rs` and a module per area.

An architecture test of the repository layout. Every `tests/*.rs` is an executable of its own, and on macOS each new
one is scanned on its first launch (12 s measured; 62 of them made `cargo test` take 27 minutes). A file may stay a
binary of its own only when it needs its own process; it is listed in `ALLOWED` with the reason, and says it in its
first line.
"""

from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
# the test files that are binaries of their own: path -> why they need a process to themselves
ALLOWED = {
    "crates/core/tests/memory.rs": "a counting #[global_allocator] measures the whole process's peak heap",
    "crates/core/tests/parameters_are_read.rs": "it walks every effect in the registry, which other tests fill",
}
FIRST_LINE = "//! Own binary: "


def problems(root: Path, allowed: dict[str, str]) -> list[str]:
    """What is wrong with the top-level `tests/*.rs` files of the crates under `root`, one string per problem."""
    found = sorted(path.relative_to(root).as_posix() for path in (root / "crates").glob("*/tests/*.rs"))
    stray = [
        f"{path}: a test binary of its own; make it a module of tests/it/ (declared in its main.rs), "
        f"or list it in ALLOWED with the reason it needs its own process"
        for path in found
        if path not in allowed
    ]
    gone = [f"{path}: listed in ALLOWED but not there; remove it" for path in sorted(allowed) if path not in found]
    unsaid = [
        f"{path}: its first line must begin {FIRST_LINE!r} and say why"
        for path in found
        if path in allowed and not (root / path).read_text(encoding="utf-8").startswith(FIRST_LINE)
    ]
    return stray + gone + unsaid


def _tree(root: Path, files: dict[str, str]) -> Path:
    for name, text in files.items():
        (root / name).parent.mkdir(parents=True, exist_ok=True)
        (root / name).write_text(text, encoding="utf-8")
    return root


def test_the_crates_have_only_the_test_binaries_listed_here():
    assert problems(ROOT, ALLOWED) == []


OWN = {"crates/core/tests/own.rs": "an allocator"}


@pytest.mark.parametrize(
    ("files", "allowed", "expected"),
    [
        pytest.param(
            {
                "crates/core/tests/it/main.rs": "mod a;\n",
                "crates/core/tests/it/a.rs": "",
                "crates/core/tests/data.bin": "",
            },
            {},
            [],
            id="modules of the one binary and non-rust files are fine",
        ),
        pytest.param(
            {"crates/core/tests/own.rs": f"{FIRST_LINE}it installs an allocator.\n"},
            OWN,
            [],
            id="a listed binary that says why is fine",
        ),
        pytest.param(
            {"crates/core/tests/new_area.rs": "//! New.\n"},
            {},
            ["crates/core/tests/new_area.rs: a test binary of its own"],
            id="a new tests/*.rs is refused",
        ),
        pytest.param(
            {"crates/derive/tests/x.rs": ""},
            {},
            ["crates/derive/tests/x.rs: a test binary of its own"],
            id="any crate is held to it",
        ),
        pytest.param(
            {"crates/core/tests/own.rs": "//! No reason here.\n"},
            OWN,
            ["crates/core/tests/own.rs: its first line must begin"],
            id="a listed binary without its reason is refused",
        ),
        pytest.param(
            {}, OWN, ["crates/core/tests/own.rs: listed in ALLOWED but not there"], id="a stale listing is refused"
        ),
    ],
)
def test_the_checker_refuses_a_stray_binary_and_accepts_the_layout_it_wants(tmp_path, files, allowed, expected):
    found = problems(_tree(tmp_path, files), allowed)
    assert len(found) == len(expected)
    assert all(problem.startswith(start) for problem, start in zip(found, expected, strict=True))
