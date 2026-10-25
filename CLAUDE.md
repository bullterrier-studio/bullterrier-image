# CLAUDE.md: bullterrier-image

Python library and CLI that styles photos with composable effects. A look is a **recipe** (TOML or Python): an ordered list of
steps. See `README.md` (usage), `docs/design-decisions.md` (why), `docs/effects-roadmap.md` (planned).

**Write docs tersely.** This file, the README, `docs/`, docstrings, reports and briefs: state the rule or fact, drop preamble,
repetition and justification a reader does not need to act. Reasons go in `docs/design-decisions.md`, once.

## Responsibility

`bullterrier-image` is the **image package** of the Bullterrier ecosystem (GitHub: `bullterrier-studio/bullterrier-image`). It
changes and understands **single pictures** and has **no concept of time**. Siblings: `audio`, `control` (values over time),
`video` (decides what happens when, maps control signals onto parameters, renders frames). Image, audio and control never depend
on each other; only video knows the others.

| Responsible for | Not responsible for |
|---|---|
| Effects: one picture in, one picture out | Effects that need several frames (trails, echo, datamosh) → video |
| Looks (recipes): effects and values as data | Values changing over time (ramps, easing, keyframes, envelopes) → control |
| Describing every parameter: name, unit, range, meaning | Mapping a control signal onto a parameter → video |
| Finding the subject; aiming effects at subject or background | Keeping the subject stable across frames → video |
| Reading and writing still images | Video files, GIF/MP4, frame rates → video |
| Same result for same inputs and seed; same look at any resolution | Sound → audio |
| Rendering one look many times with different values cheaply | Layered layouts, posters, multi-element canvases → video |

**No time:** no seconds, frames, clips, easing or fps in code, parameters or docs. A parameter is a *state* (`progress`, `phase`,
`seed`, `offset`); whoever animates it sets it per frame. Filter-shaped compositing (text as an `svg` region, overlay, blend, sticker) stays
here; anything needing a canvas of its own or several inputs does not.

## Names

`bullterrier-image` (PyPI, repository), `bullterrier_image` (import; docs alias `bti`), `bt-image` (command, `bt-image.toml`,
cache folder), `BT_IMAGE_*` (env vars), `BullterrierImageError` (base exception), `bullterrier_image.effects` (plugin group).
Full name for identity, `bt-` for what a person types. Names inside the package (`Recipe`, `fx`, `RecipeError`, effects,
parameters) carry no prefix.

## Architecture

A **microkernel**: a small `core` (contracts, registry, recipe engine) plus **plugins** (effects) found by auto-discovery. A
recipe is a **pipes-and-filters** list of steps, each a pure filter `image -> image`. One **vertical slice** per effect, joined by
its name; `tests/` mirrors `src/`:
`src/bullterrier_image/effects/<name>/` (`effect.py`, `__init__.py`; code only), `tests/effects/<name>/test_<name>.py`,
`examples/effects/<name>.toml`, `docs/gallery/<name>.jpg`. `gallery.effect_files(name)` is the one place that knows these paths.
Shared numeric code lives in `ops/`, named by concept.

```
app        cli, pipeline, gallery, sheet, checks, fx, public API      (may import anything below)
adapters   imageio (files), matte (finding the subject), loader (recipe files)   (may import core, ops)
effects    one package per effect                                      (may import plugin, core, ops; never other effects)
plugin     the effect-author API (core's names), public for other packages (may import core)
core       params (EffectParams, param, Color), registry, recipe engine (may import ops)
ops        pure numeric primitives                                    (may import errors)
errors     BullterrierImageError and its subclasses                            (imports nothing from bullterrier_image)
```
Imports flow **downward only** (import-linter: contracts in `pyproject.toml`, run by `tests/test_layering.py` and
`uv run lint-imports`). A forbidden import means the code is in the wrong layer: move it.
Nothing imports `effects`: the registry loads every entry point of the group `bullterrier_image.effects` on first query. In a
checkout, `uv sync` installs ours; without it no effect is found.
The runner is given the subject (a `matte` or a `find_subject` function); `bullterrier_image.recipe` binds `matte.subject_matte`.

## The rules

1. **Effects never import other effects.** Shared logic goes to `ops/`, in a module named for its concept (`warp`, `noise`,
   `edges`, `colorspace`...), never `utils`/`common`/`helpers`. Extract when a second effect needs it. An `ops` module arrives
   with its first consumer (`vulture` fails on unused code). `ops` is internal.
2. **Presets are data**: a named parameter set of one effect, `src/bullterrier_image/presets/<effect>.toml`.
3. **Params are declared once** as a Pydantic model; it drives validation, defaults, generated docs (`help(fx.<effect>)`,
   `bt-image effects`, `docs/effects/`, `fx.pyi`), `fx` signatures and the JSON schema. See "Parameters".
4. **Open/closed:** adding an effect creates files named for it and modifies none (`tests/test_scaffold.py` proves it). Editing
   the registry, `__init__`, README, `fx.pyi` or `docs/effects/` for a new effect means something is wrong.
5. **No hidden tunables.** Every value that shapes a result, including library options, is a named, documented parameter in the
   field's own terms. Exempt: mathematical and standard constants, each cited in a comment.
6. **Determinism:** same inputs and `seed` give the same output on the same machine and library versions; across platforms equal within float
   tolerance. `numpy.random.default_rng(seed)`; no global random state.
7. **Layering and the contract are tests**, run over every effect automatically.
8. **Errors:** callers catch `BullterrierImageError` (`bullterrier_image.errors`). Library code raises a subclass
   (`RecipeError`, `UnknownEffectError`, `SubjectNotFoundError`, `BackendUnavailableError`, `ImageReadError`,
   `NonFiniteImageError`), never a bare `ValueError`, for anything a caller, recipe or photo can get wrong; each also inherits
   the matching builtin. A broken effect definition (caught at import) is a programming error: plain `ValueError`/`TypeError`.

## Standards

- `src/` holds only code and the data it runs on (presets). Tests, examples and gallery pictures live outside it.
- Public API, each with `__all__`: `bullterrier_image`, `.recipe`, `.fx`, `.imageio`, `.plugin`. Everything else, including
  `core` and `ops`, is internal.
- No backwards-compatibility shims, aliases or re-exports before 1.0 unless the owner asks: change the callers.
- The CLI catches `BullterrierImageError` and never prints a traceback for a user's mistake.
- The library never prints and never configures logging.
- Output files carry no EXIF or GPS data from the input.
- Text is UTF-8 (`encoding="utf-8"` on every open); paths are `pathlib.Path`.
- Security: ruff `S`, `PGH`, `LOG` run in `poe check` (a subprocess call carries `# noqa: S603  <why its argv is safe>`).
  `uv run poe audit` (pip-audit on `uv.lock`, needs network) runs before a release and in CI, which uses
  `uv run --locked poe check`.
- Versions are SemVer (a 0.x minor may break the API), changed only by `uv run poe release VERSION` (the owner picks it: checks,
  shows the plan, asks, bumps, writes CHANGELOG.md with git-cliff, commits, tags, pushes main and the tag; `docs/releasing.md`).
  Tests read the version from `pyproject.toml`, never write it.
- CI: one workflow per concern in `.github/workflows/`, joined by `ci.yml`; checks run in parallel, then `build`, then
  `release.yml` publishes only what that build made. `ci-ok` is the only required check: a new job goes into its `needs` (a new
  check also into `build`'s). Actions are pinned by commit SHA, jobs hold minimal permissions; `uv run poe lint-workflows`
  (in `poe check`) enforces it.
- The gates are the review: reviewers check behaviour and these rules only. A defect a review finds twice becomes a gate.
- Design decisions and reasons go in `docs/design-decisions.md`, not here.

## The effect contract

An effect is a pure function on a float32 RGB image `(h, w, 3)` in 0..1 (`imageio.py` is the only place that knows file formats):

```python
@effect(category="light", params=GlowParams)  # category is one of CATEGORIES
def glow(rgb: np.ndarray, params: GlowParams, *, matte: np.ndarray | None = None) -> np.ndarray: ...
```
It returns the same shape, is deterministic, never modifies its input, and works with every parameter at its documented minimum
and maximum. `matte` (subject cut-out, `(h, w)` 0..1) is an injected input: declare it only if needed; so is `partition` (piece labels
of a region, `(h, w)` int32). Contract tests check all of this for every effect.
The runner applies a step's `mask`, `opacity` and `blend_mode`, never the effect. No parameter may be named like a step key
(`effect`, `mask`, `opacity`, `blend_mode`, `preset`, `partition`, `each`, `looks`) or an injected input; registering one fails. An effect whose natural
strength is below full declares its steps' default opacity, `@effect(..., opacity=0.7)`, instead of an opacity parameter.

## Parameters

A Pydantic model per effect: a subclass of `EffectParams` (`bullterrier_image.core.params`; frozen, `extra="forbid"`, strict),
each field `param(default, doc=...)`. Every field has a default and a description (what it does, unit and range in words, what
raising it does).
- **Numeric** (`int`, `float`): also a `unit` (`px`, `fraction`, `degrees`, `stops`, `count`, `ratio`, `cycles`) and inclusive
  `min`/`max` that hold the default.
- **Other** fields take no unit/min/max: `bool`, `Literal[...]` (the doc mentions every choice), `str`, `Color` (`'#rrggbb'`,
  `'#rgb'`, or three numbers 0..1), lists of those.
- **Basic and advanced:** `advanced=True` marks a field that tunes the technique rather than chooses the look; listed apart,
  still settable and validated. Aim at six or fewer basic fields.
- **State, not time:** a parameter that changes with a process is a state (`progress`, a share in `fraction`, a position in
  `cycles`), never a duration.

Use the field's own terms (radius, sigma, amount, threshold, feather, falloff, gamma, gain, lift, exposure, contrast, saturation,
hue, temperature, tint, angle, spacing, opacity, blend, octaves, persistence, lacunarity, seed), never `strength`, `intensity`,
`factor`, `val`, `k`, or the unit's name. A bad value fails at construction or load, naming effect, field, constraint and value.
`@effect` requires `params=`.

## Adding an effect

Developer tasks (`tools/dev.py` via `poe`), not part of `bt-image`. The template in `src/bullterrier_image/effects/_template/`
defines the shape; `effects/grade/`, `effects/clarity/` are worked examples.
1. **Scaffold:** `uv run poe new-effect <name> --category <category>` writes the package (`__init__.py`, `effect.py` with one
   example field), `tests/effects/<name>/test_<name>.py` and `examples/effects/<name>.toml`. It passes the gate as written.
2. **Fill in:** replace the example field with the effect's own and the body with the effect (docstring's first sentence is its
   summary); in the test file, the behaviour's properties (contract tests already cover ranges, schema, call styles); in the
   example recipe, 1 to 3 steps at `size = 480`. `uv run poe gallery --only <name>` renders `docs/gallery/<name>.jpg` and writes
   nothing else.
3. **Gate:** `uv run poe check`. Only files named for the effect may appear in `git status`. The integrator runs
   `uv run poe regenerate`; do not commit `fx.pyi` or `docs/effects/`.

## Fixing defects: structurally, not defensively

A fix removes the cause, not the symptom (reasoning in `docs/design-decisions.md`).
1. **Reproduce first, at the public interface** (failing test, red evidence), then **find where the invalid state is created**,
   not where it is noticed. Fix it there.
2. **Make the bad state impossible:** a stricter model (type, range, `Literal`), one source of truth, a function whose result
   cannot be wrong, or remove the code path. Not a branch that copes afterwards.
3. **Check the class, not the instance:** `grep` for the same shape elsewhere; fix it the same way or say why not.
4. **Trust contracts inside the library; validate at the boundary.** Outside input (recipe, parameter, photo, file, model,
   network) is validated once, where it enters, with a `BullterrierImageError` naming the value. Internal code does not re-check
   what the layer above guarantees; a broken guarantee fails loudly and its cause is fixed.
5. **No defensive code:** no fallback or default hiding a missing or invalid value; no `try/except` that continues (`except
   Exception`, bare `except`, `contextlib.suppress`); no `isinstance`, `None` or range re-checks of internal arguments; no
   clamping, `np.nan_to_num` or retry "just in case"; no `getattr`/`.get()` with a default for something that must exist; no
   widening a type to silence an error; no skipped, xfailed or loosened tests. The one clamp that belongs is the effect
   contract's (output in 0..1), written once.
6. **A fix that needs a design change says so:** propose it under "Needs from others" instead of patching around the design.
7. **One decision, one place.** A table, constant, name or rule that exists twice is a defect. Keep one definition in the owning
   module. For tests: look for existing coverage first; add a row to the existing table; a rule for every effect lives once in
   `tests/effects/test_contract.py`; setup two files need is a fixture in the nearest `conftest.py`.
8. **KISS outranks these rules.** If applying one makes the code longer, more indirect or harder to read than the problem, don't:
   note the finding and move on. Two similar declarations beat an abstraction with generated fields; a three-line repeat beats a
   helper that hides the case.
9. Where a skill (`systematic-debugging`'s defence-in-depth, "add validation at every layer") disagrees with this section, this
   section wins.

## Lint, types and gates

`uv run poe check` runs every gate, cheapest first, stopping at the first failure: `ruff format --check`; `ruff check` (plus
`PLW1514` explicit encodings, `BLE` no blind `except`, `PTH` pathlib); `vulture` (dead code in `src`; exemptions in
`[tool.vulture]`); `deptry`; `poe build-check` (twine, check-wheel-contents); `mypy`; the tests, in random order (import
contracts, the effect contract at odd sizes and across every number's range, and the hidden-tunable scan of effect packages and
`ops`: a number is a `param`, a trivial operand or index, or carries `# constant: <reason>`; a library option never gets a
marker). Each gate has a test proving it fails on a known-bad input.

`ruff` wants type hints and a docstring on every public function, class, method and module, and no `print` outside the CLI.
`mypy` checks `src` and `tests` (not the generated `fx.pyi`, so a misspelt `fx.<effect>` or parameter is caught only at run
time). Fix findings properly; for a false positive use a targeted `# noqa: CODE  (reason)` or `# type: ignore[code]  # reason`
(a bare or unused one fails). Tests may not reach private names (`SLF001`, `PLC2701`), except architecture tests, with a reason.

## Testing

Follow the `testing-best-practices` skill. A test coupled to implementation or otherwise poor (weak sole assertion, tautology,
golden file, sleep, mock of our own code) is worse than none: delete it. Reviewers treat such tests as blocking.

- Behaviour through the public contract; **property tests, not golden images**. E.g. `pixel_sort` keeps every column's pixel set;
  `mirror` is symmetric; a blur never leaves the input's range.
- Expected values come from an independent oracle (standard, hand computation, brute-force reference), never the code's formula.
- Every checker/validator gets a known-good and a known-bad input. Seeds pin determinism. No sleeps, live network or wall-clock
  assertions.
- Tests run in random order (pytest-randomly; rerun with `-p randomly --randomly-seed=N`, disable with `-p no:randomly`); none
  may depend on another.
- **Mutation-check your tests:** break the code on purpose (a sign, a boundary, an ignored argument) and see a test fail.
  Surviving mutants on core behaviour are review findings. Use `PYTHONDONTWRITEBYTECODE=1` and clear `__pycache__` between mutants.
- Refactors prove behaviour is preserved by comparing outputs before and after (byte-identical), in the report, not committed.

**Tests describe behaviour, not implementation:** they must survive any behaviour-preserving refactor.
- Never import underscore names, read production constants or private attributes, or call private functions. Worth testing means
  worth a public name; otherwise test it through the behaviour it serves.
- Never monkeypatch or mock our own code; never assert call counts or order on it. Substitute only true external boundaries
  (subject-finding model, clock, network, filesystem via `tmp_path`), preferably by injecting through a public parameter
  (`find_subject=`).
- No source or AST inspection in behaviour tests (architecture tests are the exception; their docstring says so).
- Assert observable outputs only: returned arrays (as properties), files written, exceptions raised. For errors, the type plus a
  short substring naming the argument or value, never the whole sentence.
- Hard to test through the public interface is a design signal: change the interface, don't reach inside.
- Two internal strategies (sparse and dense path) are tested through the public API on inputs that exercise each, against a
  naive reference in the test, not by forcing a private switch.

When a test reaches inside (`tests/test_suite_rules.py` fails on private imports and on patching `bullterrier_image`):
- Inject through a public parameter (`pipeline.run(find_subject=...)`, `gallery.write_docs(checkout=...)`) and count the calls of
  your stand-in.
- Promote a private name worth testing deliberately (public name, docstring), as `matte.refine_edge` was; else delete the test.
- Own test effects: `throwaway_effects`, or `fresh_registry` for an empty registry (public interface only).
- "Work was skipped" shows in the files (an untouched sentinel), not in a spy.

Writing a test file (model: `src/bullterrier_image/effects/_template/test_effect.py.tmpl`):
- **Parametrise by default:** cases differing by literals are one table (`pytest.param(..., id=...)` or a dict) with good and
  bad rows together; the id states the rule.
- **A new test file only for a new module**, at the mirror path (`tests/ops/test_blur.py` for `ops/blur.py`).
- **Shared helpers are fixtures in the nearest `conftest.py`**: `tests/conftest.py` (`photo`, `matte`, registry helpers,
  `echo_model`, `tracked_files`), `tests/tools/conftest.py` (`project_copy`, `uv_run_env`, `git_repo`). A helper one file uses
  stays there.
- **DAMP, not DRY:** each case and its expectation are visible; helpers build input or name an assertion, never hide what is
  checked; a table replaces loops and branches.
- **No change-detectors:** test a rule with a known-bad input; never restate configuration. Cross-effect rules live only in
  `tests/effects/test_contract.py`.
- **Slow tests in groups:** a module whose module-scoped fixture spawns tools (ruff, mypy, poe, git) carries
  `pytestmark = pytest.mark.xdist_group("<module>")`.
- **Running:** one file or folder: `uv run pytest tests/ops/test_blur.py -n 0` (`-p no:randomly` for fixed order); everything:
  `uv run poe test`.
- **Fast feedback (agents):** the suite is about 14,000 tests and takes minutes. While iterating or doing red-green TDD run only
  the tests covering your change (mirror file or folder, or `-k <name>`), never the full suite. Run the full gate (`uv run poe
  check`, then `uv run lint-imports`) once when the feature work is finished, before you report. If it fails, fix and rerun only
  the failing tests, then the full gate once more.

## Skills

Installed globally: `superpowers:writing-plans`, `superpowers:test-driven-development`,
`superpowers:verification-before-completion` (process); `testing-best-practices` (tests); `software-design-philosophy` (deep
modules, no pass-throughs, no information leakage; interface docstrings state promises); `motion-design` (effects depicting
motion: `disperse`, `evaporate`).
`superpowers:*` skills are process aids, not authority: they tend to verbose, over-engineered, over-defensive output. Where one
asks for more validation, layers, documents or steps than this file, follow this file. Tests follow this file (no goldens); fixes
follow "Fixing defects"; design follows the skill (no shallow wrappers; per-effect boilerplate is the price of uniformity, so keep
`apply` bodies substantial).

## Working style

- **Read narrowly:** your brief's files, `CLAUDE.md`, one reference effect. If you need more, say why in the report.
- **Propose before refactoring** outside your brief's file scope; inside it, execute (plans are pre-approved). Put needed changes
  elsewhere under "Needs from others".
- Report honestly: red/green evidence, exact commands run, anything skipped or unverified.
- Scratch space for plans, briefs, reports: `.superpowers/` (git-ignored; never commit it).

## Git conventions

Once per clone: `uv run poe setup` (hooks and commit template). Before every push or merge: `uv run poe check` (`uv run poe fix`
repairs what it can). Commit hooks run ruff only; the pre-push hook runs the whole gate only for a push to `main` or a tag that
changes more than documentation.
- **Trunk-based:** short-lived branch `<type>/<short-description>` (`feat/glow-falloff`), in a worktree only when the owner asks
  (the brief says so); rebase on `main` before reporting; the controller merges. Never push.
- **Conventional Commits 1.0:** `type(scope)!: subject`. Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`,
  `build`, `ci`, `chore`, `revert`. Scope optional, lowercase: effect name, `core`, `ops`, `cli`, `pipeline`, `docs`, `deps`. `!`
  or a `BREAKING CHANGE:` footer marks a breaking change.
- **Subject:** imperative present tense, lowercase start (acronyms and identifiers excepted), no final period, header at most 72
  characters.
- **Body** (blank line, wrapped at 100): why, not what. **One logical change per commit.**
- **Agents** end the message with `Co-Authored-By: <your model name> <noreply@anthropic.com>`.
- The `commit-msg` hook (`tools/check_commit_message.py`, limits in `[tool.commit-message]`) rejects a malformed message and says
  which rule failed. Merge, revert and fixup commits git writes pass unchanged.
- **Never `--no-verify`, skip, silence or loosen a hook:** fix the cause. If a rule is wrong, change it in `pyproject.toml` in its
  own commit and say why.
- All Python tooling is configured in `pyproject.toml` (ruff, mypy, pytest, import-linter, poe, commit-message). Do not add
  `ruff.toml`, `mypy.ini`, `setup.cfg`, `pytest.ini`, `.coveragerc` or similar (a test fails). Other config files:
  `.pre-commit-config.yaml` (hook wiring only), `.editorconfig`, `.gitmessage`.
