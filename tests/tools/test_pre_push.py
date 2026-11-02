"""tools/pre_push.py decides whether a push must pass the whole local gate first: pushes to main, anything that is not
a branch (a release tag) and any push while local main has code its remote-tracking branch lacks do; any other branch
is left to CI, so feature-branch pushes stay quick, and so is a push that changes only documentation. The tests use
real throwaway repositories with a bare repository as the remote."""

import importlib.util
import shutil
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]


def _load():
    spec = importlib.util.spec_from_file_location("pre_push", ROOT / "tools" / "pre_push.py")
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


pre_push = _load()


@pytest.mark.parametrize("ref", ["refs/heads/main", "refs/tags/v1.2.3", "refs/tags/v1.2.3rc1", "", "main"])
def test_main_and_tags_and_anything_unclear_must_pass_the_gate_whatever_main_looks_like(git_repo, monkeypatch, ref):
    git_repo.commit("chore: start", {"start.txt": "start"})
    git_repo.add_remote()
    git_repo.git("push", "-q", "origin", "main")  # main is level with origin/main
    monkeypatch.chdir(git_repo.path)
    assert pre_push.must_run(ref)


def level(repo):
    repo.commit("chore: start", {"start.txt": "start"})
    repo.add_remote()
    repo.git("push", "-q", "origin", "main")


def ahead_with_code(repo):
    level(repo)
    repo.commit("feat: code", {"src/code.py": "x = 1"})


def ahead_with_documentation(repo):
    level(repo)
    repo.commit("docs: page", {"README.md": "x", "docs/images/logo.svg": "<svg/>"})


def behind(repo):
    level(repo)
    repo.commit("feat: code", {"src/code.py": "x = 1"})
    repo.git("push", "-q", "origin", "main")
    repo.git("reset", "-q", "--hard", "HEAD~1")


def diverged(repo):
    level(repo)
    repo.commit("feat: code", {"src/code.py": "x = 1"})
    repo.git("push", "-q", "origin", "main")
    repo.git("reset", "-q", "--hard", "HEAD~1")
    repo.commit("feat: other code", {"src/other.py": "x = 2"})


def ahead_without_changes(repo):
    level(repo)
    repo.git("commit", "-q", "--allow-empty", "-m", "chore: nothing")


def no_remote_tracking_main(repo):
    repo.commit("feat: code", {"src/code.py": "x = 1"})
    repo.add_remote()  # nothing pushed yet


def no_remote(repo):
    repo.commit("feat: code", {"src/code.py": "x = 1"})


def no_local_main(repo):
    level(repo)
    repo.git("switch", "-q", "-c", "feat/x")
    repo.git("branch", "-q", "-D", "main")


def remote_named_upstream_is_level_origin_is_not(repo):
    repo.commit("chore: start", {"start.txt": "start"})
    repo.add_remote()  # origin lacks main
    repo.add_remote("upstream")
    repo.git("push", "-q", "upstream", "main")


@pytest.mark.parametrize(
    ("state", "gate_runs"),
    [
        pytest.param(level, False, id="main level with origin/main"),
        pytest.param(ahead_with_documentation, False, id="main ahead by documentation only"),
        pytest.param(behind, False, id="main behind origin/main"),
        pytest.param(ahead_with_code, True, id="main ahead by code"),
        pytest.param(diverged, True, id="main and origin/main each with commits of their own"),
        pytest.param(ahead_without_changes, True, id="main ahead by a commit that changes nothing: no proof"),
        pytest.param(no_remote_tracking_main, True, id="no origin/main: nothing says main is level"),
        pytest.param(no_remote, True, id="no remote at all"),
        pytest.param(no_local_main, True, id="no local main"),
        pytest.param(
            remote_named_upstream_is_level_origin_is_not,
            True,
            id="origin lacks main (the remote asked about is the default one)",
        ),
    ],
)
@pytest.mark.parametrize("ref", ["refs/heads/feat/x", "refs/heads/main2"])
def test_a_branch_push_runs_the_gate_while_main_has_code_its_remote_tracking_branch_lacks(
    git_repo, monkeypatch, state, gate_runs, ref
):
    state(git_repo)
    monkeypatch.chdir(git_repo.path)
    assert pre_push.must_run(ref) is gate_runs


def test_the_remote_pre_commit_names_decides_which_remote_tracking_main_counts(git_repo, monkeypatch):
    remote_named_upstream_is_level_origin_is_not(git_repo)
    monkeypatch.chdir(git_repo.path)
    assert pre_push.must_run("refs/heads/feat/x", "origin")
    assert not pre_push.must_run("refs/heads/feat/x", "upstream")


def test_a_feature_branch_push_with_main_level_skips_the_gate_and_succeeds(git_repo, monkeypatch, capsys):
    """With uv hidden a gate that did run would show as a failure here, never as the whole gate running."""
    level(git_repo)
    monkeypatch.chdir(git_repo.path)
    monkeypatch.setenv("PRE_COMMIT_REMOTE_BRANCH", "refs/heads/feature/x")
    monkeypatch.setattr(shutil, "which", lambda _name: None)
    assert pre_push.main() == 0
    assert "uv is not on the PATH" not in capsys.readouterr().out


@pytest.mark.parametrize(
    ("state", "feature_changes", "remote_name", "gate_runs"),
    [
        pytest.param(ahead_with_code, {"docs/page.md": "x"}, None, True, id="feat/x first (only a page), main ahead"),
        pytest.param(ahead_with_code, {"src/other.py": "y = 2"}, None, True, id="feat/x first with code, main ahead"),
        pytest.param(level, {"src/other.py": "y = 2"}, None, False, id="feat/x first with code, main level"),
        pytest.param(level, {"README.md": "y"}, None, False, id="feat/x first with a page, main level"),
        pytest.param(ahead_with_documentation, {"src/other.py": "y = 2"}, None, False, id="main ahead by a page only"),
        pytest.param(no_remote_tracking_main, {"README.md": "y"}, None, True, id="no origin/main"),
        pytest.param(
            remote_named_upstream_is_level_origin_is_not,
            {"README.md": "y"},
            "upstream",
            False,
            id="PRE_COMMIT_REMOTE_NAME names the remote whose main is level",
        ),
        pytest.param(
            remote_named_upstream_is_level_origin_is_not,
            {"README.md": "y"},
            None,
            True,
            id="origin is the default remote, and its main is not level",
        ),
    ],
)
def test_a_multi_ref_push_runs_the_gate_whichever_ref_comes_first(
    git_repo, monkeypatch, capsys, state, feature_changes, remote_name, gate_runs
):
    """`git push origin feat/x main` and `git push --all` report feat/x to the hook, and its pushed range is
    feat/x's own; the gate runs because main, which is also pushed, has code."""
    state(git_repo)
    main = git_repo.git("rev-parse", "main")
    git_repo.git("switch", "-q", "-c", "feat/x")
    feature = git_repo.commit("feat: on the branch", feature_changes)
    git_repo.git("switch", "-q", "main")
    monkeypatch.chdir(git_repo.path)
    monkeypatch.setenv("PRE_COMMIT_REMOTE_BRANCH", "refs/heads/feat/x")
    monkeypatch.setenv("PRE_COMMIT_FROM_REF", main)
    monkeypatch.setenv("PRE_COMMIT_TO_REF", feature)
    if remote_name:
        monkeypatch.setenv("PRE_COMMIT_REMOTE_NAME", remote_name)
    monkeypatch.setattr(shutil, "which", lambda _name: None)
    assert pre_push.main() == (1 if gate_runs else 0)
    assert ("uv is not on the PATH" in capsys.readouterr().out) is gate_runs


@pytest.mark.parametrize(
    ("path", "is_documentation"),
    [
        pytest.param("README.md", True, id="the README"),
        pytest.param("CONTRIBUTING.md", True, id="a top-level page"),
        pytest.param("docs/roadmap.md", True, id="a page in docs"),
        pytest.param("docs/images/logo.svg", True, id="a picture in docs/images"),
        pytest.param("docs/images/banner/wide.jpg", True, id="a nested picture"),
        pytest.param("docs/images/a.png", True, id="a png"),
        pytest.param("docs/images/B.SVG", True, id="a picture with a capitalised extension"),
        pytest.param("docs/images/photo.webp", True, id="a webp"),
        pytest.param("docs/images/make.py", False, id="a script in docs/images"),
        pytest.param("docs/images/banner/make.sh", False, id="a script in a subfolder of docs/images"),
        pytest.param("docs/images/README", False, id="a file without a picture extension"),
        pytest.param("docs/notes/ideas.md", False, id="a page in a subfolder of docs"),
        pytest.param("docs/make_banner.py", False, id="a script in docs"),
        pytest.param("docs/data.json", False, id="data in docs"),
        pytest.param("crates/core/README.md", False, id="a page inside a crate, which cargo packages"),
        pytest.param("src/bullterrier_image/notes.md", False, id="a markdown file inside src"),
        pytest.param("src/bullterrier_image/errors.py", False, id="a source file"),
        pytest.param("crates/core/src/lib.rs", False, id="a Rust file"),
        pytest.param("tests/tools/test_pre_push.py", False, id="a test"),
        pytest.param("examples/star.py", False, id="a program in examples, which a test runs"),
        pytest.param("Cargo.lock", False, id="the Rust lock file"),
        pytest.param("uv.lock", False, id="the Python lock file"),
        pytest.param(".github/workflows/ci.yml", False, id="a workflow"),
    ],
)
def test_only_pages_and_pictures_count_as_documentation(path, is_documentation):
    assert pre_push.is_documentation(path) is is_documentation


@pytest.mark.parametrize(
    ("changed", "gate_runs"),
    [
        pytest.param(
            {"README.md": "x", "docs/images/logo.svg": "<svg/>"}, False, id="a page and a picture skip the gate"
        ),
        pytest.param({"README.md": "x", "src/code.py": "x = 1"}, True, id="a page with one code file runs the gate"),
        pytest.param({"src/code.py": "x = 1"}, True, id="a code file alone runs the gate"),
        pytest.param({"docs/images/make.py": "x = 1"}, True, id="a script in docs/images runs the gate"),
        pytest.param({"docs/images/a.png": "x"}, False, id="a picture in docs/images skips the gate"),
    ],
)
def test_a_push_of_only_documentation_skips_the_gate(git_repo, monkeypatch, capsys, changed, gate_runs):
    """The gate is `uv run poe check`; with uv hidden it shows as an attempt that fails, so the test never runs it."""
    before = git_repo.commit("chore: start", {"start.txt": "start"})
    after = git_repo.commit("docs: change", changed)
    monkeypatch.chdir(git_repo.path)
    monkeypatch.setenv("PRE_COMMIT_REMOTE_BRANCH", "refs/heads/main")
    monkeypatch.setenv("PRE_COMMIT_FROM_REF", before)
    monkeypatch.setenv("PRE_COMMIT_TO_REF", after)
    monkeypatch.setattr(shutil, "which", lambda _name: None)
    assert pre_push.main() == (1 if gate_runs else 0)
    assert ("uv is not on the PATH" in capsys.readouterr().out) is gate_runs


def test_a_push_whose_range_is_unknown_runs_the_gate(monkeypatch, capsys):
    monkeypatch.setenv("PRE_COMMIT_REMOTE_BRANCH", "refs/heads/main")
    monkeypatch.delenv("PRE_COMMIT_FROM_REF", raising=False)
    monkeypatch.delenv("PRE_COMMIT_TO_REF", raising=False)
    monkeypatch.setattr(shutil, "which", lambda _name: None)
    assert pre_push.main() == 1
    assert "uv is not on the PATH" in capsys.readouterr().out


@pytest.mark.parametrize(
    ("from_ref", "to_ref"),
    [
        pytest.param("0" * 40, "HEAD", id="a pushed branch that is new on the remote: no earlier commit"),
        pytest.param("f" * 40, "HEAD", id="a commit this clone does not have"),
        pytest.param("HEAD", "", id="an empty end of the range"),
        pytest.param("", "HEAD", id="an empty start of the range"),
    ],
)
def test_a_range_git_cannot_read_runs_the_gate(git_repo, monkeypatch, capsys, from_ref, to_ref):
    git_repo.commit("docs: only a page", {"README.md": "x"})
    monkeypatch.chdir(git_repo.path)
    monkeypatch.setenv("PRE_COMMIT_REMOTE_BRANCH", "refs/heads/main")
    monkeypatch.setenv("PRE_COMMIT_FROM_REF", from_ref)
    monkeypatch.setenv("PRE_COMMIT_TO_REF", to_ref)
    monkeypatch.setattr(shutil, "which", lambda _name: None)
    assert pre_push.main() == 1
    assert "uv is not on the PATH" in capsys.readouterr().out


def test_a_push_that_changes_nothing_runs_the_gate(git_repo, monkeypatch, capsys):
    """An empty list of changed files is no proof that only documentation changed."""
    head = git_repo.commit("docs: a page", {"README.md": "x"})
    monkeypatch.chdir(git_repo.path)
    monkeypatch.setenv("PRE_COMMIT_REMOTE_BRANCH", "refs/heads/main")
    monkeypatch.setenv("PRE_COMMIT_FROM_REF", head)
    monkeypatch.setenv("PRE_COMMIT_TO_REF", head)
    monkeypatch.setattr(shutil, "which", lambda _name: None)
    assert pre_push.main() == 1
    assert "uv is not on the PATH" in capsys.readouterr().out


@pytest.mark.parametrize(
    ("main_changes", "gate_runs"),
    [
        pytest.param({"docs/roadmap.md": "y"}, False, id="a tag of documentation, and main ahead by documentation"),
        pytest.param({"src/code.py": "x = 1"}, True, id="a tag of documentation does not hide code in main"),
    ],
)
def test_a_documentation_only_push_of_a_tag_skips_the_gate_unless_main_has_code(
    git_repo, monkeypatch, capsys, main_changes, gate_runs
):
    before = git_repo.commit("chore: start", {"start.txt": "start"})
    git_repo.add_remote()
    git_repo.git("push", "-q", "origin", "main")
    after = git_repo.commit("docs: change", {"docs/roadmap.md": "x"})
    git_repo.commit("chore: main change", main_changes)
    monkeypatch.chdir(git_repo.path)
    monkeypatch.setenv("PRE_COMMIT_REMOTE_BRANCH", "refs/tags/v0.1.0")
    monkeypatch.setenv("PRE_COMMIT_FROM_REF", before)
    monkeypatch.setenv("PRE_COMMIT_TO_REF", after)
    monkeypatch.setattr(shutil, "which", lambda _name: None)
    assert pre_push.main() == (1 if gate_runs else 0)
    assert ("uv is not on the PATH" in capsys.readouterr().out) is gate_runs


def test_a_rename_of_code_into_docs_runs_the_gate(git_repo, monkeypatch, capsys):
    """With rename detection `git diff --name-only` lists only the new name, `docs/lib.md`: that would skip the gate."""
    before = git_repo.commit("chore: start", {"src/lib.rs": "pub fn draw() -> u32 {\n    1\n}\n" * 20})
    (git_repo.path / "docs").mkdir()
    git_repo.git("mv", "src/lib.rs", "docs/lib.md")
    git_repo.git("commit", "-q", "-m", "docs: move")
    after = git_repo.git("rev-parse", "HEAD")
    assert git_repo.git("diff", "--name-status", "-M", f"{before}..{after}").startswith("R100")  # git sees a rename
    monkeypatch.chdir(git_repo.path)
    monkeypatch.setenv("PRE_COMMIT_REMOTE_BRANCH", "refs/heads/main")
    monkeypatch.setenv("PRE_COMMIT_FROM_REF", before)
    monkeypatch.setenv("PRE_COMMIT_TO_REF", after)
    monkeypatch.setattr(shutil, "which", lambda _name: None)
    assert pre_push.main() == 1
    assert "uv is not on the PATH" in capsys.readouterr().out


# A gate that already passed on exactly this tree is not repeated --------------------------------------------------


def passed_gate(
    repo, *, dirty_at_start=False, tidied_between=False, change_between=None, dirty_at_end=False, skip_start=False
):
    """Run what `poe check` does around the gate in `repo`: note the start, let `change_between` happen (a commit), and
    note the pass. `dirty_at_start` and `dirty_at_end` leave an uncommitted file at that moment; `tidied_between`
    removes the file again before the end."""
    if dirty_at_start:
        (repo.path / "scratch.txt").write_text("x", encoding="utf-8")
    if not skip_start:
        pre_push.record_gate_start()
    if tidied_between:
        (repo.path / "scratch.txt").unlink()
    if change_between:
        repo.commit("chore: change", change_between)
    if dirty_at_end:
        (repo.path / "scratch.txt").write_text("y", encoding="utf-8")
    pre_push.record_gate_pass()


def test_a_gate_that_passed_on_a_clean_tree_is_remembered_for_that_tree_and_no_other(git_repo, monkeypatch):
    first = git_repo.commit("chore: start", {"start.txt": "start"})
    monkeypatch.chdir(git_repo.path)
    assert not pre_push.gate_passed_on(first)

    passed_gate(git_repo)

    assert pre_push.gate_passed_on(first)
    second = git_repo.commit("chore: more", {"more.txt": "more"})
    assert not pre_push.gate_passed_on(second), "another tree is not the one that was checked"
    assert pre_push.gate_passed_on(first), "the checked tree is still remembered"


def test_a_commit_with_the_same_tree_counts_as_the_tree_that_passed(git_repo, monkeypatch):
    git_repo.commit("chore: start", {"start.txt": "start"})
    monkeypatch.chdir(git_repo.path)
    passed_gate(git_repo)
    git_repo.git("commit", "-q", "--allow-empty", "-m", "chore: same tree, new commit")
    assert pre_push.gate_passed_on("HEAD")


@pytest.mark.parametrize(
    "run",
    [
        pytest.param({"dirty_at_start": True}, id="uncommitted files when it began"),
        pytest.param(
            {"dirty_at_start": True, "tidied_between": True}, id="uncommitted files when it began, gone when it ended"
        ),
        pytest.param({"dirty_at_end": True}, id="uncommitted files when it ended"),
        pytest.param({"change_between": {"other.txt": "o"}}, id="a commit while it ran"),
        pytest.param({"skip_start": True}, id="no note of when it began"),
    ],
)
def test_a_pass_is_not_remembered_unless_the_tree_was_the_same_and_clean_throughout(git_repo, monkeypatch, run):
    git_repo.commit("chore: start", {"start.txt": "start"})
    monkeypatch.chdir(git_repo.path)
    passed_gate(git_repo, **run)
    assert not pre_push.gate_passed_on("HEAD")
    assert not pre_push.gate_passed_on("HEAD~1" if run.get("change_between") else "HEAD")


def test_outside_a_repository_nothing_is_remembered_and_nothing_fails(tmp_path, monkeypatch):
    monkeypatch.chdir(tmp_path)
    pre_push.record_gate_start()
    pre_push.record_gate_pass()
    assert not pre_push.gate_passed_on("HEAD")


@pytest.mark.parametrize(
    ("checked", "gate_runs"),
    [
        pytest.param(True, False, id="the pushed tree passed the gate"),
        pytest.param(False, True, id="it did not"),
    ],
)
def test_a_push_of_a_tree_that_already_passed_the_gate_skips_it(git_repo, monkeypatch, capsys, checked, gate_runs):
    """With uv hidden a gate that did run shows as a failure."""
    before = git_repo.commit("chore: start", {"start.txt": "start"})
    after = git_repo.commit("feat: code", {"src/code.py": "x = 1"})
    monkeypatch.chdir(git_repo.path)
    if checked:
        passed_gate(git_repo)
    monkeypatch.setenv("PRE_COMMIT_REMOTE_BRANCH", "refs/heads/main")
    monkeypatch.setenv("PRE_COMMIT_FROM_REF", before)
    monkeypatch.setenv("PRE_COMMIT_TO_REF", after)
    monkeypatch.setattr(shutil, "which", lambda _name: None)
    assert pre_push.main() == (1 if gate_runs else 0)
    assert ("uv is not on the PATH" in capsys.readouterr().out) is gate_runs


def test_a_push_of_a_newer_tree_than_the_one_that_passed_runs_the_gate(git_repo, monkeypatch, capsys):
    before = git_repo.commit("chore: start", {"start.txt": "start"})
    git_repo.commit("feat: code", {"src/code.py": "x = 1"})
    monkeypatch.chdir(git_repo.path)
    passed_gate(git_repo)
    after = git_repo.commit("feat: more code", {"src/code.py": "x = 2"})
    monkeypatch.setenv("PRE_COMMIT_REMOTE_BRANCH", "refs/heads/main")
    monkeypatch.setenv("PRE_COMMIT_FROM_REF", before)
    monkeypatch.setenv("PRE_COMMIT_TO_REF", after)
    monkeypatch.setattr(shutil, "which", lambda _name: None)
    assert pre_push.main() == 1
    assert "uv is not on the PATH" in capsys.readouterr().out


def test_a_push_right_after_a_passed_gate_does_not_run_it_again(monkeypatch):
    monkeypatch.setenv("PRE_COMMIT_REMOTE_BRANCH", "refs/heads/main")
    monkeypatch.setenv("BT_IMAGE_GATE_PASSED", "1")
    assert pre_push.main() == 0
