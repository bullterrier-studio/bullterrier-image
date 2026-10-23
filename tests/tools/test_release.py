"""tools/release.py cuts a release on main: it bumps the version, writes the changelog section, commits and tags, and
never pushes; it refuses, before changing anything, when the repository or the bump is not fit for a release.

Each test runs the script in a throwaway repository with a local bare `origin` (tests/tools/conftest.py). Its project
uses this repository's real git-cliff configuration, and a `poe check` task the test chooses (passing or failing) in
place of the real gate.
"""

import re
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
REPOSITORY = "https://example.com/owner/demo"
PREAMBLE = "# Changelog\n\nAll notable changes.\n\n"
FIRST_SECTION = "## [0.1.0] - 2026-10-23\n\nThe first release.\n"
FIRST_LINK = f"[0.1.0]: {REPOSITORY}/releases/tag/v0.1.0\n"
DATED = r"\d{4}-\d{2}-\d{2}"


def git_cliff_tables() -> str:
    """This project's `[tool.git-cliff...]` tables, as written in its pyproject.toml."""
    tables = re.split(r"(?m)^(?=\[)", (ROOT / "pyproject.toml").read_text(encoding="utf-8"))
    return "".join(table for table in tables if table.startswith("[tool.git-cliff"))


def make_project(git_repo, *, version="0.1.0", changelog=PREAMBLE + FIRST_SECTION + "\n" + FIRST_LINK, gate="pass"):
    """A project at `version` on main, pushed to origin; its `poe check` runs `python -c <gate>`."""
    git_repo.write(
        "pyproject.toml",
        f'[project]\nname = "demo"\nversion = "{version}"\nrequires-python = ">=3.12"\ndependencies = []\n\n'
        f'[project.urls]\nRepository = "{REPOSITORY}"\n\n'
        f"[tool.poe.tasks]\ncheck = \"python -c '{gate}'\"\n\n" + git_cliff_tables(),
    )
    git_repo.write("CHANGELOG.md", changelog)
    lock = git_repo.run_uv("lock")
    assert lock.returncode == 0, lock.stderr
    git_repo.commit("feat: first")
    git_repo.push()


def released_once(git_repo, **project) -> None:
    """A project whose v0.1.0 is tagged, followed by user-facing commits and some that a changelog leaves out."""
    make_project(git_repo, **project)
    git_repo.git("tag", "-a", "v0.1.0", "-m", "v0.1.0")
    git_repo.commit("feat(glow): add a falloff parameter")
    git_repo.commit("docs: explain the recipes")
    git_repo.commit("fix: stop rounding twice")
    git_repo.push()


def snapshot(git_repo) -> dict:
    """What a refused release must leave as it was."""
    return {
        "head": git_repo.git("rev-parse", "HEAD"),
        "status": git_repo.git("status", "--porcelain"),
        "tags": git_repo.git("tag", "--list"),
        "pyproject": git_repo.read("pyproject.toml"),
        "changelog": git_repo.read("CHANGELOG.md"),
    }


def project_version(git_repo) -> str:
    return tomllib.loads(git_repo.read("pyproject.toml"))["project"]["version"]


def locked_version(git_repo) -> str:
    lock = tomllib.loads(git_repo.read("uv.lock"))
    return next(package["version"] for package in lock["package"] if package["name"] == "demo")


# ---------------------------------------------------------------------------------------------------------------------
# known-good


def test_a_minor_release_bumps_writes_the_section_commits_and_tags_but_never_pushes(git_repo):
    released_once(git_repo)
    origin_main = git_repo.git("rev-parse", "origin/main")

    result = git_repo.run_tool("release.py", "minor")

    assert result.returncode == 0, result.stdout + result.stderr
    assert project_version(git_repo) == "0.2.0"
    assert locked_version(git_repo) == "0.2.0"
    changelog = git_repo.read("CHANGELOG.md")
    section = re.search(rf"\A{re.escape(PREAMBLE)}## \[0\.2\.0\] - {DATED}\n\n(.*?)\n## \[0\.1\.0\]", changelog, re.S)
    assert section, changelog
    added, fixed = "### Added\n\n- glow: Add a falloff parameter", "### Fixed\n\n- Stop rounding twice"
    assert section.group(1).strip() == f"{added}\n\n{fixed}"  # in Keep a Changelog's order; the docs commit left out
    assert FIRST_SECTION in changelog  # the earlier section is kept as it was
    assert b"\r" not in (git_repo.path / "CHANGELOG.md").read_bytes()  # line endings as committed, on Windows too
    assert changelog.endswith(f"[0.2.0]: {REPOSITORY}/compare/v0.1.0...v0.2.0\n{FIRST_LINK}")
    assert git_repo.git("log", "-1", "--format=%s") == "chore(release): v0.2.0"
    assert git_repo.git("status", "--porcelain") == ""
    assert git_repo.git("cat-file", "-t", "v0.2.0") == "tag"  # annotated
    assert git_repo.git("rev-parse", "v0.2.0^{commit}") == git_repo.git("rev-parse", "HEAD")
    assert "git push --atomic origin main v0.2.0" in result.stdout
    # nothing reached origin
    assert git_repo.git("ls-remote", "origin", "refs/heads/main").split()[0] == origin_main
    assert git_repo.git("ls-remote", "--tags", "origin") == ""


def test_the_pushed_release_is_one_the_release_workflow_accepts(git_repo):
    released_once(git_repo)
    assert git_repo.run_tool("release.py", "patch").returncode == 0
    git_repo.git("push", "-q", "--atomic", "origin", "main", "v0.1.1")  # the command release.py prints

    accepted = git_repo.run_tool("check_release.py", "v0.1.1")

    assert accepted.returncode == 0, accepted.stderr


def test_a_dry_run_shows_the_version_and_section_and_changes_nothing(git_repo):
    released_once(git_repo)
    before = snapshot(git_repo)

    result = git_repo.run_tool("release.py", "minor", "--dry-run")

    assert result.returncode == 0, result.stdout + result.stderr
    assert "v0.2.0" in result.stdout
    assert "- glow: Add a falloff parameter" in result.stdout
    assert snapshot(git_repo) == before


def test_an_explicit_pre_release_version_is_released_and_tagged(git_repo):
    released_once(git_repo)

    result = git_repo.run_tool("release.py", "0.2.0rc1")

    assert result.returncode == 0, result.stdout + result.stderr
    assert project_version(git_repo) == "0.2.0rc1"
    assert re.search(rf"^## \[0\.2\.0rc1\] - {DATED}$", git_repo.read("CHANGELOG.md"), re.M)
    assert git_repo.git("cat-file", "-t", "v0.2.0rc1") == "tag"


def test_a_release_after_its_pre_release_lists_every_change_since_the_previous_release(git_repo):
    released_once(git_repo)
    assert git_repo.run_tool("release.py", "0.2.0rc1").returncode == 0
    git_repo.push()
    git_repo.commit("fix: keep the alpha channel")
    git_repo.push()

    result = git_repo.run_tool("release.py", "0.2.0")

    assert result.returncode == 0, result.stdout + result.stderr
    changelog = git_repo.read("CHANGELOG.md")
    section = changelog.split("## [0.2.0] - ", 1)[1].split("## [0.2.0rc1]", 1)[0]
    for entry in ("- glow: Add a falloff parameter", "- Stop rounding twice", "- Keep the alpha channel"):
        assert entry in section
    assert f"[0.2.0]: {REPOSITORY}/compare/v0.1.0...v0.2.0\n" in changelog


def test_a_section_written_by_hand_is_kept_and_dated_for_the_first_release(git_repo):
    written = "### Added\n\n- Everything, curated by hand.\n"
    make_project(git_repo, changelog=f"{PREAMBLE}## [0.1.0] - Unreleased\n\n{written}\n{FIRST_LINK}")

    result = git_repo.run_tool("release.py", "0.1.0")

    assert result.returncode == 0, result.stdout + result.stderr
    changelog = git_repo.read("CHANGELOG.md")
    expected = rf"{re.escape(PREAMBLE)}## \[0\.1\.0\] - {DATED}\n\n{re.escape(written)}\n{re.escape(FIRST_LINK)}"
    assert re.fullmatch(expected, changelog), changelog
    assert git_repo.git("cat-file", "-t", "v0.1.0") == "tag"


# ---------------------------------------------------------------------------------------------------------------------
# known-bad: one per rule, each refused before anything changes


def refused(git_repo, *args: str) -> str:
    before = snapshot(git_repo)
    result = git_repo.run_tool("release.py", *args)
    assert result.returncode == 1, result.stdout + result.stderr
    assert snapshot(git_repo) == before
    return result.stderr


def test_a_bump_that_is_not_a_part_or_a_version_is_refused(git_repo):
    released_once(git_repo)
    assert "banana" in refused(git_repo, "banana")


def test_a_version_lower_than_the_current_one_is_refused(git_repo):
    released_once(git_repo)
    message = refused(git_repo, "0.0.9")
    assert "0.0.9" in message
    assert "0.1.0" in message


def test_a_version_whose_tag_exists_is_refused(git_repo):
    released_once(git_repo)
    assert "v0.1.0 exists" in refused(git_repo, "0.1.0")


def test_a_release_from_another_branch_is_refused(git_repo):
    released_once(git_repo)
    git_repo.git("switch", "-q", "-c", "feature")
    assert "main" in refused(git_repo, "minor")


def test_a_release_with_uncommitted_changes_is_refused(git_repo):
    released_once(git_repo)
    git_repo.write("CHANGELOG.md", git_repo.read("CHANGELOG.md") + "\nedited\n")
    assert "uncommitted" in refused(git_repo, "minor")


def test_a_release_behind_origin_is_refused(git_repo):
    released_once(git_repo)
    git_repo.git("reset", "-q", "--hard", "HEAD~1")
    assert "origin/main" in refused(git_repo, "minor")


def test_a_release_whose_gate_fails_is_refused(git_repo):
    released_once(git_repo, gate="raise SystemExit(3)")
    assert "poe check" in refused(git_repo, "minor")


def test_a_release_with_nothing_for_the_changelog_is_refused(git_repo):
    make_project(git_repo)
    git_repo.git("tag", "-a", "v0.1.0", "-m", "v0.1.0")
    git_repo.commit("docs: only documentation since the last release")
    git_repo.push()
    assert "empty" in refused(git_repo, "patch")
