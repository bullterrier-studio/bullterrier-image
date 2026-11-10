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
`crates/core/src/effects/<name>.rs` (an effect the core runs; a Python one is `src/bullterrier_image/effects/<name>/`, code only),
`examples/effects/<name>.toml`, `docs/gallery/<name>.jpg`. `gallery.effect_files(name)` is the one place that knows these paths.
Shared numeric code lives in `ops/`, named by concept.

```
app        cli, pipeline, gallery, sheet, checks, fx, public API      (may import anything below)
adapters   imageio (files), finding (finding the subject), loader (recipe files)   (may import core, ops)
effects    one package per effect                                      (may import plugin, core, ops; never other effects)
plugin     the effect-author API (core's names), public for other packages (may import core)
core       params (EffectParams, param), values (Color, FieldName, Picture, Lut), registry, steps (the recipe model:
           Step, Each, Recipe), regions and framing (its declarations), finder (Finder), native (the extension's
           entry points), bridge (a recipe run by the Rust core)                                          (may import ops)
ops        pure numeric primitives                                    (may import errors, _native)
errors     BullterrierImageError and its subclasses                            (imports nothing from bullterrier_image)
_native    home of the Rust extension `_native._core`, beside errors          (imports nothing from bullterrier_image)
```
**Rust core:** `crates/core` (crate `bullterrier-image`, pure Rust: `ops/` by concept, one `effects/<name>.rs` per
ported effect, found by `build.rs`), `crates/derive` (the `Params` and `Choice` derives) and `crates/python` (PyO3
bindings building `bullterrier_image._native._core`; generic, names no effect). A ported effect has **no Python folder**: the registry registers every
effect the core declares (`registry._register_core_effects`) with a function that calls `_core.apply("<name>", rgb,
params.model_dump())`. Its Rust file declares the parameters (`Params`, see "Parameters"), its section of the catalogue, default
opacity and inputs (`pub(crate) const META: Meta`) and has `pub(crate) fn render(picture: &Picture, params: &Params, inputs: &Inputs) -> Image`; its
documentation is the `///` doc of `Params`, its Python model `class XParams(EffectParams, core="x")` is built from that
declaration, and category and opacity come from the core (`@effect(params=XParams)` on a hand-written Python model of a core
effect refuses them). The core's registry (`Effect` trait, `register`) also
takes foreign effects, such as a Python function (`_core.register(name, category, opacity, fn, *, inputs=, fields=)`; `fn(rgb,
params, **inputs)`). `Effect::apply(&Picture, params, &Inputs)` hands an effect only the inputs it declares: `size_ratio`
(1 if undeclared), `partition` (`Labels`), under each field parameter's name a `Layer` and under each lookup table parameter's name a `Lut`
(`Inputs::lut`; none is the identity). A `lut` parameter (`LutName`) holds the name of a table the photo was given
(`Photo::with_lut(name, Arc<Lut>)`), else the path of a `.cube` file read once per render, else an error naming it; the runner
finds the table before step 1 and offers it, so the table never travels in the recipe (`crate::Lut`, `lut.rs`, `lut/cube.rs`:
the `.cube` reader and trilinear lookup are the core's). A `picture` parameter (`PictureName`, nullable, none = no picture) is the same route:
`Photo::with_image(name, Arc<Image>)` (straight-alpha RGB/RGBA), else the path of an image file read once per render by `files::read`; the effect
finds it as `Inputs::image(parameter)`; Python's `Picture` travels as `$picture:<digest>` and `pictures=` (`run/assets.rs`). Rust effects never refer to each other.
The recipe is Rust too (`crates/core/src/recipe.rs`): `Recipe`, `Step`, `Each` are plain values, built in code or read
(`Recipe::from_toml`, `from_json`; the same types are the JSON form); `Step::of(grade::Params {..})` builds a step from an effect's typed parameters. `Recipe::checked(&Presets)` applies the rules of a
look, `Table::check` those of an effect's parameters (`_core.check`, `_core.check_recipe`, `_core.read_recipe`). The Python
`Step`, `Recipe`, regions and `Framing` are Pydantic builders over these types: their models validate (and document) first,
and a `Recipe` is then held to `Recipe::checked`, which alone reads a region's path data.
**The runner is Rust too** (`crates/core/src/run.rs`): `Photo::new(Picture)` borrows the picture; `with_map(name, Layer)` hands
a map in, `with_finder(name, Arc<dyn Finder>)` gives a finder (`Finder::find(&Picture)` only; its answer for the whole photo is kept and asked once;
a `Subject` region found `per` piece also asks the `subject` finder, uncached, about the crop of each piece);
`photo.render(&Recipe, &Presets, Options)` and `photo.trace(..) -> Vec<Stage>` run one engine (`Stage`: `Input`, `Subject`,
`Depth`, `Selection { name, layer }`, `Partition { name, labels }` (each region the steps use, in declaration order),
`Step { number, image }`, `CutOut(image)`; numbered-piece drawing and labels are the facade's). The recipe's `frame` (typed `Frame`, checked by
`Recipe::checked`) frames the photo and its maps first (flip, turn, crop, borders: `ops/frame`, `ops/rotate`, one `Geometry`
planned from the shape and the subject, applied to each plane), then `Options::with_size` or the recipe's `size` resamples them
(`ops/resample`); the ratio of design px (480 on the longest side) is derived from that working picture, never passed in. Each
step's parameters are completed (preset, defaults, `Table::check`) and scaled by it (`Table::scaled`, from the declared units),
the result is laid over its input by `ops/blend` (blend mode, opacity, mask), and a `Drive` is a mix of runs at `levels` values
(`ops/levels`). After the last step `frame.transparent` gives the `Image` an alpha (`channels()` 3|4; `into_values` is
`channels()` per pixel, straight alpha; `ops/cutout`); effects read a `Picture` and never see it. `Layer` (an h x w map, finite,
0..1) and `Labels` (piece numbers) are the maps. **Files** (`crates/core/src/files.rs`, `files/`): `read` (RGB or RGBA with straight alpha, upright, first frame, an embedded
profile converted to sRGB), `read_map`, `write` and `write_with_quality` (png, jpg, webp; only the sRGB profile
embedded); a fixed pixel limit and a decoder memory cap; `ErrorKind::ImageRead`, `ImageWrite` and `BackendUnavailable`.
`imageio.py` calls them through `core/native.py` and keeps what is Python's (composite over white, array checks, Pillow
conversions). `Error::kind()` names the package exception of an error (`ErrorKind`, one mapping
for every binding). **Regions:** `Recipe.regions` is an ordered `Regions` of the 13 typed kinds in `bullterrier_image::region`
(`Region`, `kind` required; ranges in `Region::check`); `Recipe::needs(&Presets) -> Needs { maps, finds_pieces }` and
`Recipe::known_maps()` say what a photo must be given or find. Maps are found before step 1; every other region is made the first
time a step asks (`run/regions.rs`, `Evaluator`) and dropped after the last step that needs it. `each` is `run/each.rs`
(`ops/blur`, `ops/random`: xoshiro256++, not NumPy's stream). Every region kind runs in the core, a torn `edge`
included. The core opens one file, `Svg.file` (`run/laid.rs`: read through `take(max_kilobytes * 1024 + 1)`; `ops/svg` takes the
bytes: UTF-8 only, `<!ENTITY` refused before the parser, nothing in the file opened or run), and its limits are `Limits` in
`ops/outlines` (`Limits::STANDARD`; tests pass a small one). `Pick` by `id` reads the ids an `Svg` made (`Made::Pieces`);
a relative file path in a recipe file is the loader's to resolve. `Shards.preset` is expanded when the region is read
(`recipe/region/presets.rs`, from `crates/core/data/regions/shards.toml`; Python reads them through `_core.region_presets`). Every user number that drives
allocation or work has a maximum and a row in `crates/core/tests/it/limits.rs`; its completeness test fails for a bound of
`Region::check` with neither a row nor a reason.
Python is a facade over the core and runs no recipe itself. It reaches the core through `core/native.py` (every entry point raises the `BullterrierImageError` subclass of the
`kind` of the `_core.CoreError` the extension raises; any other exception, such as a Python effect's, passes unchanged) and
`core/bridge.py` (`render`, `render_all`, `trace`, `maps_needed`, `needs_finder`, `known_maps`: a `Recipe` as the dict
`steps.recipe_dict` writes; `size` a whole number, a name in `maps` or `finders` no recipe has, and each map the recipes need
found once, by `core/finder.py`'s `find_map`, are its checks; a finder's not-found error is told to the core). `recipe.py`
binds the default finders and names a trace's stages (drawing the numbered pieces is `ops/pieces`). The registry
mirrors every Python-registered effect, but those with a Rust model (`core=`), into the core at registration; a `Picture`
parameter travels as the name `$picture:<digest>` and a `Lut` as `$lut:<digest>` of what the render is given (`registry.holding`);
the Python `Lut` wraps the core's table (`_core.Lut`). `_core.effects()` lists those mirrors too;
`effects(declared=True)` only the effects whose parameters the core declares.
**Threads:** `Options::with_threads(n)` (default 1: none started; 0: every core; at most 256, else refused) makes `render` and
`trace` install a rayon pool of that size (`ops/par.rs`); the render stays on the calling thread, only row bands run on the
pool, and rayon's global pool is never used. A kernel fills its output with `par::rows(out, row_len, |first_row, band| ..)`
(`Picture::map` and `map_at` do, so a per-pixel effect is threaded as written); outside a pool it runs inline, and
`Effect::apply` takes no thread parameter. Rule: the output looks the same at any thread count and is identical where that comes naturally (a band reads only
input rows and carries nothing from one row to the next; a reduction chunks by the data, never by the thread count). Never add
real complexity for the last bit: if a kernel's rounding differs between runs with no visible effect, compare within a small
tolerance. Tests: render at threads 1, 2, 3, 10 and compare (`crates/core/tests/it/threads.rs`). Python: `render`/`trace`/`render_all`
take `threads=`; `Step.apply` and `fx` use 1.

Imports flow **downward only** (import-linter: contracts in `pyproject.toml`, run by `tests/test_layering.py` and
`uv run lint-imports`). A forbidden import means the code is in the wrong layer: move it.
Nothing imports `effects`: the registry loads every entry point of the group `bullterrier_image.effects` on first query. In a
checkout, `uv sync` installs ours; without it no effect is found.
The runner is given each map by name, as an array (`maps={"subject": alpha}`) or a `Finder(identity, find)`
(`finders={"subject": ...}`); the core's `Photo` takes the same (`with_map`, `with_finder`). `subject`, `depth` and `light` (made
from the working picture: framed and sized, before any step) are built in, and `background` is 1 - `subject`; any other map is a recipe's `given` region.
`bullterrier_image.recipe` binds the default `subject` and `depth` finders when a recipe needs them.

## Rules

@docs/rules.md

Those rules apply here. What follows is image's own, or where it needs a rule differently, with the reason.

## Image's rules

1. **Effects never import other effects.** Shared logic goes to `ops/` (default "One concept, one module": `warp`, `noise`,
   `edges`, `colorspace`...). An `ops` module arrives with its first consumer (`vulture` fails on unused code). `ops` is
   internal.
2. **Presets and looks are data**: a preset is a named parameter set of one effect, `src/bullterrier_image/presets/<effect>.toml`; a look is a multi-step recipe with a `description`, `src/bullterrier_image/looks/<name>.toml`, loaded by name (`load_recipe("crt")`).
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
An effect the core runs **declares its fields in Rust**, on its public `Params` struct (`#[derive(Debug, Clone, PartialEq, Deserialize, Params)]`, from
`crate::params`): each field a type, a `///` doc comment and `#[param(default = .., unit = px, min = .., max = ..,
at_most = other, drivable, advanced, cyclic)]` (`at_most = other`: the number may not exceed the number field `other`, of the
same unit, as `band_min_height` stays below `band_max_height`; `Table::check` and the Python model both apply it); a choice is a fieldless `#[derive(Choice)]` enum (`default = Variant`). The struct's own
doc is the model's docstring, a field's doc its help text in Python: plain text, no rustdoc links, a blank `///` line
becomes a double space. The build refuses a broken declaration, naming the field (an unknown unit is rustc's own
error; that a choice's doc names every variant is checked when Python builds the model). The Python class is
`class XParams(EffectParams, core="x")` with an empty body (`# noqa: D101`); the model is built from the Rust
declaration, a default or bound keeping the literal as written (a whole number stays an `int`). Python's model validates a
user's values when a step is made; `Table::check` in Rust is the same rules, compared with Python's by parity tests
(`tests/_native`); running an effect trusts its parameters. Rust kinds: `f32`, `i32`, `u32` (a seed: 0..4294967295), `bool`, `Color`, `Vec<Color>`
(`#[param(default = ["#12002b", "#ff2bd6"], min_length = 2, max_length = 8)]`: `min_length` of at least 1 required, `max_length`
optional, the default lowercase `#rrggbb`; Python sees a `tuple[Color, ...]`), `Option<Color>` (`#[param()]`, no default: none until
a step gives one, and the `///` doc says what none means; written `nullable`; Python sees `Color | None`), `FieldName`
(`#[param(default = "depth")]`: the name of the selection the effect works from, which it finds as a `Layer` in `Inputs`
under the parameter's name, and `Native` lists as one of its `fields()`; Python sees a `FieldName`), `LutName` (`#[param()]`, no
default: none is the identity table; the table reaches the effect in `Inputs`, see "Rust core"; Python sees a `Lut`), `PictureName`
(`#[param()]`, nullable, no default: none is no picture; Python sees `Picture | None`), `String` (text:
`#[param(default = " .:-=+*#%@", min_length = 2, max_length = 128, min_distinct = 2)]`, each rule optional and counting characters,
`min_distinct` the fewest different ones; Python sees `str`) and a
choice; an effect with any other (other optional values) keeps a Python model, each field `param(default, doc=...)`, until its
port extends the macro.
Every field has a default and a description (what it does, unit and range in words, what raising it does).

**The format.** What a parameter is, as JSON (keys, kinds, the closed list of units, ranges, `cyclic`, `drivable`,
`advanced`), is the ecosystem's parameter format, version 3: [docs/parameter-format/parameters.md](docs/parameter-format/parameters.md),
with its [schema](docs/parameter-format/schemas/parameter-3.schema.json) and
[fixtures](docs/parameter-format/fixtures/parameters). The copy is not edited: a change is a new version in the
ecosystem's format first, then a new copy here. `tests/_native/test_parameter_format.py` validates every field of every
Rust-declared effect. The Rust declaration covers every kind but a nullable `text`/`field` and a number list; `picture` (`PictureName`)
is a name or path, never a colour, and `usize` counts are kind `int`. What follows is what image adds.
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
