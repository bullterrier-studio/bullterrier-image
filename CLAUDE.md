# CLAUDE.md: photo-fx

Python library and CLI that styles photos with composable effects (ASCII, glitch, halftone, colour grades, particle animations). A look is a **recipe**
(TOML or Python): an ordered list of effects. Read `README.md` for usage, `docs/design-decisions.md` for why it is built this way and
`docs/effects-roadmap.md` for what is planned.

## Architecture in one paragraph

A **microkernel**: a small stable `core` (contracts, registry, recipe engine) and many **plugins** (effects) that register themselves and are found by
**auto-discovery**. The engine is a **pipes-and-filters pipeline**: a recipe is an ordered list of steps, each step is a pure filter `image -> image`.
Code is organised as **vertical slices**, one per effect, held together by its name and a `tests/` tree that mirrors `src/`: its code is the package
`src/photo_fx/effects/<name>/` (`effect.py`, `__init__.py`; code only), its tests `tests/effects/<name>/test_<name>.py`, its example recipe
`examples/effects/<name>.toml` and its gallery picture `docs/gallery/<name>.jpg` (artifacts stay out of `src/`); `gallery.effect_files(name)` is the one
place that knows these paths. Shared numeric code lives in `ops/`, named by concept.

```
app        cli, pipeline, gallery, sheet, checks, fx, public API      (may import anything below)
adapters   imageio (files), matte (finding the subject)               (may import core, ops)
effects    one package per effect                                      (may import plugin, core, ops; never other effects)
plugin     the effect-author API (core's names), public for other packages (may import core)
core       params (EffectParams, param, Color), registry, recipe engine (may import ops)
ops        pure numeric primitives                                    (may import errors)
errors     PhotoFxError and its subclasses                            (imports nothing from photo_fx)
```
Imports flow **downward only**. `import-linter` enforces this in the test suite (contracts in `pyproject.toml`, run by `tests/test_layering.py`; `uv run lint-imports`).
If you want an import the rules forbid, the code is in the wrong layer: move it.
Nothing imports `effects`: the registry loads every entry point of the group `photo_fx.effects` (ours is in `pyproject.toml`; third parties add theirs) on its first query, so `import photo_fx` stays fast. In a checkout, `uv sync` installs that entry point; without it no effect is found.
The runner is given the subject (a `matte`, or a `find_subject` function); the public `photo_fx.recipe` binds `matte.subject_matte` as the default.

## The rules

1. **Effects never import other effects.** Shared logic moves to `ops/`, in a module named for its concept (`warp`, `noise`, `edges`, `colorspace`...). Never `utils.py`, `common.py`, `helpers.py`. Extract to `ops/` when a second effect needs it, not before. An `ops` module is added together with its first consumer, never ahead of it (`vulture` fails the gate on code nothing uses; earlier modules wait in git history). `photo_fx.ops` is internal, not API.
2. **Presets are data, not code**: a preset is a recipe fragment (TOML), so it never needs to import effects.
3. **Params are declared once** as a Pydantic model; it drives validation (in `fx`, recipes and direct calls), defaults, the generated documentation (`help(fx.<effect>)`, `photo-fx effects`, `docs/effects.md`, `fx.pyi`), the `fx` signatures and the JSON schema. See "Parameters".
4. **Open/closed:** adding an effect creates new files named for it (its package, its tests, its example recipe, later its picture) and modifies no existing file (`tests/test_scaffold.py` proves it on a copy of the tree). If you find yourself editing the registry, `__init__`, README, `fx.pyi` or `docs/effects.md`, stop: something is wrong.
5. **No hidden tunables.** Every value that shapes a result, including options of libraries an effect calls, is a named, documented parameter in the field's own terminology. Exempt only mathematical and standard constants, each cited in a comment.
6. **Determinism:** same inputs and `seed` give the same output on any machine. Use `numpy.random.default_rng(seed)`; never global random state.
7. **Layering and the contract are tests, not conventions:** they run over every effect automatically.
8. **Errors:** callers catch `PhotoFxError` (`photo_fx.errors`, the bottom layer, exported from `photo_fx`). Library code raises one of its subclasses (`RecipeError`, `UnknownEffectError`, `SubjectNotFoundError`, `BackendUnavailableError`, `ImageReadError`), never a bare `ValueError`, for anything a caller, recipe or photo can get wrong; each subclass also inherits the matching builtin, so `except ValueError` keeps working. A broken effect definition (caught at import) is a programming error and stays a plain `ValueError`/`TypeError`.

## Standards

- `src/` holds only code. Tests, examples and gallery pictures live outside it; `tests/` mirrors `src/`.
- The public API is explicit: `photo_fx`, `photo_fx.recipe`, `photo_fx.fx`, `photo_fx.imageio`, `photo_fx.plugin`, each with `__all__`. Everything else is internal;
  `photo_fx.core` and `photo_fx.ops` are not API.
- No backwards-compatibility shims, aliases or re-exports before 1.0 unless the owner asks: change the callers.
- The library raises `PhotoFxError` subclasses; the CLI catches them and never prints a traceback for a user's mistake.
- The library never prints and never configures logging; only the CLI writes to the terminal.
- Output files carry no EXIF or GPS data from the input.
- Text is UTF-8 (`encoding="utf-8"` on every open); paths are `pathlib.Path`.
- Security gates: ruff's `S` (bandit), `PGH` and `LOG` rules run in `poe check` (a subprocess call carries `# noqa: S603  <why its argv is safe>`);
  `uv run poe audit` runs pip-audit on `uv.lock` (needs the network, so not in `check`: before a release and in CI, which runs `uv run --locked poe check` so a stale `uv.lock` fails).
- Versions are SemVer (a 0.x minor may break the API), written in `pyproject.toml` and changed only by
  `uv run poe release VERSION` (the owner picks VERSION, e.g. `0.1.0rc1` for TestPyPI, `0.1.0` for PyPI). That one command
  checks, shows the plan, asks, then bumps, writes the CHANGELOG.md section with git-cliff from the Conventional Commits,
  commits, tags and pushes main and the tag atomically; docs/releasing.md. Tests never write the project's
  version: they read it from `pyproject.toml` (a test fails otherwise).
- CI is one workflow per concern in `.github/workflows/`, joined by `ci.yml`: the checks (lint, test, min-deps, security,
  pre-commit) run in parallel, then `build` runs only if all passed; `release.yml` calls `ci.yml` and publishes only
  what that build made, so nothing that fails a gate is built or released. `ci-ok` is the only required check: a new
  job goes into its `needs` (a new check also into `build`'s), never into branch protection. Actions are pinned by
  commit SHA and each job holds only the permissions it uses; `uv run poe lint-workflows` (actionlint, zizmor; part of
  `poe check`) enforces it.
- The gates (`uv run poe check`) are the review: reviewers check behaviour and the rules in this file, nothing else. A defect a review finds twice
  becomes a gate.
- Design decisions and their reasons are in `docs/design-decisions.md`; a new one goes there, not in this file.

## The effect contract

An effect is a pure function on a float32 RGB image `(h, w, 3)` in 0..1 (`photo_fx/imageio.py` is the only place that knows about file formats):

```python
@effect(category="light", params=GlowParams)  # category is one of CATEGORIES; params is the effect's Pydantic model
def glow(rgb: np.ndarray, params: GlowParams, *, matte: np.ndarray | None = None) -> np.ndarray: ...
```
It returns an array of the same shape, is deterministic, never modifies its input, and works with every parameter at its documented minimum and maximum.
`matte` (the subject cut-out, `(h, w)` 0..1) is an injected input: declare it only if the effect needs the subject. The contract tests check all of this for every registered effect.

## Parameters

A Pydantic model per effect: a subclass of `EffectParams` (`photo_fx.core.params`; frozen, `extra="forbid"`, every field strict), each field declared with
`param(default, doc=...)`. Every field needs a default and a description (what it does, its unit and range in words, what raising it does).
- **Numeric fields** (`int`, `float`) and **lists of numbers** (`tuple[float, ...]`) also need a `unit` (`px`, `fraction`, `degrees`, `stops`, `seconds`, `count`,
  `ratio`, `cycles`) and an inclusive `min` and `max` that hold the default; for a list they apply to each item.
- **Other fields** take no unit, min or max: `bool` switches, `Literal[...]` choices (the doc must mention every choice), `str`, `Color` (`'#rrggbb'` or `'#rgb'`
  with the leading `#`, or three numbers 0..1), and lists of those. A list may be written as a list or an array.
- **Animation:** any numeric parameter may be animated; an `int` field moves in whole steps (rounded to the nearest), and every rendered frame is checked
  against the range when the recipe loads.

Names are the field's own terms (radius, sigma, amount, threshold, feather, falloff, gamma, gain, lift, exposure, contrast, saturation, hue, temperature, tint,
angle, spacing, opacity, blend, octaves, persistence, lacunarity, seed), never `strength`, `intensity`, `factor`, `val`, `k`, nor the name of the unit. A misspelt
or out-of-range value fails at construction or load, naming effect, field, constraint and value. `@effect` requires `params=`: there is no other way
to declare parameters.

## Adding an effect (the recipe)

The three commands below are developer tasks (`tools/dev.py`, run through `poe`), not part of the installed `photo-fx` program.

What a new effect looks like is defined once, by the template in `src/photo_fx/effects/_template/` (the colour effects, `effects/grade/`,
`effects/clarity/`... are worked examples of it). Three steps:
1. **Scaffold:** `uv run poe new-effect <name> --category <category>` writes `src/photo_fx/effects/<name>/__init__.py` and `effect.py` (the
   `EffectParams` model with one documented example field and the registered function), `tests/effects/<name>/test_<name>.py` and
   `examples/effects/<name>.toml`. It works and passes the gate as written.
2. **Fill in:** in `effect.py` replace the example field with the effect's own (every value that shapes the result is a `param` field, library options
   included) and the function body with the effect, whose docstring's first sentence is its summary; in the test file the properties of the behaviour (the
   contract tests already cover ranges, schema and call styles for every effect); in the example recipe a short recipe showing it (`size = 480`, 1 to 3
   steps). Then `uv run poe gallery --only <name>` renders its picture, `docs/gallery/<name>.jpg`.
3. **Gate:** `uv run poe check`; nothing but files named for the new effect should appear in `git status`. `uv run poe regenerate` is run by the
   integrator, not by you: do not commit `fx.pyi` or `docs/effects.md`.

## Lint, types and the other gates

`uv run poe check` runs every gate, cheapest first, and stops at the first failure: `ruff format --check`; `ruff check` (rules below, plus explicit encodings `PLW1514`, no blind `except` `BLE`, pathlib `PTH`); `vulture` (dead code in `src`; framework-called names are exempt in `[tool.vulture]`); `deptry` (every import declared, development tools not used by the library); `poe build-check` (sdist and wheel checked by twine and check-wheel-contents); `mypy`; the tests, in random order, which include the import contracts, the effect contract (odd sizes, every number across its range) and the hidden-tunable scan of effect packages and `ops` (a number is a `param`, a trivial operand or index, or carries `# constant: <reason>`; never a library option). Each gate has a test proving it fails on a known-bad input. Process rule: **a defect a review finds twice becomes a gate**, so reviews stay light.

`ruff` (rules in `pyproject.toml`) wants type hints and a docstring on every public function, class, method and module, and no `print` outside the CLI. `mypy` type-checks `src` and `tests` (not the generated `fx.pyi`, so a misspelt `fx.<effect>` or parameter name is only caught at run time). Fix a finding properly; for a false positive write a targeted `# noqa: CODE  (reason)` or `# type: ignore[code]  # reason`: a bare or unused one fails the gate. Tests may not reach private names (`SLF001`, `PLC2701`), except architecture tests, with a reason.

## Testing

Tests must follow the testing-best-practices skill, no exceptions. A test that is coupled to implementation, or otherwise poor (weak sole assertion, tautology, golden file, sleep, mock of our own code), is worse than no test: delete it rather than keep it. A smaller suite of behaviour tests beats a larger one with coupling. Reviewers treat such tests as blocking findings.

- Behaviour through the public contract; **property tests, not golden images** (a golden written by the same author as the code proves nothing and churns binary diffs). Examples of properties: `pixel_sort` keeps every column's set of pixels; `mirror` is symmetric; a blur never leaves the input's range.
- Expected values come from an independent oracle (a standard, a hand computation, a brute-force reference), never from the code's own formula.
- Every checker/validator gets a known-good and a known-bad input. Determinism pinned by seeds. No sleeps, no live network, no wall-clock assertions.
- Tests run in a random order (pytest-randomly; the seed is printed, rerun with `-p randomly --randomly-seed=N`, turn off with `-p no:randomly`), so no test may depend on another.
- **Mutation-check your own tests**: break the code on purpose (a sign, a boundary, an ignored argument) and make sure a test fails. Surviving mutants on core behaviour are review findings.
- Refactors prove behaviour is preserved by comparing outputs before and after (byte-identical), recorded in the report, not committed.

**Tests describe behaviour, not implementation.** A test must still pass after any behaviour-preserving refactor: renaming or moving private helpers, replacing the algorithm with another correct one, splitting or merging functions, changing internal data structures.
- Never import underscore-prefixed names, read production constants or private attributes, or call private functions. If something deserves a test it deserves a public name; otherwise test it through the behaviour it serves.
- Never monkeypatch, patch or mock our own modules, functions or classes, and never assert call counts or call order on our own code. Substitute only true external boundaries (the subject-finding model, the clock, the network, the filesystem via `tmp_path`, the `ffmpeg` binary), and prefer injecting through a public parameter (`find_subject=`) over patching.
- No source or AST inspection in behaviour tests. The architecture tests (import-linter, the module-graph checks) are the one exception, because their subject is structure; their docstring says so.
- Assert observable outputs only: returned arrays (as properties), files written, exceptions raised. For errors, assert the exception type and a short substring naming the offending argument or value, never the whole sentence.
- Hard to test through the public interface is a design signal: change the interface (inject a dependency, expose a parameter), do not reach inside.
- Two internal strategies (say a sparse and a dense path) are tested through the public API on inputs that naturally exercise each, against an independent reference written in the test (a naive implementation), not by forcing a private switch.

Rewrite patterns, when a test reaches inside (`tests/test_suite_rules.py` fails on private imports and on patching `photo_fx`):
- To observe or control our own code, inject through a public parameter (`pipeline.run(find_subject=...)`, `gallery.write_docs(checkout=...)`) and count the calls of the stand-in you passed.
- A private name worth testing is promoted deliberately (public name, docstring), as `matte.refine_edge` was; otherwise test the behaviour it serves and delete the direct test.
- Effects of a test's own: `throwaway_effects`, or `fresh_registry` for an empty registry; both use only the registry's public interface.
- "Work was skipped" is seen in the files (a sentinel left untouched), not by spying on a function. Errors: the type plus a short substring naming the argument or value.

How a test file is written (`src/photo_fx/effects/_template/test_effect.py.tmpl` is the model):
- **Parametrise by default:** cases that differ by literals are one test over a table (`pytest.param(..., id=...)`, or a dict of id to case) with known-good and known-bad rows side by side; the id states the rule.
- **A new test file only for a new module**, at the mirror of its path (`tests/ops/test_blur.py` for `ops/blur.py`); anything else goes into the existing file.
- **Shared helpers live in the nearest `conftest.py`**, as fixtures: `tests/conftest.py` (`photo`, `matte`, the registry helpers, `echo_model`, `tracked_files`), `tests/tools/conftest.py` (`project_copy`, `uv_run_env`, `git_repo`). A helper one file uses stays in that file.
- **DAMP, not DRY:** a reader sees each case and its expectation in the test; a helper builds input or names an assertion, never hides what is checked, and a table replaces loops and branches in a test body.
- **No change-detectors:** test a rule with a known-bad input, never restate configuration (a category, a list of names, a pinned limit). Cross-effect rules live only in `tests/effects/test_contract.py`.
- **Slow tests in groups:** a module whose module-scoped fixture spawns tools (ruff, mypy, poe, git) carries `pytestmark = pytest.mark.xdist_group("<module>")`, so with `--dist=loadgroup` one worker builds it once.
- **Running:** one file or folder in one process with `uv run pytest tests/ops/test_blur.py -n 0` (add `-p no:randomly` for a fixed order); the whole suite with `uv run poe test`.

## Skills

Use them; they are installed globally. `superpowers:writing-plans`, `superpowers:test-driven-development`, `superpowers:verification-before-completion` for process; `testing-best-practices`
for tests; `software-design-philosophy` for design (deep modules, no pass-throughs, no information leakage; interface docstrings state promises); `motion-design` for anything animated.
Where they pull against this file: tests follow this file (no goldens); design follows the skill (do not add shallow wrappers; per-effect boilerplate is the price of uniformity, so keep `apply` bodies substantial).

## Working style

- **Read narrowly.** A change should need only the files in your task brief, `CLAUDE.md`, and one reference effect. If you need more, say why in your report.
- **Propose before refactoring** anything outside your brief's file scope; inside the brief, execute (plans are pre-approved). Note needed changes elsewhere under "Needs from others".
- Report honestly: red/green evidence for new behaviour, exact commands run, anything skipped or unverified.
- Scratch space for plans, briefs and reports is `.superpowers/` (git-ignored); keep it out of commits.

## Git conventions

Once per clone: `uv run poe setup` (installs the hooks and the commit template). Before every push or merge: `uv run poe check` (every gate, see "Lint, types and the other gates"; `uv run poe fix` repairs what it can). Commit hooks run ruff only, so commits stay fast; the pre-push hook runs the whole gate only for a push to `main` or a tag (a feature branch is checked by CI when its pull request opens).
- **Trunk-based:** work on a short-lived branch (in a worktree only when the project owner asks for one; the controller says so in the brief) named `<type>/<short-description>` (`feat/glow-falloff`), rebase on `main` before reporting; the controller merges. Never push.
- **Conventional Commits 1.0:** `type(scope)!: subject`. Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`. Scope is optional and lowercase: the effect name, `core`, `ops`, `cli`, `pipeline`, `docs` or `deps`. `!` or a `BREAKING CHANGE:` footer marks a breaking change.
- **Subject:** imperative present tense ("add", not "added"), lowercase start (acronyms and identifiers excepted), no final period, header at most 72 characters. Imperative mood is a convention, not checked by the hook.
- **Body** (after a blank line, wrapped at 100): why the change was made, not what the diff shows. **One logical change per commit**; small commits.
- **Agents** end the message with `Co-Authored-By: <your model name> <noreply@anthropic.com>`.
- The `commit-msg` hook (`tools/check_commit_message.py`, limits in `[tool.commit-message]`) rejects a malformed message and says which rule failed. Merge, revert and fixup commits git writes itself pass unchanged.
- **Never `--no-verify`, never skip, silence or loosen a hook** to get a commit through: fix the cause. If a rule is wrong, change it in `pyproject.toml` in its own commit and say why.
- All Python tooling is configured in pyproject.toml (ruff, mypy, pytest, import-linter, poe, commit-message limits). Do not add ruff.toml, mypy.ini, setup.cfg, pytest.ini, .coveragerc or similar; a test fails if you do. The only other config files are .pre-commit-config.yaml (hook wiring only), .editorconfig and .gitmessage.
