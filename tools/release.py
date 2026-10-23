"""Cut a release on main: bump the version, write its CHANGELOG.md section, commit and tag. It never pushes.

    uv run poe release minor|patch|major|<version> [--dry-run]

`<version>` is an explicit release version, such as 0.2.0rc1 for a pre-release or 0.2.0 to finish one. The script
refuses, before changing anything, unless: the bump gives a version not lower than the current one whose tag does not
exist yet; HEAD is `main`, the tree is clean and `main` is the same commit as `origin/main` (after a fetch); the new
changelog section passes tools/check_release.py, the rules release.yml applies to the pushed tag; and `poe check`
passes. Then `uv version` writes pyproject.toml and uv.lock, the section goes into CHANGELOG.md, `chore(release): vX`
is committed, the annotated tag `vX` is created, and the command that publishes it is printed. `--dry-run` runs every
check and prints the version and the section, and writes nothing.

The section: one already in CHANGELOG.md for the version, written by hand, is kept and its `Unreleased` becomes
today's date; otherwise git-cliff writes it from the Conventional Commits since the last tag (configured in
`[tool.git-cliff]` in pyproject.toml), above the previous release, and its compare link joins the link definitions at
the bottom.

Run from the repository root (poe does that). Exit code: 0 released (or would release), 1 refused (reasons on stderr).
"""

import argparse
import datetime
import re
import subprocess
import sys
import tomllib
from pathlib import Path

from check_release import LINK_DEFINITION, ReleaseError, check_release, parse_version

PARTS = ("major", "minor", "patch")
CHANGELOG = Path("CHANGELOG.md")
PYPROJECT = Path("pyproject.toml")


def _run(*argv: str) -> str:
    """Run a command, return its output; a failure becomes a ReleaseError naming the command and its message."""
    result = subprocess.run(argv, capture_output=True, encoding="utf-8", check=False)  # noqa: S603  argv built here
    if result.returncode != 0:
        raise ReleaseError([f"`{' '.join(argv)}` failed: {(result.stderr or result.stdout).strip()}"])
    return result.stdout.strip()


def _check_repository() -> None:
    """Refuse unless HEAD is main, the tree is clean and main is the same commit as origin/main."""
    problems = []
    branch = _run("git", "rev-parse", "--abbrev-ref", "HEAD")
    if branch != "main":
        problems.append(f"releases are made from main, not from {branch!r}")
    if _run("git", "status", "--porcelain"):
        problems.append("the tree has uncommitted changes: commit or remove them first")
    _run("git", "fetch", "--quiet", "origin", "main")
    if _run("git", "rev-parse", "HEAD") != _run("git", "rev-parse", "origin/main"):
        problems.append("main is not the same commit as origin/main: pull or push first")
    if problems:
        raise ReleaseError(problems)


def _next_version(bump: str, current: str) -> str:
    """The version `bump` gives (uv's arithmetic for a part); refuse a lower version or one already tagged."""
    version = _run("uv", "version", "--bump", bump, "--dry-run", "--short", "--no-sync") if bump in PARTS else bump
    if parse_version(version) < parse_version(current):
        raise ReleaseError([f"{version} is lower than the current version, {current}"])
    if _run("git", "tag", "--list", f"v{version}"):
        raise ReleaseError([f"the tag v{version} exists already"])
    return version


def _previous_release_tag() -> str | None:
    """The highest final-release tag on HEAD's history; pre-releases are skipped, as git-cliff's `ignore_tags` skips
    them, so a release's section and its compare link both start at the previous release."""
    releases = []
    for tag in _run("git", "tag", "--list", "v[0-9]*", "--merged", "HEAD").splitlines():
        try:
            version = parse_version(tag.removeprefix("v"))
        except ReleaseError:
            continue  # a tag that is not a release version
        if not version.prerelease:
            releases.append((version, tag))
    return max(releases)[1] if releases else None


def _changelog_with_release(changelog: str, version: str, repository: str) -> str:
    """CHANGELOG.md's text with `version`'s section in place: the hand-written one dated, or one by git-cliff."""
    heading = f"## [{version}]"
    if any(line.startswith(heading) for line in changelog.splitlines()):
        today = datetime.datetime.now(tz=datetime.UTC).astimezone().date().isoformat()  # the local date
        return re.sub(rf"(?m)^{re.escape(heading)} - Unreleased$", f"{heading} - {today}", changelog)
    previous = _previous_release_tag()
    commits = [f"{previous}..HEAD"] if previous else []
    section = _run("git-cliff", "--config", str(PYPROJECT), "--tag", f"v{version}", "--strip", "all", *commits)
    link = f"[{version}]: {repository}/" + (
        f"compare/{previous}...v{version}" if previous else f"releases/tag/v{version}"
    )
    lines = changelog.splitlines()
    first_section = next((i for i, line in enumerate(lines) if line.startswith("## ")), len(lines))
    first_link = next((i for i, line in enumerate(lines) if LINK_DEFINITION.fullmatch(line)), len(lines))
    lines[first_link:first_link] = [link]
    lines[first_section:first_section] = [*section.splitlines(), ""]
    return "\n".join(lines) + "\n"


def release(bump: str, *, dry_run: bool) -> None:
    """Check everything, then (unless `dry_run`) bump, write the changelog, commit and tag; raise ReleaseError."""
    project = tomllib.loads(PYPROJECT.read_text(encoding="utf-8"))["project"]
    if bump not in PARTS:
        parse_version(bump)  # a mistyped bump is refused before anything slower runs
    _check_repository()
    version = _next_version(bump, project["version"])
    repository = project.get("urls", {}).get("Repository")
    if not repository:
        raise ReleaseError(["pyproject.toml has no [project.urls] Repository for the changelog's compare links"])
    changelog = _changelog_with_release(CHANGELOG.read_text(encoding="utf-8"), version, repository)
    notes = check_release(f"v{version}", version, changelog, tag_on_main=True).notes
    print(f"release: running the gate, `poe check`, before releasing v{version}", flush=True)
    if subprocess.run([sys.executable, "-m", "poethepoet", "check"], check=False).returncode != 0:
        raise ReleaseError(["`poe check` failed: fix the gate before releasing"])
    if dry_run:
        print(f"\nrelease (dry run): would release v{version} with these notes; nothing was changed\n\n{notes}")
        return
    _run("uv", "version", version, "--no-sync")
    CHANGELOG.write_text(changelog, encoding="utf-8")
    _run("git", "add", str(PYPROJECT), "uv.lock", str(CHANGELOG))
    _run("git", "commit", "--quiet", "--message", f"chore(release): v{version}")
    _run("git", "tag", "--annotate", f"v{version}", "--message", f"{project['name']} {version}")
    print(f"\nrelease: v{version} is committed and tagged locally. Publish it with:\n\n")
    print(f"    git push --atomic origin main v{version}\n")


def main(argv: list[str] | None = None) -> int:
    """Release from the repository in the working directory."""
    parser = argparse.ArgumentParser(description="Cut a release on main: bump, changelog, commit, tag (never pushes).")
    parser.add_argument("bump", help="major, minor, patch, or an explicit version such as 0.2.0rc1")
    parser.add_argument("--dry-run", action="store_true", help="run every check and show the release; change nothing")
    args = parser.parse_args(argv)
    try:
        release(args.bump, dry_run=args.dry_run)
    except ReleaseError as error:
        print("release: refused:", file=sys.stderr)
        print("\n".join(f"  - {problem}" for problem in error.problems), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
