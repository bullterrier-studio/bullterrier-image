"""No code file grows without bound: tools/check_file_size.py holds source files to one budget and test files to a
larger one, and lets a file that was already over budget when the gate began shrink but never grow."""

import importlib.util
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]


def _load_checker():
    spec = importlib.util.spec_from_file_location("check_file_size", ROOT / "tools" / "check_file_size.py")
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


checker = _load_checker()

# throwaway trees use small budgets so a file over them stays a few lines long
SOURCE_BUDGET = 4
TESTS_BUDGET = 7


def _tree(root: Path, files: dict[str, int], exempt: dict[str, int] | None = None) -> Path:
    """A repository at `root` with a pyproject.toml holding the budgets and `exempt`, and each of `files` (path ->
    number of lines)."""
    exempt_lines = "".join(f'"{path}" = {size}\n' for path, size in (exempt or {}).items())
    (root / "pyproject.toml").write_text(
        f"[tool.file-size]\nsource = {SOURCE_BUDGET}\ntests = {TESTS_BUDGET}\n[tool.file-size.exempt]\n{exempt_lines}",
        encoding="utf-8",
    )
    for path, lines in files.items():
        (root / path).parent.mkdir(parents=True, exist_ok=True)
        (root / path).write_text("x\n" * lines, encoding="utf-8")
    return root


def test_the_repository_is_within_its_file_size_budgets():
    assert checker.check(ROOT) == []


@pytest.mark.parametrize(
    "path",
    ["src/pkg/mod.py", "src/pkg/deep/er/mod.py", "tools/dev.py", "crates/core/src/lib.rs", "crates/core/build.rs"],
)
def test_a_source_file_may_have_exactly_the_budget_but_not_a_line_more(tmp_path, path):
    assert checker.check(_tree(tmp_path, {path: SOURCE_BUDGET})) == []
    (tmp_path / path).write_text("x\n" * (SOURCE_BUDGET + 1), encoding="utf-8")
    [problem] = checker.check(tmp_path)
    assert path in problem
    assert f"{SOURCE_BUDGET + 1} lines" in problem
    assert f"{SOURCE_BUDGET}-line source budget" in problem
    assert "split it by concept" in problem.lower()


@pytest.mark.parametrize("path", ["tests/test_a.py", "tests/effects/x/test_x.py", "crates/core/tests/recipes.rs"])
def test_a_test_file_gets_the_larger_budget(tmp_path, path):
    assert checker.check(_tree(tmp_path, {path: TESTS_BUDGET})) == []
    (tmp_path / path).write_text("x\n" * (TESTS_BUDGET + 1), encoding="utf-8")
    [problem] = checker.check(tmp_path)
    assert path in problem
    assert f"{TESTS_BUDGET + 1} lines" in problem
    assert f"{TESTS_BUDGET}-line test budget" in problem


def test_a_last_line_without_a_newline_counts_as_a_line(tmp_path):
    _tree(tmp_path, {})
    (tmp_path / "tools").mkdir()
    (tmp_path / "tools" / "t.py").write_text("x\n" * SOURCE_BUDGET + "x", encoding="utf-8")
    assert len(checker.check(tmp_path)) == 1


@pytest.mark.parametrize(
    "path",
    [
        "src/pkg/fx.pyi",  # generated
        "src/pkg/presets/glow.toml",
        "src/pkg/effects/_template/effect.py.tmpl",
        "docs/guide.md",
        "docs/make.py",
        "examples/demo.py",
        "uv.lock",
        "README.md",
        "benches/run.py",
    ],
)
def test_a_file_outside_the_scope_is_not_measured(tmp_path, path):
    assert checker.check(_tree(tmp_path, {path: 50 * TESTS_BUDGET})) == []


def test_every_file_over_budget_is_reported_not_just_the_first(tmp_path):
    _tree(tmp_path, {"src/a.py": 9, "src/b.py": 9, "tests/test_c.py": 9, "src/fine.py": 1})
    problems = checker.check(tmp_path)
    assert [problem.split(":")[0] for problem in problems] == ["src/a.py", "src/b.py", "tests/test_c.py"]


@pytest.mark.parametrize(("lines", "ok"), [(5, True), (6, True), (10, True), (11, False)])
def test_an_exempt_file_may_shrink_but_never_grow_past_its_recorded_size(tmp_path, lines, ok):
    _tree(tmp_path, {"src/big.py": lines}, exempt={"src/big.py": 10})
    problems = checker.check(tmp_path)
    if ok:
        assert problems == []
    else:
        [problem] = problems
        assert "src/big.py" in problem
        assert "11 lines" in problem
        assert "recorded 10" in problem
        assert "may only shrink" in problem


def test_an_exempt_test_file_is_held_to_its_record_too(tmp_path):
    _tree(tmp_path, {"tests/test_big.py": 21}, exempt={"tests/test_big.py": 20})
    [problem] = checker.check(tmp_path)
    assert "tests/test_big.py" in problem
    assert "may only shrink" in problem


def test_an_entry_for_a_file_now_within_budget_is_stale(tmp_path):
    _tree(tmp_path, {"src/small.py": SOURCE_BUDGET}, exempt={"src/small.py": 10})
    [problem] = checker.check(tmp_path)
    assert "src/small.py" in problem
    assert f"{SOURCE_BUDGET} lines" in problem
    assert "remove it from [tool.file-size.exempt]" in problem


def test_an_entry_for_a_file_that_does_not_exist_is_stale(tmp_path):
    _tree(tmp_path, {}, exempt={"src/gone.py": 10})
    [problem] = checker.check(tmp_path)
    assert "src/gone.py" in problem
    assert "remove it from [tool.file-size.exempt]" in problem


def test_main_reports_each_problem_and_fails(tmp_path, capsys):
    _tree(tmp_path, {"src/a.py": 9}, exempt={"src/gone.py": 10})
    assert checker.main([str(tmp_path)]) == 1
    err = capsys.readouterr().err
    assert "src/a.py" in err
    assert "src/gone.py" in err


def test_main_is_silent_and_succeeds_on_a_tree_within_budget(tmp_path, capsys):
    _tree(tmp_path, {"src/a.py": 2, "tests/test_a.py": 3, "src/big.py": 8}, exempt={"src/big.py": 8})
    assert checker.main([str(tmp_path)]) == 0
    captured = capsys.readouterr()
    assert captured.out == ""
    assert captured.err == ""
