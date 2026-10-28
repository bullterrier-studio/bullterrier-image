# Contributing to bullterrier-image

Thank you for helping. This page is the short version; [CLAUDE.md](CLAUDE.md) holds every rule in full (it is written for
AI agents and people alike), and [docs/design-decisions.md](docs/design-decisions.md) says why the project is built the
way it is.

## Set up

You need [uv](https://docs.astral.sh/uv/), git and [rustup](https://rustup.rs/) (the effects are written in Rust;
`rust-toolchain.toml` makes rustup install the right compiler on the first build).

```bash
git clone https://github.com/bullterrier-studio/bullterrier-image.git
cd bullterrier-image
uv sync                # the project (its Rust extension built) and its development tools, in .venv
uv run poe setup       # once per clone: the git hooks, the commit-message template, cargo-shear and cargo-deny
```

`uv sync` and `uv run` rebuild the extension when a Rust file changes.

## Before you push

```bash
uv run poe check       # every gate, Python and Rust: format, lint, dead code, dependencies, build, types, tests
uv run poe fix         # repairs what lint and format can repair by themselves
uv run poe audit       # known vulnerabilities in the locked dependencies (pip-audit, cargo-deny; needs the network)
uv run pytest tests/ops -n 0  # one folder or file, in one process; `uv run poe test` runs the suite on every core
```

`poe check` is the review: a change that passes it and follows the rules in CLAUDE.md is ready. Tests describe
behaviour through the public API, as properties rather than golden images; CLAUDE.md's "Testing" section says what that
means in practice.

## Adding an effect

```bash
uv run poe new-effect my_effect --category light
```

This writes the effect's package (`src/bullterrier_image/effects/my_effect/`), its tests, and its example recipe from the
template, working as written. Replace the example parameter and the body with your effect, describe its properties in
the tests, then `uv run poe gallery --only my_effect` renders its picture. Adding an effect edits no existing file; if
`git status` shows anything else, something is wrong. Every value that shapes the result is a documented parameter
(see "Parameters" in CLAUDE.md), and effects never import each other: shared code goes to `src/bullterrier_image/ops/`.

## Commits and branches

- Work on a short-lived branch named `<type>/<short-description>` and rebase it on `main` before asking for a merge.
- Messages follow [Conventional Commits](https://www.conventionalcommits.org/): `type(scope): imperative subject`, at
  most 72 characters, then a body saying why. The `commit-msg` hook checks the format.
- One logical change per commit. Never skip a hook (`--no-verify`): fix what it reports.

## Continuous integration and releasing

Every push to `main` and every pull request runs the whole gate on GitHub; a release is one command, `uv run poe release
VERSION`, and it publishes only if everything passes. How it works, how to approve and recover, and the one-time setup
are in [docs/releasing.md](docs/releasing.md).

## Design decisions

A new decision, or a change to an old one, goes in [docs/design-decisions.md](docs/design-decisions.md) with its reason,
in the same change as the code. Planned effects are in [docs/effects-roadmap.md](docs/effects-roadmap.md).

## Security problems

Do not open a public issue; see [SECURITY.md](SECURITY.md).
