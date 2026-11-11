# CLAUDE.md: bullterrier-image

A Rust library (crate `bullterrier-image`) with a Python interface, as polars has, that styles photos with composable effects;
a CLI and a service follow. Python is kept only where Rust lacks an equivalent. **The code API is the product** (as numpy's or polars'
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
| Effects: one picture in, one picture out | Effects that need earlier frames (trails, datamosh) → video; one that ghosts a single picture with itself (`echo`) stays here |
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
`crates/core/src/effects/<name>.rs` (the kernel, which declares and runs the effect),
`examples/effects/<name>.toml`, `docs/gallery/<name>.jpg`. `gallery.effect_files(name)` is the one place that knows these paths.
Shared numeric code lives in `ops/`, named by concept.

```
app        cli (starts the core's command line), pipeline, gallery, sheet, fx, public API   (may import anything below)
adapters   imageio (files), finding (finding the subject), loader (recipe files)   (may import core, ops)
plugin     the effect-author API (core's names), public for other packages (may import core)
core       params (EffectParams, param), values (Color, FieldName, Picture, Lut), registry, steps (the recipe model:
           Step, Each, Recipe), regions and framing (its declarations), finder (Finder), native (the extension's
           entry points), bridge (a recipe run by the Rust core)                                          (may import ops)
ops        pure numeric primitives                                    (may import errors, _native)
errors     BullterrierImageError and its subclasses                            (imports nothing from bullterrier_image)
_native    home of the Rust extension `_native._core`, beside errors          (imports nothing from bullterrier_image)
```
**Architecture rules.** Reference for the Rust core, runner, regions, files, threads and bindings: `docs/architecture.md`; read it before touching them.
- An effect has **no Python folder**: the registry registers every effect the core declares (`registry._register_core_effects`) with a function that calls `_core.apply("<name>", rgb, params.model_dump())`.
- Rust effects never refer to each other.
- The ratio of design px (480 on the longest side) is derived from that working picture, never passed in.
- Every user number that drives allocation or work has a maximum and a row in `crates/core/tests/it/limits.rs`; its completeness test fails for a bound of `Region::check` with neither a row nor a reason.
- Python is a facade over the core and runs no recipe itself.
- The render stays on the calling thread, only row bands run on the pool, and rayon's global pool is never used.
- Outside a pool a kernel runs inline, and `Effect::apply` takes no thread parameter.
- Rule: the output looks the same at any thread count and is identical where that comes naturally (a band reads only input rows and carries nothing from one row to the next; a reduction chunks by the data, never by the thread count).
- Never add real complexity for the last bit: if a kernel's rounding differs between runs with no visible effect, compare within a small tolerance.

Imports flow **downward only** (import-linter: contracts in `pyproject.toml`, run by `tests/test_layering.py` and
`uv run lint-imports`). A forbidden import means the code is in the wrong layer: move it.
The registry loads every entry point of the group `bullterrier_image.effects` (other packages' effects) on first query.

## Rules

@docs/rules.md

Those rules apply here. What follows is image's own, or where it needs a rule differently, with the reason.

## Image's rules

1. **Effects never import other effects.** Shared logic goes to `ops/` (default "One concept, one module": `warp`, `noise`,
   `edges`, `colorspace`...). An `ops` module arrives with its first consumer (`vulture` fails on unused code). `ops` is
   internal.
2. **Presets and looks are data**: a preset is a named parameter set of one effect, `crates/core/data/presets/<effect>.toml`; a look is a multi-step recipe with a `description`, `crates/core/data/looks/<name>.toml`, loaded by name (`load_recipe("crt")`).
3. **Params are declared once**: in Rust for an effect the core runs, else as a Pydantic model; either way the Pydantic
   model drives validation, defaults, generated docs (`help(fx.<effect>)`, `bt-image effects`, `docs/effects/`,
   `fx.pyi`), `fx` signatures and the JSON schema. See "Parameters".
4. **Open/closed:** adding an effect creates files named for it and modifies none (`tests/test_scaffold.py` proves it). Editing
   the registry, `__init__`, README, `fx.pyi` or `docs/effects/` for a new effect means something is wrong.
5. **No hidden tunables.** Every value that shapes a result, including library options, is a named, documented parameter in the
   field's own terms. Exempt: mathematical and standard constants, each cited in a comment.
6. **Determinism:** same inputs and `seed` give the same output on the same machine, library versions and models; across
   platforms equal within float tolerance. `numpy.random.default_rng(seed)`; no global random state.
7. **Layering and the contract are tests**, run over every effect automatically.
8. **Errors:** callers catch `BullterrierImageError` (`bullterrier_image.errors`). Library code raises a subclass
   (`RecipeError`, `UnknownEffectError`, `MapNotFoundError` with `SubjectNotFoundError` and `DepthNotFoundError`,
   `BackendUnavailableError`, `ImageReadError`, `ImageWriteError`, `LutError`, `InvalidImageError` with `NonFiniteImageError`), never a bare `ValueError`, for anything a caller, recipe or photo can get wrong;
   each also inherits the matching builtin. A broken effect definition (caught at import) is a programming error: plain
   `ValueError`/`TypeError`.

## Rust code in image

Rust follows the same rules (effects never refer to each other; shared code in `ops/`, arriving with its first user;
no hidden tunables: a number is a `Params` field, inside a `#[param(..)]` attribute, trivial, or carries `// constant:
<reason>`). Also: `pub` only for what the bindings use, the recipe model, the support types the derives write (`crate::params`) and an
effect's typed parameters (`effects::<name>::Params`, its fields and choice enums, documented); else `pub(crate)`: an
effect's `META` and `render`. Fields are `pub`, so rustc no longer reports one that `render` never reads:
`crates/core/tests/parameters_are_read.rs` does. A new effect needs nothing there, unless a parameter matters only while
another is set (as `grade.pivot` needs `contrast` other than 1): add a row to that test's `conditional` table. Adding a field to a
`Params` breaks code that builds it without `..Default::default()`, accepted until the crate is published.
A Python `ops` function and its Rust twin may coexist
while unported effects use the Python one; delete it with its last Python user. `pub` also covers the runner's types
(`Photo`, `Options`, `Finder`, `Stage`, `Layer`, `Labels`, `Inputs`, `Drive`, `ErrorKind`). A user's values are checked by Rust
code (`Table::check`, `Recipe::checked`, `Presets`), which Python calls when a `Recipe` is made; running an effect trusts
its parameters.

## Image's standards

- **Override of the default "Runtime dependencies: none":** numpy is the interface's picture type; Pillow, scipy, pydantic and
  the finders' runtimes stay until R6 and E7 (reasons in `docs/design-decisions.md`).
- `src/` holds only code and the data it runs on (presets, looks). Tests, examples and gallery pictures live outside it.
- **File size:** a source file stays within 1000 lines, a test file within 1500 (`tools/check_file_size.py`, run by the tests;
  `[tool.file-size]`). Split by concept before you reach it. A file already over is listed in `[tool.file-size.exempt]` at its
  size: it may shrink, never grow; never add an entry.
- Public API, each with `__all__`: `bullterrier_image`, `.recipe`, `.fx`, `.imageio`, `.plugin`. Everything else, including
  `core` and `ops`, is internal.
- A public call takes at most one argument by position, the thing itself (`Recipe(steps)`, `Step(effect)`); effect
  parameters and everything else by keyword. What only the library uses is a function of its module, not an attribute
  of a public class.
- Output files carry no EXIF or GPS data from the input.
- A new CI check goes into `ci-ok`'s `needs` and into `build`'s; image's own jobs are in `.github/workflows/project.yml`.

## The effect contract

An effect is a pure function on a float32 RGB image `(h, w, 3)` in 0..1 (`imageio.py`, over the core's `files`, is the only place that knows file formats):

```python
@effect(category="light", params=BloomParams)  # category is one of CATEGORIES
def bloom(rgb: np.ndarray, params: BloomParams) -> np.ndarray: ...
```
It returns a finite float32 array of the same shape (another dtype or a NaN is a broken effect: the core refuses it with a
`ValueError` naming the effect, and nothing casts it for you), is deterministic, never modifies its input, and works with every parameter at its documented minimum
and maximum. A map it works from (the subject, the depth, a region) is a field parameter (see "Parameters"), never an
input. The injected inputs, declared only if needed: `partition` (piece labels of a region, `(h, w)` int32) and `size_ratio`
(float, working px per design px, default 1.0: declare it only if light or density is per working pixel). Contract tests
check all of this for every effect.
In the core the inputs arrive as `Inputs` (only those declared) and a field as a `Layer` under its parameter's name; a foreign
(Python) effect gets them as keyword arguments of `fn(rgb, params, **inputs)`: `size_ratio`, `partition` (int32), each field a
float32 `(h, w)` array. An effect the core cannot describe (no parameter table, as every Python one today) receives its values
as written, in design pixels, unscaled, and scales them itself by `size_ratio`; the Python registry wrapper does (validate, scale,
call).
The runner applies a step's `mask`, `opacity` and `blend_mode`, never the effect. No parameter may be named like a step key
(`effect`, `mask`, `opacity`, `blend_mode`, `preset`, `partition`, `each`, `looks`, `name`) or an injected input; registering one fails. An effect whose natural
strength is below full declares its steps' default opacity, `@effect(..., opacity=0.7)`, instead of an opacity parameter.

## Parameters

A Pydantic model per effect: a subclass of `EffectParams` (`bullterrier_image.core.params`; frozen, `extra="forbid"`, strict).
Parameter declarations in Rust, the kinds and the Python seam: `docs/parameters.md`; read it before adding or changing a parameter. Binding sentences:
- An effect the core runs **declares its fields in Rust**, on its public `Params` struct: each field a type, a `///` doc comment and `#[param(default = .., unit = px, min = .., max = .., at_most = other, drivable, advanced, cyclic)]`.
- The struct's own doc is the model's docstring, a field's doc its help text in Python: plain text, no rustdoc links, a blank `///` line becomes a double space.
- The Python class is `class XParams(EffectParams, core="x")` with an empty body (`# noqa: D101`); the model is built from the Rust declaration, a default or bound keeping the literal as written (a whole number stays an `int`).
- Python's model validates a user's values when a step is made; `Table::check` in Rust is the same rules, compared with Python's by parity tests (`tests/_native`); running an effect trusts its parameters.
- The copy is not edited: a change is a new version in the ecosystem's format first, then a new copy here.
- A `picture` parameter (`PictureName`) is a name or path, never a colour, and `usize` counts are kind `int`.

Every field has a default and a description (what it does, unit and range in words, what raising it does).
- **Numeric** (`int`, `float`): a `unit` from the format's list and inclusive `min`/`max` that hold the default.
- **Counts and densities:** a few marks read as a `count` (their sizes are `px`, so the same count looks alike at any
  size). Numerous marks take `density`: marks per 1000 design px², the one meaning everywhere (`params.scaled`
  divides it by the size ratio squared).
- **Other** fields: `bool`, `Literal[...]` (the doc mentions every choice), `str`, `Color` (`'#rrggbb'`, `'#rgb'`, or
  three numbers 0..1), lists of those.
- **Basic and advanced:** `advanced=True` lists a field apart, still settable and validated. Aim at six or fewer basic
  fields.
- **Drivable:** `drivable=True` on a float lets a `Drive` vary it per pixel. Declare it only where the output changes
  smoothly with the value (a radius, an amount, an exposure), never for a whole number, choice, seed or colour; every
  drivable field is tested by `tests/effects/test_contract.py` (the facade) and `crates/core/tests/effects_contract.rs`.
- **Field:** a `FieldName` parameter (`field: FieldName = param("depth", doc=...)`) names a selection (`depth`, `light`,
  `subject`, a region), as `mask` and `Drive.by` do; not drivable, not a number. The effect takes the array as a keyword-only
  argument of that name, no default: `def relight(rgb, params, *, field)`; the runner builds only the fields steps name.
- **Cyclic:** `cyclic=True` on a degrees field whose value and the value 360 further give the same picture (a direction,
  a rotation, a hue). Not for elevations, leans, twists or amounts; the contract test checks the picture at `min` equals
  the picture at `max`.
- **State, not time:** a parameter that changes with a process is a state (`progress`, a share in `fraction`, a position in
  `cycles`), never a duration.

Use the field's own terms (radius, sigma, amount, threshold, feather, falloff, gamma, gain, lift, exposure, contrast, saturation,
hue, temperature, tint, angle, spacing, opacity, blend, octaves, persistence, lacunarity, seed), never `strength`, `intensity`,
`factor`, `val`, `k`, or the unit's name. A bad value fails at construction or load, naming effect, field, constraint and value.
`@effect` requires `params=`.

## Adding an effect

Developer tasks (`tools/dev.py` via `poe`), not part of `bt-image`. The template in `src/bullterrier_image/effects/_template/`
defines the shape; `grade` (`crates/core/src/effects/grade.rs`) is the worked example. An effect that
works from a map declares a `FieldName` parameter and reads its `Layer` from `Inputs` (`focus_blur`).
1. **Scaffold:** `uv run poe new-effect <name> --category <category>` writes the Rust kernel `crates/core/src/effects/<name>.rs` (`META` with the
   category, a public `Params` with one example field, `render`) and
   `examples/effects/<name>.toml`; no Python file and no Python test. The effect exists once the extension is rebuilt with the kernel (`uv run` and `uv sync`
   do); then it passes the gate as written (`uv run poe check-scaffold` proves it in a copy; CI runs it).
2. **Fill in:** replace the example field with the effect's own in `Params` (the one declaration), `META` (opacity below 1
   if its steps should be mixed in partway), and the kernel with the effect (the `///` doc of `Params` documents it for users; its first
   sentence is the summary; a parameter that matters only while another is set gets a row in
   `parameters_are_read.rs`); its behaviour's properties as a module of `crates/core/tests/it` (`effects_contract.rs` already covers ranges, refusals, seeds, threads, size); in the example recipe, 1 to 3 steps at
   `size = 480`. `uv run poe gallery --only <name>` renders `docs/gallery/<name>.jpg` and writes nothing else.
3. **Gate:** `uv run poe check`. Only files named for the effect may appear in `git status`. The integrator runs
   `uv run poe regenerate`; do not commit `fx.pyi` or `docs/effects/`.
4. **Python tests:** an effect ships Rust tests; `tests/effects/<name>/` holds only what Python code can break (preset data, `fx` and
   recipe plumbing, `size_ratio`, field plumbing).

## Fixing defects: image's additions

On top of the rules in `docs/rules.md`:
- Outside input is a recipe, a parameter, a photo, a file, a model or the network; each is validated once, where it enters.
- The one clamp that belongs is the effect contract's (output in 0..1), written once.
- A rule for every effect lives once, in `crates/core/tests/effects_contract.rs` (the facade's: `tests/effects/test_contract.py`).
- Two similar Python declarations beat an abstraction with generated fields (a field's declaration is written once, in Rust,
  only for an effect the core runs); a three-line repeat beats a helper that hides the case.

## Gates: image's additions

- `poe fast` also runs the Rust tests (it is not an extension point of the template). `poe full` adds, through `project-full`:
  `check-scaffold` (`poe new-effect` end to end, the `rebuild` marker), `audit` (needs the network) and `check-min-deps`.
- **Full-tier Rust tests:** a matrix over sizes or bands has one row in `check` and every row in a test marked `#[ignore = "full: <why>"]`, run by `poe rust-test-full` (in `project-full` and a CI job). Only that reason may ignore a test (`tests/tools/test_rust_full_tier.py`).
- A gate needs a known-bad row for every kind of source it must see (an effect file pulled in by `include!` is one); image's rows
  are in `tests/tools/test_rust_gates.py`, and the files its own proofs copy or match are `[tool.gate-proofs] project-triggers`.
- Test and build profile: `[profile.test] opt-level = 1`, `[profile.dev] debug = "line-tables-only"`, `split-debuginfo = "off"`
  (macOS otherwise keeps every object file).
- The tests include the import contracts, the effect contract at odd sizes and across every number's range, and the
  hidden-tunable scan of effect packages and `ops`: a number is a `param`, a trivial operand or index, or carries
  `# constant: <reason>`; a library option never gets a marker.
- `mypy` does not check the generated `fx.pyi`, so a misspelt `fx.<effect>` or parameter is caught only at run time.

## Testing: image's additions

On top of `docs/rules.md` ("Testing"):
- **A behaviour is tested once.** Python tests of an effect the core runs keep only the facade (see "Adding an effect", step 4);
  the same property tested in Rust and Python is a defect: delete the Python one.
- Property examples: `pixel_sort` keeps every column's pixel set; `kaleidoscope` is symmetric about its mirror axes; a blur never
  leaves the input's range.
- A registered effect in a Rust test gets a name no other test has (`unique`, a per-module prefix and a counter), and no test
  asserts the whole registry.
- **Mutation-check your tests, lightly** (overrides the default "Mutation-check your tests": about 90 effects make a full pass
  too slow): when a port or change adds logic with weak oracles (boundaries, numeric primitives,
  parameter checks), break the code on purpose, about five targeted mutants per new kernel or primitive, once per finished kernel,
  not per commit. A survivor gets a test, or is named equivalent or marginal and left. Skip thin kernels whose behaviour tests
  already compare hand-worked values. Before the crate is published, one separate deeper pass over `ops/` and the parameter
  checks (`cargo-mutants` or by hand, under `memguard`); not a CI gate. Use `PYTHONDONTWRITEBYTECODE=1`.
- Substitute only true external boundaries (subject-finding model, clock, network, filesystem via `tmp_path`), by injecting
  through a public parameter (`finders={"subject": Finder("stand-in", ...)}`).
- Two internal strategies (sparse and dense path) are tested through the public API on inputs that exercise each, against a
  naive reference in the test, not by forcing a private switch.

When a test reaches inside (`tests/test_suite_rules.py` fails on private imports and on patching `bullterrier_image`):
- Inject through a public parameter (`pipeline.run(finders=...)`, `gallery.write_docs(checkout=...)`) and count the calls of
  your stand-in.
- Promote a private name worth testing deliberately (public name, docstring), as `finding.subject.refine_edge` was; else delete the test.
- Own test effects: `throwaway_effects`, or `fresh_registry` for an empty registry (public interface only).
- "Work was skipped" shows in the files (an untouched sentinel), not in a spy.

Test files: the mirror path is `tests/ops/test_maps.py` for `ops/maps.py`; the shared fixtures are `tests/conftest.py` (`photo`,
`subject`, registry helpers, `echo_model`, `tracked_files`) and `tests/tools/conftest.py` (`project_copy`, `uv_run_env`,
`git_repo`). One file: `uv run pytest tests/ops/test_maps.py -n 0`. The suite is about 14,000 tests and takes minutes (default
"Fast feedback"; `-n 2` for an ad-hoc full run, because `tests/finding` uses about 2.4 GB per worker).

## Skills

Installed globally: `superpowers:writing-plans`, `superpowers:test-driven-development`,
`superpowers:verification-before-completion` (process); `testing-best-practices` (tests); `software-design-philosophy` (deep
modules, no pass-throughs, no information leakage; interface docstrings state promises); `motion-design` (effects depicting
motion: `disperse`).
`superpowers:*` skills are process aids, not authority: they tend to verbose, over-engineered, over-defensive output. Where one
asks for more validation, layers, documents or steps than this file, follow this file. Tests follow this file (no goldens); fixes
follow "Fixing defects" in `docs/rules.md`; design follows the skill (no shallow wrappers; per-effect boilerplate is the price of uniformity, so keep
`apply` bodies substantial).

## Git conventions: image's additions

On top of `docs/rules.md`: commit scopes are an effect name, `core`, `ops`, `cli`, `pipeline`, `docs`, `deps`
(`feat/glow-falloff` for a branch).
