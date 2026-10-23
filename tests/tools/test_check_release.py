"""tools/check_release.py decides whether a pushed tag may be published: the tag names the project's version, the
changelog has a dated, non-empty section for it, and the tagged commit is on main."""

import importlib.util
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]


def _load_checker():
    spec = importlib.util.spec_from_file_location("check_release", ROOT / "tools" / "check_release.py")
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


checker = _load_checker()

CHANGELOG = """# Changelog

Preamble text.

## [0.2.0rc1] - 2026-11-02

### Fixed

- Stop rounding twice

## [0.2.0] - 2026-11-03

### Added

- glow: Add a falloff parameter

### Fixed

- Stop rounding twice

## [0.1.0] - 2026-10-23

The first release.

[0.2.0]: https://github.com/example/photo-fx/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/example/photo-fx/releases/tag/v0.1.0
"""


def check(tag="v0.2.0", version="0.2.0", changelog=CHANGELOG, *, tag_on_main=True):
    return checker.check_release(tag, version, changelog, tag_on_main=tag_on_main)


def problems_of(**kwargs) -> str:
    with pytest.raises(checker.ReleaseError) as caught:
        check(**kwargs)
    return str(caught.value)


# ---------------------------------------------------------------------------------------------------------------------
# known-good


def test_a_stable_tag_is_ready_and_its_notes_are_its_changelog_section():
    release = check()
    assert release.version == "0.2.0"
    assert release.prerelease is False
    assert release.notes == "### Added\n\n- glow: Add a falloff parameter\n\n### Fixed\n\n- Stop rounding twice"


@pytest.mark.parametrize("version", ["0.2.0rc1", "1.0.0a3", "1.0.0b12"])
def test_a_pre_release_tag_is_ready_and_flagged_as_a_pre_release(version):
    changelog = f"## [{version}] - 2026-11-02\n\n- Something\n"
    release = check(f"v{version}", version, changelog)
    assert release.prerelease is True
    assert release.notes == "- Something"


def test_the_oldest_section_ends_before_the_link_definitions():
    assert check("v0.1.0", "0.1.0").notes == "The first release."


# ---------------------------------------------------------------------------------------------------------------------
# known-bad: one per rule


@pytest.mark.parametrize("tag", ["0.2.0", "v0.2", "v0.2.0.dev1", "v0.2.0-rc1", "release-0.2.0", "v01.2.0"])
def test_a_tag_that_is_not_v_and_a_release_version_is_refused(tag):
    assert tag in problems_of(tag=tag)


def test_a_tag_that_differs_from_the_project_version_is_refused():
    message = problems_of(tag="v0.2.0", version="0.2.1")
    assert "v0.2.0" in message
    assert "0.2.1" in message


def test_a_version_without_a_changelog_section_is_refused():
    message = problems_of(tag="v0.3.0", version="0.3.0")
    assert "## [0.3.0]" in message


def test_a_changelog_section_without_a_release_date_is_refused():
    message = problems_of(changelog="## [0.2.0] - Unreleased\n\n- Something\n")
    assert "date" in message


def test_an_empty_changelog_section_is_refused():
    message = problems_of(changelog="## [0.2.0] - 2026-11-03\n\n## [0.1.0] - 2026-10-23\n\n- Old\n")
    assert "empty" in message


def test_a_tag_whose_commit_is_not_on_main_is_refused():
    assert "main" in problems_of(tag_on_main=False)


def test_every_problem_is_reported_not_just_the_first():
    message = problems_of(tag="v0.3.0", version="0.3.1", tag_on_main=False)
    assert "v0.3.0 does not match" in message
    assert "## [0.3.1]" in message
    assert "main" in message


# ---------------------------------------------------------------------------------------------------------------------
# versions


def test_versions_order_numerically_with_pre_releases_before_their_release():
    ordered = ["0.2.0a1", "0.2.0b1", "0.2.0rc2", "0.2.0rc10", "0.2.0", "0.2.1", "0.10.0", "1.0.0"]
    shuffled = ["0.10.0", "0.2.0rc10", "1.0.0", "0.2.0", "0.2.0a1", "0.2.1", "0.2.0rc2", "0.2.0b1"]
    assert sorted(shuffled, key=checker.parse_version) == ordered


@pytest.mark.parametrize("text", ["banana", "1.2", "1.2.3.4", "1.2.3-rc1", "1.2.3rc", "1.2.3.post1", ""])
def test_a_string_that_is_not_a_release_version_is_refused(text):
    with pytest.raises(checker.ReleaseError, match="version"):
        checker.parse_version(text)


# ---------------------------------------------------------------------------------------------------------------------
# the command line, as release.yml runs it, in a real repository


def _project(git_repo, version: str) -> None:
    git_repo.write("pyproject.toml", f'[project]\nname = "demo"\nversion = "{version}"\n')
    git_repo.write("CHANGELOG.md", f"# Changelog\n\n## [{version}] - 2026-11-03\n\n### Added\n\n- A thing\n")


def test_the_command_line_accepts_a_tag_on_main_and_writes_the_notes_and_outputs(git_repo, tmp_path):
    _project(git_repo, "0.2.0rc1")
    git_repo.commit("feat: a thing")
    git_repo.push()
    git_repo.git("tag", "-a", "v0.2.0rc1", "-m", "v0.2.0rc1")
    notes, outputs = tmp_path / "notes.md", tmp_path / "outputs"
    outputs.write_text("earlier=kept\n", encoding="utf-8")  # GITHUB_OUTPUT is appended to, never replaced

    result = git_repo.run_tool("check_release.py", "v0.2.0rc1", "--notes", str(notes), "--github-output", str(outputs))

    assert result.returncode == 0, result.stderr
    assert notes.read_text(encoding="utf-8") == "### Added\n\n- A thing\n"
    assert outputs.read_text(encoding="utf-8").splitlines() == ["earlier=kept", "version=0.2.0rc1", "prerelease=true"]


def test_the_command_line_refuses_a_tag_on_a_branch_that_is_not_on_main(git_repo):
    _project(git_repo, "0.1.0")
    git_repo.commit("feat: first")
    git_repo.push()
    git_repo.git("switch", "-q", "-c", "side")
    _project(git_repo, "0.2.0")
    git_repo.commit("feat: only on a side branch")
    git_repo.git("tag", "-a", "v0.2.0", "-m", "v0.2.0")

    result = git_repo.run_tool("check_release.py", "v0.2.0")

    assert result.returncode == 1
    assert "main" in result.stderr
