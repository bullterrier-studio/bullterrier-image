# Development

Needs uv, git and rustup (`rust-toolchain.toml` pins the compiler; rustup installs it on the first build).

```bash
uv sync                # builds the Rust extension too; uv rebuilds it when a Rust file changes
uv run poe setup       # once per clone: the git hooks, the commit-message template, the pinned cargo tools
uv run poe fast        # the inner loop: format, lint, types, Rust tests
uv run poe check       # before every merge or push to main: every gate, Python and Rust
uv run poe full        # before a release: check + gate proofs + scaffold + audit + lowest dependencies
uv run poe gate-tests  # the proofs that each gate fails on a known-bad change (minutes; CI runs them)
uv run poe check-min-deps   # for CI: the tests on Python 3.12 with the lowest dependency releases the floors allow
uv run poe check-scaffold   # for CI: a scaffolded effect, after `uv sync` in a copy, passes its tests (about 30 s)
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
as written once `uv sync` has rebuilt the extension: `src/bullterrier_image/effects/my_effect/effect.py` (+ `__init__.py`;
parameter model and pure function `def my_effect(rgb, params) -> rgb` under `@effect(category="...", params=...)`,
docstring = its documentation, body `_core.apply("my_effect", rgb, params.model_dump())`),
`crates/core/src/effects/my_effect.rs` (the kernel: `Params` with the model's fields, `render`; the build script finds
it), `tests/effects/my_effect/test_my_effect.py`, `examples/effects/my_effect.toml` (example recipe, size 480, makes
its picture `docs/gallery/my_effect.jpg` via `uv run poe gallery --only my_effect`). Effects never import each other: shared code goes
to `src/bullterrier_image/ops/`. Shared tests then check shape, range, determinism, no input mutation, category, docstring,
defaults, example recipe, shared parameter names (`amount`, not `strength`).

## Notes

- Generated shared files (`src/bullterrier_image/fx.pyi`, `docs/effects/`) are not updated on feature branches, so branches
  adding effects never conflict. After merging, `uv run poe regenerate` brings them up to date in one commit. Existing
  pictures kept (`uv run poe gallery --force` renders all again; effect named with `--only` always rendered).
- Every gate (ruff, vulture, deptry, mypy, tests; `full` adds the build check; for Rust rustfmt, clippy, rustdoc, cargo-shear,
  cargo-deny, stubtest, cargo test) is configured in `pyproject.toml` and `Cargo.toml`, listed in
  [CLAUDE.md](../CLAUDE.md#lint-types-and-gates). `uv run poe fast` is the inner loop (format, lint, types, Rust tests).
- Three tiers. `fast`: the inner loop. `check`: every static gate, the Rust tests and the Python suite, then the gate
  proofs (`gate-tests`: pytest marker `gate`, tests that copy the project, write one fault and see a gate fail; they
  remove their copies and build folders) if the change touches a file matching `[tool.gate-proofs] triggers` in
  `pyproject.toml` (what a proof edits or stands on) or its base with `origin/main` cannot be read (`poe proofs`). `full`:
  `check` with the build of the sdist and the wheel (`build-check`), the proofs always, the scaffold test (`rebuild`), the audit and the lowest-dependency run. The pre-push
  hook runs `check` for a push to `main` and `full` for a tag; none of it for a push that changes only documentation
  or a tree a gate at least as high already passed on (the record carries the tier: `full` covers `check`, never the
  reverse); other branches are checked by CI when the pull request opens. CI runs the proofs once, on one Linux
  cell of `test.yml`.
- Apple Vision test skipped off macOS. Portable subject backend and depth model tested with tiny stand-in models built in the
  tests: suite never downloads a real model; tests of real models run only when already in cache and assert coarse
  properties (a model update may change every pixel).

## Project layout

```
src/bullterrier_image/                  layers, top to bottom; each imports only from layers below
  cli.py         app       the entry point of the `bt-image` command, which is the core's (`crates/cli`); `python -m bullterrier_image` runs it too
  pipeline.py              many photos x many recipes: cache, checks, export, report
  gallery.py               what is generated from effects (catalogue page, pictures, fx.pyi); new-effect, driven by tools/dev.py
  sheet.py                 contact sheets (the core's)
  fx.py                    effects as validated step builders for Python (+ generated fx.pyi)
  recipe.py                public namespace: core's recipe names, default finders bound (`finding`)
  imageio.py     adapters  facade over the core's file reading and writing (formats, EXIF, colour profiles); Pillow only for `to_pil`/`from_pil`
  loader.py                load_recipe: reads a recipe file (TOML) and pictures it names, through imageio
  finding/                 subject_matte(rgb) -> alpha, depth_map(rgb) -> depth, inverse_depth(rgb) (before per-photo
                           scaling): thin names for the core's finders (IS-Net, Depth Anything V2 on ONNX Runtime, or Apple
                           Vision for the subject, `crates/finders`), which also pass the onnxruntime library file
  (presets/ and looks/     moved to crates/core/data/ and compiled into the core: data, not code)
  effects/       effects   the effect library, one package per effect (code only), found automatically;
                           _template/ is what `poe new-effect` copies
  plugin.py                effect-author API, also for other packages (EffectParams, param, Color, Picture, effect)
  core/          core      registry.py, params.py (parameter models), presets.py, looks.py, types.py, vocabulary.py, and the
                           recipe model: steps.py (`Step`, `Recipe`), regions.py and framing.py (their declarations),
                           finder.py (`Finder`), bridge.py (render, render_all, trace in the Rust core; given the photo
                           and its maps)
  ops/           ops       shared numeric primitives named by concept: one module per concept (`blur`, `noise`, `warp`,
                           `edges`, `colorspace`...; internal; core may import ops; ops imports nothing above)
  errors.py      errors    BullterrierImageError and subclasses: bottom layer, importable from every other
  _native/       errors    home of the Rust extension `_native._core` (crates/python; `_core.pyi` its stub), beside
                           errors: imports nothing
crates/core/     the Rust core, crate `bullterrier-image`: ops/ (primitives by concept), effects/<name>.rs (one per effect)
crates/python/   the PyO3 bindings that build `bullterrier_image._native._core`; generic, names no effect
examples/        demo project (bt-image.toml, photos/, recipes/, output/), one snippet per effect (effects/),
                 older experiments (experiments/), a Python example; see examples/README.md
docs/            effect catalogue (effects/, generated: index + page per category), its pictures (gallery/), roadmap, design decisions
tests/           mirrors src/: tests/effects/<effect>/test_<effect>.py per effect package
```

Each module owns one decision, callers never see it: `imageio` only place that knows file formats, `finding` only place that
knows Vision or portable model, effects know nothing about files, mattes, recipes. Layers are import-linter contracts in
`pyproject.toml`, checked by the test suite (`uv run lint-imports` alone).
