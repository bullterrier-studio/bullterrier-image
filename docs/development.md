# Development

Needs uv, git and rustup (`rust-toolchain.toml` pins the compiler; rustup installs it on the first build).

```bash
uv sync                # builds the Rust extension too; uv rebuilds it when a Rust file changes
uv run poe setup       # once per clone: the git hooks, the commit-message template, the pinned cargo tools
uv run poe check       # before every push or merge: every gate, Python and Rust
uv run poe check-min-deps   # for CI: the tests on Python 3.12 with the lowest dependency releases the floors allow
uv run poe fix         # applies the automatic lint and format fixes
uv run poe new-effect my_effect --category light   # a new effect's files, from the template
uv run bt-image render examples/photos/bullterrier.jpeg examples/effects/my_effect.toml   # look at it, in out/
uv run poe regenerate   # at integration: fx.pyi, docs/effects/, missing pictures
```

Layers and project layout: [below](#project-layout). Why built this way:
[design-decisions.md](design-decisions.md). Commits follow [Conventional Commits](https://www.conventionalcommits.org/),
a `commit-msg` hook checks ([CLAUDE.md](../CLAUDE.md#git-conventions)). Contribute: [CONTRIBUTING.md](../CONTRIBUTING.md). Releases:
[CHANGELOG.md](../CHANGELOG.md). Security problem, report privately: [SECURITY.md](../SECURITY.md).

## Adding an effect

Adding an effect adds files named for it and edits none. `uv run poe new-effect my_effect --category light` writes, working
as written: `src/bullterrier_image/effects/my_effect/effect.py` (+ `__init__.py`; parameter model and pure function
`def my_effect(rgb, params) -> rgb` under `@effect(category="...", params=...)`, docstring = its documentation),
`tests/effects/my_effect/test_my_effect.py`, `examples/effects/my_effect.toml` (example recipe, size 480, makes its picture
`docs/gallery/my_effect.jpg` via `uv run poe gallery --only my_effect`). Effects never import each other: shared code goes
to `src/bullterrier_image/ops/`. Shared tests then check shape, range, determinism, no input mutation, category, docstring,
defaults, example recipe, shared parameter names (`amount`, not `strength`).

## Notes

- Generated shared files (`src/bullterrier_image/fx.pyi`, `docs/effects/`) are not updated on feature branches, so branches
  adding effects never conflict. After merging, `uv run poe regenerate` brings them up to date in one commit. Existing
  pictures kept (`uv run poe gallery --force` renders all again; effect named with `--only` always rendered).
- Every gate (ruff, vulture, deptry, build check, mypy, tests; for Rust rustfmt, clippy, rustdoc, cargo-shear,
  cargo-deny, stubtest, cargo test) is configured in `pyproject.toml` and `Cargo.toml`, listed in
  [CLAUDE.md](../CLAUDE.md#lint-types-and-gates). Push to `main` (or a tag) runs the whole gate via pre-push hook unless it
  changes only documentation (pages and `docs/images/`); other branches checked by CI when the pull request opens.
- Apple Vision test skipped off macOS. Portable subject backend and depth model tested with tiny stand-in models built in the
  tests: suite never downloads a real model; tests of real models run only when already in cache and assert coarse
  properties (a model update may change every pixel).

## Project layout

```
src/bullterrier_image/                  layers, top to bottom; each imports only from layers below
  cli.py         app       the `bt-image` command (no logic of its own); `python -m bullterrier_image` runs it too
  pipeline.py              many photos x many recipes: cache, checks, export, report
  gallery.py               what is generated from effects (catalogue page, pictures, fx.pyi); new-effect, driven by tools/dev.py
  sheet.py                 contact sheets, step sheets, parameter sweeps
  checks.py                automatic quality checks
  fx.py                    effects as validated step builders for Python (+ generated fx.pyi)
  recipe.py                public namespace: core's recipe names, default finders bound (`finding`)
  imageio.py     adapters  file formats, EXIF, colour profiles; defines the one in-memory image type
  loader.py                load_recipe: reads a recipe file (TOML) and pictures it names, through imageio
  finding/                 subject_matte(rgb) -> alpha, depth_map(rgb) -> depth, inverse_depth(rgb) (before per-photo
                           scaling). Which model finds them (IS-Net, Depth Anything V2 on ONNX Runtime; or Apple Vision
                           for subject, when asked) is hidden here
  presets/                 named parameter sets of built-in effects, one TOML per effect (data, not code)
  looks/                   built-in multi-step recipes, one TOML each, loaded by name (`load_recipe("crt")`; data, not code)
  effects/       effects   the effect library, one package per effect (code only), found automatically;
                           _template/ is what `poe new-effect` copies
  plugin.py                effect-author API, also for other packages (EffectParams, param, Color, Picture, effect)
  core/          core      registry.py, params.py (parameter models), presets.py, looks.py, types.py, vocabulary.py, and the
                           recipe engine: steps.py (`Step`, `Recipe`), regions.py, framing.py (`Framing`), runner.py
                           (render, render_all, trace; given the photo and the subject)
  ops/           ops       shared numeric primitives named by concept: one module per concept (`blur`, `noise`, `warp`,
                           `edges`, `colorspace`...; internal; core may import ops; ops imports nothing above)
  errors.py      errors    BullterrierImageError and subclasses: bottom layer, importable from every other
  _native/       errors    home of the Rust extension `_native._core` (crates/python; `_core.pyi` its stub), beside
                           errors: imports nothing
crates/core/     the Rust core: ops/ (numeric primitives by concept) and effects/<name>.rs (one per ported effect)
crates/python/   the PyO3 bindings that build `bullterrier_image._native._core`; generic, names no effect
examples/        demo project (bt-image.toml, photos/, recipes/, output/), one snippet per effect (effects/),
                 older experiments (experiments/), a Python example; see examples/README.md
docs/            effect catalogue (effects/, generated: index + page per category), its pictures (gallery/), roadmap, design decisions
tests/           mirrors src/: tests/effects/<effect>/test_<effect>.py per effect package
```

Each module owns one decision, callers never see it: `imageio` only place that knows file formats, `finding` only place that
knows Vision or portable model, effects know nothing about files, mattes, recipes. Layers are import-linter contracts in
`pyproject.toml`, checked by the test suite (`uv run lint-imports` alone).
