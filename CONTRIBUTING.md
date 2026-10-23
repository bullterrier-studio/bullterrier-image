# Contributing to photo-fx

Thank you for helping. This page is the short version; [CLAUDE.md](CLAUDE.md) holds every rule in full (it is written for
AI agents and people alike), and [docs/design-decisions.md](docs/design-decisions.md) says why the project is built the
way it is.

## Set up

You need [uv](https://docs.astral.sh/uv/) and git.

```bash
git clone https://github.com/jcakstins/photo-fx.git
cd photo-fx
uv sync                # the project and its development tools, in .venv
uv run poe setup       # once per clone: the git hooks and the commit-message template
```

## Before you push

```bash
uv run poe check       # every gate: format, lint, dead code, dependencies, build, types, tests
uv run poe fix         # repairs what lint and format can repair by themselves
uv run poe audit       # known vulnerabilities in the locked dependencies (pip-audit; needs the network)
```

`poe check` is the review: a change that passes it and follows the rules in CLAUDE.md is ready. Tests describe
behaviour through the public API, as properties rather than golden images; CLAUDE.md's "Testing" section says what that
means in practice.

## Adding an effect

```bash
uv run poe new-effect my_effect --category light
```

This writes the effect's package (`src/photo_fx/effects/my_effect/`), its tests, and its example recipe from the
template, working as written. Replace the example parameter and the body with your effect, describe its properties in
the tests, then `uv run poe gallery --only my_effect` renders its picture. Adding an effect edits no existing file; if
`git status` shows anything else, something is wrong. Every value that shapes the result is a documented parameter
(see "Parameters" in CLAUDE.md), and effects never import each other: shared code goes to `src/photo_fx/ops/`.

## Commits and branches

- Work on a short-lived branch named `<type>/<short-description>` and rebase it on `main` before asking for a merge.
- Messages follow [Conventional Commits](https://www.conventionalcommits.org/): `type(scope): imperative subject`, at
  most 72 characters, then a body saying why. The `commit-msg` hook checks the format.
- One logical change per commit. Never skip a hook (`--no-verify`): fix what it reports.

## Continuous integration

`.github/workflows/ci.yml` runs on every push to main, pull request and merge-queue entry, and by hand (Actions tab,
*CI*, *Run workflow*). Stage 1 runs every check in parallel: `lint.yml` (format, lint, dead code, dependencies, types,
workflows), `test.yml` (the suite on Linux, macOS and Windows, Python 3.12 to 3.14), `min-deps.yml` (the lowest allowed
dependencies), `security.yml` (pip-audit, CodeQL, and dependency review on pull requests) and `pre-commit.yml` (the
commit hooks over every file). Stage 2, `build.yml`, builds and checks the distributions and installs the wheel on the
three systems, only once every check passed. `ci-ok` waits for all of them and is the one required check. Each of
these workflows can also be started by hand, and `security.yml` runs weekly on its own. A release runs the whole CI
again and publishes only the files its build made, so nothing that fails a check can be built or published.

## Releasing

Versions follow [Semantic Versioning](https://semver.org/). Before 1.0 a minor release may change the public API (0.2.0
may rename a parameter); a patch release never does. The version is written in `pyproject.toml` and changed only by
the release command. Five commands, from a clone with the hooks installed:

```bash
git switch main && git pull --ff-only     # releases are cut from main, the same commit as origin/main
uv run poe audit                          # no known vulnerability in the locked dependencies
uv run poe release minor --dry-run        # every check, the gate included; shows the version and its changelog section
uv run poe release minor                  # bumps pyproject.toml and uv.lock, writes CHANGELOG.md, commits, tags v0.2.0
git push --atomic origin main v0.2.0      # the command it prints: nothing is published before this push
```

`poe release` takes `major`, `minor`, `patch` or an explicit version, and never pushes. It refuses, changing nothing,
on another branch, with uncommitted changes, when main differs from origin/main, when `poe check` fails, or when the
version would go down or is tagged already. The changelog section is written by git-cliff from the Conventional Commits
since the previous release: `feat` under Added, `fix` under Fixed, `perf` and `revert` under Changed, a subject starting
with remove, drop or delete under Removed, the scope `security` under Security, and a breaking change (`!` or a
`BREAKING CHANGE:` footer) under Changed, marked **Breaking**; `docs`, `test`, `style`, `ci`, `build`, `chore` and
`refactor` commits are left out. To write the notes yourself, add a `## [0.2.0] - Unreleased` section before releasing:
it is kept, and dated.

**Pre-releases** go to TestPyPI, not PyPI: `uv run poe release 0.2.0rc1` tags `v0.2.0rc1`, which publishes to
test.pypi.org and creates a GitHub pre-release; try it with
`pip install --index-url https://test.pypi.org/simple/ --extra-index-url https://pypi.org/simple/ photo-fx==0.2.0rc1`.
`uv run poe release 0.2.0` then makes the release (a bump from a pre-release goes past it: `minor` gives 0.3.0).

**What the tag starts** (`.github/workflows/release.yml`): `tools/check_release.py` checks that the tag is the version
in `pyproject.toml`, that CHANGELOG.md has a dated section for it and that the tagged commit is on main; the whole CI
runs (every check, then the build; the publish jobs need it, so any failure stops the release); after a reviewer
approves the deployment, the distributions CI built and checked are uploaded through Trusted
Publishing (no token is stored anywhere) with signed attestations; last, the GitHub Release gets the changelog section
as its notes, the same files and a build-provenance attestation. If anything fails before the upload, nothing is
published: delete the tag (`git push --delete origin v0.2.0 && git tag -d v0.2.0`), fix main, and release again.

**One-time setup, by the owner:**
- pypi.org, *Publishing*: add a pending publisher for the project `photo-fx`: owner `jcakstins`, repository
  `photo-fx`, workflow `release.yml`, environment `pypi`. The same on test.pypi.org with the environment `testpypi`.
  The first upload creates the project.
- GitHub, *Settings > Environments*: `pypi` and `testpypi`, each with the owner as required reviewer and deployments
  limited to tags `v*`.
- GitHub, *Settings > Rules*: a ruleset on `main` requiring the status check `ci-ok` (the one job that waits for all
  the others, so adding a job never changes the ruleset).
- The first release is `uv run poe release 0.1.0`: the version is 0.1.0 already, and its changelog section is written
  by hand.

## Design decisions

A new decision, or a change to an old one, goes in [docs/design-decisions.md](docs/design-decisions.md) with its reason,
in the same change as the code. Planned effects are in [docs/effects-roadmap.md](docs/effects-roadmap.md).

## Security problems

Do not open a public issue; see [SECURITY.md](SECURITY.md).
