# CLAUDE.md: bullterrier-image

Python library and CLI that styles photos with composable effects. **The code API is the product** (as numpy's or polars'
is): a look is a `Recipe`, an ordered list of steps, built and run in Python; a TOML recipe file is one serialisation of
it, never a model of its own. Every capability exists in code first. See `README.md` (usage), `docs/design-decisions.md`
(why), `docs/effects-roadmap.md` (planned).

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
`seed`, `offset`); whoever animates it sets it per frame. Filter-shaped compositing (text as an `svg` region, blend, a logo or
sticker) stays here; anything needing a canvas of its own or several inputs does not.

## Names

`bullterrier-image` (PyPI, crates.io: the core crate, repository), `bullterrier_image` (import, Rust library; docs alias
`bti`), `bt-image` (command, `bt-image.toml`, cache folder), `BT_IMAGE_*` (env vars), `BullterrierImageError` (base
exception), `bullterrier_image.effects` (plugin group). Full name for identity, `bt-` for what a person types. Names
inside the package (`Recipe`, `fx`, `RecipeError`, effects, parameters) carry no prefix.

## Architecture

A **microkernel**: a small `core` (contracts, registry, recipe engine) plus **plugins** (effects) found by auto-discovery. A
recipe is a **pipes-and-filters** list of steps, each a pure filter `image -> image`. One **vertical slice** per effect, joined by
its name; `tests/` mirrors `src/`:
`src/bullterrier_image/effects/<name>/` (`effect.py`, `__init__.py`; code only), `tests/effects/<name>/test_<name>.py`,
`examples/effects/<name>.toml`, `docs/gallery/<name>.jpg`. `gallery.effect_files(name)` is the one place that knows these paths.
Shared numeric code lives in `ops/`, named by concept.

```
app        cli, pipeline, gallery, sheet, checks, fx, public API      (may import anything below)
adapters   imageio (files), finding (finding the subject), loader (recipe files)   (may import core, ops)
effects    one package per effect                                      (may import plugin, core, ops; never other effects)
plugin     the effect-author API (core's names), public for other packages (may import core)
core       params (EffectParams, param, Color), registry, recipe engine (may import ops)
ops        pure numeric primitives                                    (may import errors, _native)
errors     BullterrierImageError and its subclasses                            (imports nothing from bullterrier_image)
_native    home of the Rust extension `_native._core`, beside errors          (imports nothing from bullterrier_image)
```
**Rust core:** `crates/core` (crate `bullterrier-image`, pure Rust: `ops/` by concept, one `effects/<name>.rs` per
ported effect, found by `build.rs`) and `crates/python` (PyO3 bindings building `bullterrier_image._native._core`; generic, names no effect). A
ported effect keeps its Python model, docstring and `@effect`; its body is `_core.apply("<name>", rgb,
params.model_dump())` (`from bullterrier_image._native import _core`). Its Rust file has `Params`
(`#[serde(deny_unknown_fields)]`, no defaults) and `fn render(picture: &Picture, params: &Params) -> Image`. Rust effects
never refer to each other.
Imports flow **downward only** (import-linter: contracts in `pyproject.toml`, run by `tests/test_layering.py` and
`uv run lint-imports`). A forbidden import means the code is in the wrong layer: move it.
Nothing imports `effects`: the registry loads every entry point of the group `bullterrier_image.effects` on first query. In a
checkout, `uv sync` installs ours; without it no effect is found.
The runner is given each map by name, as an array (`maps={"subject": alpha}`) or a `Finder(identity, find)`
(`finders={"subject": ...}`). `subject`, `depth` and `light` (made from the picture) are built in; any other map is a recipe's `given` region.
`bullterrier_image.recipe` binds the default `subject` and `depth` finders when a recipe needs them.

## The rules

1. **Effects never import other effects.** Shared logic goes to `ops/`, in a module named for its concept (`warp`, `noise`,
   `edges`, `colorspace`...), never `utils`/`common`/`helpers`. Extract when a second effect needs it. An `ops` module arrives
   with its first consumer (`vulture` fails on unused code). `ops` is internal.
2. **Presets and looks are data**: a preset is a named parameter set of one effect, `src/bullterrier_image/presets/<effect>.toml`; a look is a multi-step recipe with a `description`, `src/bullterrier_image/looks/<name>.toml`, loaded by name (`load_recipe("crt")`).
3. **Params are declared once** as a Pydantic model; it drives validation, defaults, generated docs (`help(fx.<effect>)`,
   `bt-image effects`, `docs/effects/`, `fx.pyi`), `fx` signatures and the JSON schema. See "Parameters".
4. **Open/closed:** adding an effect creates files named for it and modifies none (`tests/test_scaffold.py` proves it). Editing
   the registry, `__init__`, README, `fx.pyi` or `docs/effects/` for a new effect means something is wrong.
5. **No hidden tunables.** Every value that shapes a result, including library options, is a named, documented parameter in the
   field's own terms. Exempt: mathematical and standard constants, each cited in a comment.
6. **Determinism:** same inputs and `seed` give the same output on the same machine, library versions and models; across
   platforms equal within float tolerance. `numpy.random.default_rng(seed)`; no global random state.
7. **Layering and the contract are tests**, run over every effect automatically.
8. **Errors:** callers catch `BullterrierImageError` (`bullterrier_image.errors`). Library code raises a subclass
   (`RecipeError`, `UnknownEffectError`, `MapNotFoundError` with `SubjectNotFoundError` and `DepthNotFoundError`,
   `BackendUnavailableError`, `ImageReadError`, `LutError`, `InvalidImageError` with `NonFiniteImageError`), never a bare `ValueError`, for anything a caller, recipe or photo can get wrong;
   each also inherits the matching builtin. A broken effect definition (caught at import) is a programming error: plain
   `ValueError`/`TypeError`.

## Rust code

Rust follows the same rules (effects never refer to each other; shared code in `ops/`, arriving with its first user;
no hidden tunables: a number is a `Params` field, trivial, or carries `// constant: <reason>`). Also: no `unwrap`,
`expect`, `panic!` or printing (clippy fails them): return an `Error`. Core code is safe Rust. `pub` only for what the
bindings use, else `pub(crate)`. A Python `ops` function and its Rust twin may coexist while unported effects use the
Python one; delete it with its last Python user.

## Standards

- `src/` holds only code and the data it runs on (presets, looks). Tests, examples and gallery pictures live outside it.
- Public API, each with `__all__`: `bullterrier_image`, `.recipe`, `.fx`, `.imageio`, `.plugin`. Everything else, including
  `core` and `ops`, is internal.
- No backwards-compatibility shims, aliases or re-exports before 1.0 unless the owner asks: change the callers.
- A public call takes at most one argument by position, the thing itself (`Recipe(steps)`, `Step(effect)`); effect
  parameters and everything else by keyword. What only the library uses is a function of its module, not an attribute
  of a public class.
- The CLI catches `BullterrierImageError` and never prints a traceback for a user's mistake.
- The library never prints and never configures logging.
- Output files carry no EXIF or GPS data from the input.
- Text is UTF-8 (`encoding="utf-8"` on every open); paths are `pathlib.Path`.
- Security: ruff `S`, `PGH`, `LOG` run in `poe check` (a subprocess call carries `# noqa: S603  <why its argv is safe>`).
  `uv run poe audit` (pip-audit on `uv.lock`, cargo-deny advisories on `Cargo.lock`; needs network) runs before a
  release and in CI, which uses `uv run --locked poe check`.
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
@effect(category="light", params=BloomParams)  # category is one of CATEGORIES
def bloom(rgb: np.ndarray, params: BloomParams) -> np.ndarray: ...
```
It returns the same shape, is deterministic, never modifies its input, and works with every parameter at its documented minimum
and maximum. A map it works from (the subject, the depth, a region) is a field parameter (see "Parameters"), never an
input. The injected inputs, declared only if needed: `partition` (piece labels of a region, `(h, w)` int32) and `size_ratio`
(float, working px per design px, default 1.0: declare it only if light or density is per working pixel). Contract tests
check all of this for every effect.
The runner applies a step's `mask`, `opacity` and `blend_mode`, never the effect. No parameter may be named like a step key
(`effect`, `mask`, `opacity`, `blend_mode`, `preset`, `partition`, `each`, `looks`, `name`) or an injected input; registering one fails. An effect whose natural
strength is below full declares its steps' default opacity, `@effect(..., opacity=0.7)`, instead of an opacity parameter.

## Parameters

A Pydantic model per effect: a subclass of `EffectParams` (`bullterrier_image.core.params`; frozen, `extra="forbid"`, strict),
each field `param(default, doc=...)`. Every field has a default and a description (what it does, unit and range in words, what
raising it does).
- **Numeric** (`int`, `float`): also a `unit` (`px`, `fraction`, `degrees`, `stops`, `count`, `ratio`, `cycles`, `density`) and inclusive
  `min`/`max` that hold the default.
- **Counts and densities:** a few marks read as a `count` (their sizes are `px`, so the same count looks alike at any
  size). Numerous marks take `density`: marks per 1000 design px², the one meaning everywhere (`params.scaled`
  divides it by the size ratio squared).
- **Other** fields take no unit/min/max: `bool`, `Literal[...]` (the doc mentions every choice), `str`, `Color` (`'#rrggbb'`,
  `'#rgb'`, or three numbers 0..1), lists of those.
- **Basic and advanced:** `advanced=True` marks a field that tunes the technique rather than chooses the look; listed apart,
  still settable and validated. Aim at six or fewer basic fields.
- **Drivable:** `drivable=True` on a float lets a `Drive` vary it per pixel. Declare it only where the output changes
  smoothly with the value (a radius, an amount, an exposure), never for a whole number, choice, seed or colour; every
  drivable field is tested by `tests/effects/test_contract.py`.
- **Field:** a `FieldName` parameter (`field: FieldName = param("depth", doc=...)`) names a selection (`depth`, `light`,
  `subject`, a region), as `mask` and `Drive.by` do; not drivable, not a number. The effect takes the array as a keyword-only
  argument of that name, no default: `def relight(rgb, params, *, field)`; the runner builds only the fields steps name.
- **Cyclic:** `cyclic=True` on a degrees field whose value and the value 360 further give the same picture (a direction,
  a rotation, a hue): `min` to `max` is one turn and any real wraps into it. Not for elevations, leans, twists or amounts;
  the contract test checks the picture at `min` equals the picture at `max`.
- **State, not time:** a parameter that changes with a process is a state (`progress`, a share in `fraction`, a position in
  `cycles`), never a duration.

Use the field's own terms (radius, sigma, amount, threshold, feather, falloff, gamma, gain, lift, exposure, contrast, saturation,
hue, temperature, tint, angle, spacing, opacity, blend, octaves, persistence, lacunarity, seed), never `strength`, `intensity`,
`factor`, `val`, `k`, or the unit's name. A bad value fails at construction or load, naming effect, field, constraint and value.
`@effect` requires `params=`.

## Adding an effect

Developer tasks (`tools/dev.py` via `poe`), not part of `bt-image`. The template in `src/bullterrier_image/effects/_template/`
defines the shape; `grade` (`effects/grade/`, `crates/core/src/effects/grade.rs`) is the worked example. An effect that
works from a map keeps a Python body until the core takes maps (`effects/relight/`).
1. **Scaffold:** `uv run poe new-effect <name> --category <category>` writes the package (`__init__.py`, `effect.py` with one
   example field, its body calling the core), the Rust kernel `crates/core/src/effects/<name>.rs` (`Params`, `render`),
   `tests/effects/<name>/test_<name>.py` and `examples/effects/<name>.toml`. After `uv sync` (it rebuilds the extension
   with the kernel) it passes the gate as written (`uv run poe check-scaffold` proves it in a copy; CI runs it).
2. **Fill in:** replace the example field with the effect's own, in the model and in `Params`, and the kernel with the
   effect (the Python docstring documents it; its first sentence is the summary); in the test file, the behaviour's
   properties (contract tests already cover ranges, schema, call styles); in the example recipe, 1 to 3 steps at
   `size = 480`. `uv run poe gallery --only <name>` renders `docs/gallery/<name>.jpg` and writes nothing else.
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

`uv run poe check` runs every gate, cheapest first, stopping at the first failure: `ruff format --check` and `rust-fmt`;
`ruff check` (plus `PLW1514` explicit encodings, `BLE` no blind `except`, `PTH` pathlib, `PYI` stubs) and `rust-lint`
(clippy pedantic, `unwrap`/`expect`/`panic`/print banned, dead code, missing docs, unreachable `pub`; `[workspace.lints]`
in `Cargo.toml`); `dead-code` (vulture on `src`; exemptions in `[tool.vulture]`, and a whitelist, written as it runs,
of the fields the Rust core reads); `rust-doc`; `deptry`; `rust-tools` (cargo-shear and cargo-deny at the versions
`Cargo.toml` pins; `poe setup` installs them), `rust-shear` (unused crates), `rust-deny` (bans, licences, sources;
`deny.toml`); `poe build-check` (maturin sdist and wheel, twine, check-wheel-contents); `mypy` and `stubtest`
(`_native/_core.pyi` against the extension); `rust-test`; the tests, in random order (import
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
  `kaleidoscope` is symmetric about its mirror axes; a blur never leaves the input's range.
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
  (`finders={"subject": Finder("stand-in", ...)}`).
- No source or AST inspection in behaviour tests (architecture tests are the exception; their docstring says so).
- Assert observable outputs only: returned arrays (as properties), files written, exceptions raised. For errors, the type plus a
  short substring naming the argument or value, never the whole sentence.
- Hard to test through the public interface is a design signal: change the interface, don't reach inside.
- Two internal strategies (sparse and dense path) are tested through the public API on inputs that exercise each, against a
  naive reference in the test, not by forcing a private switch.

When a test reaches inside (`tests/test_suite_rules.py` fails on private imports and on patching `bullterrier_image`):
- Inject through a public parameter (`pipeline.run(finders=...)`, `gallery.write_docs(checkout=...)`) and count the calls of
  your stand-in.
- Promote a private name worth testing deliberately (public name, docstring), as `finding.subject.refine_edge` was; else delete the test.
- Own test effects: `throwaway_effects`, or `fresh_registry` for an empty registry (public interface only).
- "Work was skipped" shows in the files (an untouched sentinel), not in a spy.

Writing a test file (model: `src/bullterrier_image/effects/_template/test_effect.py.tmpl`):
- **Parametrise by default:** cases differing by literals are one table (`pytest.param(..., id=...)` or a dict) with good and
  bad rows together; the id states the rule.
- **A new test file only for a new module**, at the mirror path (`tests/ops/test_blur.py` for `ops/blur.py`).
- **Shared helpers are fixtures in the nearest `conftest.py`**: `tests/conftest.py` (`photo`, `subject`, registry helpers,
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
motion: `disperse`).
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
- All Python tooling is configured in `pyproject.toml` (ruff, mypy, pytest, import-linter, poe, commit-message, maturin),
  all Rust tooling in `Cargo.toml` (lints, pinned cargo tools). Do not add `ruff.toml`, `mypy.ini`, `setup.cfg`,
  `pytest.ini`, `.coveragerc`, `clippy.toml`, `.cargo/config.toml` or similar (a test fails). Other config files:
  `.pre-commit-config.yaml` (hook wiring only), `.editorconfig`, `.gitmessage`, and, for Rust tools that cannot read
  `Cargo.toml`, `rust-toolchain.toml`, `rustfmt.toml`, `deny.toml` (root only).
