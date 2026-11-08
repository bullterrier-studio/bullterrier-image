"""The Rust gates can fail: each Rust task of `poe check` rejects a known-bad change, naming the rule, and passes the
workspace as it is.

An architecture test of the tooling: its subject is the configuration (the poe tasks in pyproject.toml, the lints in
Cargo.toml, rustfmt.toml, deny.toml), so it runs those tasks, the way `uv run poe check` does, in a copy of the Rust
workspace into which one offence at a time is written. The copies share one build folder, so the dependencies are
compiled once, and nothing needs the network.
"""

import os
import shutil
import subprocess
import sys
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
# one xdist worker runs this module, so its copy of the workspace and its build folder are made once
pytestmark = [pytest.mark.xdist_group("rust-gates"), pytest.mark.gate]
WORKSPACE_FILES = ["pyproject.toml", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "rustfmt.toml", "deny.toml"]
CORE = "crates/core/src/lib.rs"
CORE_MANIFEST = "crates/core/Cargo.toml"


@dataclass(frozen=True)
class Verdict:
    returncode: int
    report: str  # what the tool printed, both streams


@pytest.fixture(scope="module")
def workspace(tmp_path_factory) -> Iterator[Path]:
    root = tmp_path_factory.mktemp("workspace")
    for name in WORKSPACE_FILES:
        shutil.copy(ROOT / name, root)
    shutil.copytree(ROOT / "crates", root / "crates")
    # files outside the crates that the core's tests read
    for outside in (Path("examples/assets/mosaic.svg"),):
        (root / outside).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(ROOT / outside, root / outside)
    (root / "tools").mkdir()
    shutil.copy(ROOT / "tools" / "run_capped.py", root / "tools")  # `rust-test` runs under it
    yield root
    shutil.rmtree(root)


@pytest.fixture(scope="module")
def cargo_env(tmp_path_factory) -> Iterator[dict[str, str]]:
    """`uv run`'s environment (PyO3 finds this Python), offline, uncoloured, with one build folder for every copy; the
    folder is removed afterwards (gigabytes)."""
    target = tmp_path_factory.mktemp("target")
    yield {
        **os.environ,
        "PATH": os.pathsep.join([str(Path(sys.executable).parent), os.environ.get("PATH", "")]),
        "CARGO_TARGET_DIR": str(target),
        "CARGO_NET_OFFLINE": "true",
        "CARGO_TERM_COLOR": "never",
        "NO_COLOR": "1",
    }
    shutil.rmtree(target)


def poe(workspace: Path, task: str, env: dict[str, str]) -> Verdict:
    """`task` is a poe task and the arguments after its name: `rust-test --lib probe::` runs only what they select."""
    # the simple executor: the commands run in this environment, as under `uv run`, without making one for the copy
    command = [sys.executable, "-m", "poethepoet", "--executor", "simple", *task.split()]
    result = subprocess.run(command, cwd=workspace, env=env, capture_output=True, encoding="utf-8", check=False)
    return Verdict(result.returncode, result.stdout + result.stderr)


@pytest.fixture
def edit(workspace, cargo_env) -> Iterator[Callable[[str, Callable[[str], str]], None]]:
    """`edit(relative, change)` rewrites one file of the copy with `change(text)` for one test; afterwards the copy is
    as it was, Cargo.lock included. A manifest edit updates the lock offline, as `cargo add` would."""
    touched = {"Cargo.lock": (workspace / "Cargo.lock").read_text(encoding="utf-8")}

    def change(relative: str, rewrite: Callable[[str], str]) -> None:
        path = workspace / relative
        touched.setdefault(relative, path.read_text(encoding="utf-8"))
        path.write_text(rewrite(path.read_text(encoding="utf-8")), encoding="utf-8")
        if relative.endswith("Cargo.toml"):
            subprocess.run(
                ["cargo", "metadata", "--offline", "--format-version", "1"],
                cwd=workspace,
                env=cargo_env,
                capture_output=True,
                check=True,
            )

    yield change
    for relative, text in touched.items():
        (workspace / relative).write_text(text, encoding="utf-8")


def appended(source: str) -> Callable[[str], str]:
    return lambda text: text + source


def replaced(old: str, new: str) -> Callable[[str], str]:
    def rewrite(text: str) -> str:
        assert old in text
        return text.replace(old, new)

    return rewrite


# One offence each: (task and the arguments that select what it runs, file, change, what the report must name: the
# rule, and where it was broken). Every kind of source a gate must see has a row: the core's lib.rs, an `ops` module, an
# effect file (pulled in by include!, which rustfmt does not follow), a test module of tests/it, the build script and
# the manifests.
OFFENCES = {
    "unformatted code": ("rust-fmt", CORE, appended("pub  fn  spaced( )->u8{1}\n"), ["Diff in", CORE]),
    "unformatted code in an effect file (which rustfmt does not reach through include!)": (
        "rust-fmt",
        "crates/core/src/effects/grade.rs",
        appended("pub  fn  spaced( )->u8{1}\n"),
        ["Diff in", "effects/grade.rs"],
    ),
    "unformatted code in the build script": (
        "rust-fmt",
        "crates/core/build.rs",
        appended("pub  fn  spaced( )->u8{1}\n"),
        ["Diff in", "build.rs"],
    ),
    "unformatted code in a test module": (
        "rust-fmt",
        "crates/core/tests/it/main.rs",
        appended("pub  fn  spaced( )->u8{1}\n"),
        ["Diff in", "tests/it/main.rs"],
    ),
    "an unwrap in an effect file": (
        "rust-lint",
        "crates/core/src/effects/grade.rs",
        appended("\n/// Probe.\n#[must_use]\npub fn probe(text: &str) -> u8 {\n    text.parse().unwrap()\n}\n"),
        ["unwrap_used", "effects/grade.rs"],
    ),
    "an unwrap in a test module": (
        "rust-lint",
        "crates/core/tests/it/main.rs",
        appended("\n/// Probe.\n#[must_use]\npub fn probe(text: &str) -> u8 {\n    text.parse().unwrap()\n}\n"),
        ["unwrap_used", "tests/it/main.rs"],
    ),
    "an unwrap": (
        "rust-lint",
        CORE,
        appended("\n/// Probe.\n#[must_use]\npub fn probe(text: &str) -> u8 {\n    text.parse().unwrap()\n}\n"),
        ["unwrap_used", CORE],
    ),
    "an expect": (
        "rust-lint",
        CORE,
        appended(
            '\n/// Probe.\n#[must_use]\npub fn probe(text: &str) -> u8 {\n    text.parse().expect("a number")\n}\n'
        ),
        ["expect_used", CORE],
    ),
    "a panic": (
        "rust-lint",
        CORE,
        appended('\n/// Probe.\npub fn probe() {\n    panic!("no");\n}\n'),
        ["#panic", CORE],
    ),
    "printing": (
        "rust-lint",
        CORE,
        appended('\n/// Probe.\npub fn probe() {\n    println!("hello");\n}\n'),
        ["print_stdout", CORE],
    ),
    "dead code": ("rust-lint", CORE, appended("\nfn never_called() {}\n"), ["dead-code", "never_called", CORE]),
    "a public function without documentation": (
        "rust-lint",
        CORE,
        appended("\npub fn undocumented() {}\n"),
        ["missing-docs", CORE],
    ),
    "pub where no user of the crate reaches it": (
        "rust-lint",
        "crates/core/src/ops.rs",
        appended("\n/// Probe.\npub fn reachable_only_inside() {}\n"),
        ["unreachable-pub", "crates/core/src/ops.rs"],
    ),
    "a declared parameter no test shows to change the result": (
        "rust-test --test parameters_are_read",
        "crates/core/src/effects/grade.rs",
        replaced(
            "\n}\n\n/// The graded picture.",
            "\n    /// A parameter nobody reads.\n    #[param(default = 0.0, unit = ratio, min = 0, max = 1)]\n"
            "    #[serde(default)]\n    pub(crate) unused: f32,\n}\n\n/// The graded picture.",
        ),
        ["parameters_are_read", "grade.unused is never read"],
    ),
    "unsafe code in the core": (
        "rust-lint",
        CORE,
        appended(
            "\n/// Probe.\n#[must_use]\npub fn probe(value: &u8) -> u8 {\n    unsafe { *std::ptr::from_ref(value) }\n}"
        ),
        ["unsafe_code", CORE],
    ),
    "a pedantic lint": (
        "rust-lint",
        CORE,
        appended("\n/// Probe.\n#[must_use]\npub fn probe(value: u32) -> u8 {\n    value as u8\n}\n"),
        ["cast_possible_truncation", CORE],
    ),
    "a broken link in the documentation": (
        "rust-doc",
        CORE,
        appended("\n/// Like [`Nowhere`].\npub fn probe() {}\n"),
        ["broken-intra-doc-links", "Nowhere"],
    ),
    "a dependency no code uses": (
        "rust-shear",
        CORE_MANIFEST,
        replaced("[dependencies]\n", '[dependencies]\nlibc = "0.2"\n'),
        ["unused dependency `libc`", CORE_MANIFEST],
    ),
    "PyO3 in the pure-Rust core": (
        "rust-deny",
        CORE_MANIFEST,
        replaced("[dependencies]\n", "[dependencies]\npyo3.workspace = true\n"),
        ["banned", "pyo3", "parent 'bullterrier-image = "],  # the core, by name
    ),
    "a licence outside the allow-list": (
        "rust-deny",
        CORE_MANIFEST,
        replaced("license.workspace = true", 'license = "GPL-3.0-only"'),
        ["rejected", "GPL-3.0-only"],
    ),
    "a failing Rust test": (
        "rust-test --lib probe::",
        CORE,
        appended(
            "\n#[cfg(test)]\nmod probe {\n    #[test]\n    fn fails() {\n        assert_eq!(1 + 1, 3);\n    }\n}\n"
        ),
        ["test result: FAILED", "probe::fails"],
    ),
}


@pytest.mark.parametrize(("task", "relative", "change", "named"), OFFENCES.values(), ids=OFFENCES.keys())
def test_each_offence_fails_its_gate_naming_the_rule(workspace, cargo_env, edit, task, relative, change, named):
    edit(relative, change)
    verdict = poe(workspace, task, cargo_env)
    assert verdict.returncode != 0, verdict.report
    for name in named:
        assert name in verdict.report, verdict.report


# --- stubtest: the stub against the built extension ---------------------------------------------------------------


def stubtest(checkout: Path, env: dict[str, str]) -> Verdict:
    # the gate's own task; the stub is read from the copy (mypy_path = "src" in its pyproject.toml)
    command = [sys.executable, "-m", "poethepoet", "stubtest"]
    result = subprocess.run(command, cwd=checkout, env=env, capture_output=True, encoding="utf-8", check=False)
    return Verdict(result.returncode, result.stdout + result.stderr)


STUB = "src/bullterrier_image/_native/_core.pyi"


@pytest.mark.typecheck
def test_the_stub_as_it_is_matches_the_extension(project_copy, copy_env):
    verdict = stubtest(project_copy("pyproject.toml"), copy_env)
    assert verdict.returncode == 0, verdict.report


@pytest.mark.typecheck
@pytest.mark.parametrize(
    ("old", "new", "named"),
    [
        pytest.param("effect: str", "name: str", "bullterrier_image._native._core.apply", id="a parameter renamed"),
        pytest.param(
            "def effects(*, declared: bool = False) -> list[str]: ...\n",
            "def effects(*, declared: bool = False) -> list[str]: ...\ndef ghost() -> None: ...\n",
            "bullterrier_image._native._core.ghost",
            id="a function the extension lacks",
        ),
    ],
)
def test_a_stub_that_disagrees_with_the_extension_fails_naming_the_function(project_copy, copy_env, old, new, named):
    checkout = project_copy("pyproject.toml")
    stub = checkout / STUB
    text = stub.read_text(encoding="utf-8")
    assert old in text
    stub.write_text(text.replace(old, new), encoding="utf-8")
    verdict = stubtest(checkout, copy_env)
    assert verdict.returncode != 0, verdict.report
    assert named in verdict.report, verdict.report
