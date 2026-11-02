# Design decisions

The decisions that shape bullterrier-image and still hold, each with its reason and what it costs or commits us to. Rules for
contributors are in [CLAUDE.md](../CLAUDE.md); what is planned is in the [effects roadmap](effects-roadmap.md).

## Architecture

- **Linear layers: app > adapters > effects > core > ops > errors.** Each layer imports only the layers below it; the
  exception hierarchy (`bullterrier_image.errors`) imports nothing, so it is the bottom layer and every layer may raise it.
  *Reason:* one direction of dependency is the simplest rule that stays true as the project grows; a forbidden import
  means the code is in the wrong layer, so the fix is always a move, never an exception. *Consequence:* the rules are
  import-linter contracts in `pyproject.toml`, run by the test suite (`uv run lint-imports`); `core` never imports
  `effects`: the registry loads the effect library through an entry point (see "Effects are found through entry
  points"). The Rust extension lives in a package of its own, `_native` (the compiled `_native._core`), at the bottom
  beside `errors`: the linter cannot read a compiled module, but it sees the package, so an upward import from it, or
  an import of the package root from anywhere, fails like any other.
- **A microkernel with plugins.** A small, stable `core` (registry, parameter models, recipe engine) and many effects that
  register themselves and are found by discovery. *Reason:* adding an effect must not edit shared files, so many effects
  can be written in parallel without conflicts. *Consequence:* an effect is new files named for it (its package, Rust
  kernel, tests, example recipe, gallery picture) and nothing else; `poe new-effect` writes them from `effects/_template/`, and
  `gallery.effect_files(name)` is the one place that knows where they live.
- **Effects are pure functions in packages.** `effect(rgb, params) -> rgb` on float32 RGB `(h, w, 3)` in
  0..1, deterministic, never modifying its input, one package per effect holding code only; its tests mirror `src/`
  (`tests/effects/<name>/`), its example and picture live outside the package. *Reason:* a pure function is trivially
  testable and composable, and keeping artefacts out of `src/` keeps the wheel small. *Consequence:* effects never import
  each other; shared numeric code goes to `ops/`, named by concept.
- **A second picture is an asset: a parameter whose value is a picture.** An effect that uses a
  file of its own (a texture to blend, a logo, the photo inside a double exposure, a colour reference, a displacement
  map) declares a `Picture` field (read-only RGBA pixels with a content digest, in `core/params.py`). A recipe file writes a path,
  relative to the recipe file, and the recipe reader reads it with `imageio` once, when the recipe is loaded; `fx`
  takes a path, an array, a `Picture` or a colour (a colour is a flat 1x1 picture, which is why every Picture field has
  a default). The effect receives an RGBA
  float32 picture at its own size and places it itself (cover, tile, anchor and scale are its own parameters), because
  a logo, a texture and a colour reference are placed differently. *Reason:* the core and the effects never see a file
  or a folder, `imageio` stays the only reader of pictures, a missing file fails at load naming the step and the path,
  and video can pass a new array for every frame. Combining several photos as equals (`collage`, `transition`,
  `morph`, side by side) is not an asset: it needs several inputs or a canvas of its own, so it belongs to video or a
  layout package, and before/after is the `compare` command. *Consequence:* the recipe reader moved from `core` to the
  adapters layer, beside `imageio` (`bullterrier_image/loader.py`); the pipeline's fingerprint covers each asset's content, not only the recipe text;
  a large asset is counted nowhere (the effect contract measures the budget on an effect's default 1x1 pictures, and a recipe's
  assets are held as loaded), so a real budget for assets is a decision still to make; another kind of asset file gets its own value type and reader in the adapters layer together with its first effect:
  the `.cube` lookup table is `Lut` (`core/params.py`), read by `imageio.load_lut` (`LutError` names file and line) and held by `lut`'s `file`.
  `lut` interpolates trilinearly with no parameter for it: nearest-entry lookup only bands, so no look needs the choice.
- **Effects are found through entry points, on first use.** The registry imports every module of the entry-point group
  `bullterrier_image.effects` the first time it is queried, once, under a lock; bullterrier-image's own effect library is one such entry
  point and a third-party package declares its own, written against the public `bullterrier_image.plugin`. *Reason:* importing
  every effect (and scipy, pydantic) made `import bullterrier_image` cost a quarter of a second before anything was used, and
  left outside packages no way into recipes or the command line; entry points are PyPA's standard plugin mechanism
  (pytest, Sphinx, flake8 use it). *Consequence:* a checkout needs `uv sync` to install the entry point; a plugin that
  fails to import is a `BullterrierImageError` naming it, `BT_IMAGE_PLUGINS=builtin` skips third-party packages, effect names
  are unique across every installed package, and registering is for import time (queries are thread-safe once loaded).
- **`ops` modules arrive with their first consumer.** *Reason:* code nothing uses is a cost with no benefit, and a
  primitive designed before its user is designed for the wrong user. *Consequence:* the dead-code gate fails on unused
  code; the removed primitives (`colorspace`, `noise`, `edges`, `warp`, `voronoi`, `sprites`) are restored from commit
  3b9b2ed (`git show 3b9b2ed:src/bullterrier_image/ops/<module>.py`) together with the effect that needs them.
- **One public path per name.** The public API is `bullterrier_image` (with `render` & co. binding the default finders),
  `bullterrier_image.recipe`, `bullterrier_image.fx`, `bullterrier_image.imageio` and `bullterrier_image.plugin`, each with `__all__`. *Reason:* two paths to one name with
  different behaviour (a `render` that finds the subject and one that does not) is a trap. *Consequence:* `bullterrier_image.core`
  and `bullterrier_image.ops` are internal and re-export nothing; no compatibility shims before 1.0.
- **No time in the library.** Effects are pure functions of one picture and a set of values; a parameter describes a state
  (`progress`, `phase`, `seed`), never a duration. Easing, ramps, animated values, the animation timeline, frame
  rendering and GIF/MP4 output were removed. *Reason:* bullterrier-image is the image package of a wider ecosystem
  in which one package owns each concept: `control` shapes values over time, `video` maps them
  onto parameters and renders frames, `audio` supplies the beats. Animation done here would exist twice, and a parameter
  measured in seconds would tie an effect to one clip and one clock. *Consequence:* a parameter that moved with time
  becomes a state parameter in `cycles` or `fraction` (disperse's shimmer is `ambient_phase`); the removed code is in git
  history (before the "remove animation" commits) for when video is built.
- **Typesetting belongs to bullterrier-vector; image keeps glyphs as texture.** Shaping, layout and text as outlines are
  vector's. A caption in image is an `svg` region (vector's outlines) plus `fill`, `outline` or `drop_shadow`; there is no
  `text` effect. Effects that draw glyphs as texture (`ascii`, `disperse`, later `braille`) stay, on image's
  own font. *Reason:* a text engine (shaping, bidi, wrapping, font fallback) is a package's worth of work, and two would
  drift apart. *Consequence:* `text` and `type_on` leave the roadmap.
- **One bundled monospaced font, no `font` parameter.** Glyph effects draw with JetBrains Mono Regular (SIL OFL 1.1),
  shipped in the wheel as `src/bullterrier_image/fonts/JetBrainsMono-Regular.ttf` (`ops/glyphs.FONT`). `sheet`, the banner
  and the piece-number digits use the same file. There is no `font` parameter, `BT_IMAGE_FONTS`, system-font search or
  Pillow fallback. *Reason:* a font found on the system made the output depend on the host, a missing path failed at
  render time, and the file escaped the cache fingerprint; one cited asset gives the same look on every host with the
  same library versions. It covers the Latin ramps and the block elements `█▓▒░▀▄`, which a system monospaced font often
  lacks; braille is not covered (a later `braille` effect draws its own dots or picks its font then: YAGNI). Pillow's
  basic layout engine is pinned: its optional Raqm engine keeps fractional advances where the basic one rounds, which
  moves a glyph a pixel. *Consequence:* the licence is `MIT AND OFL-1.1` with the OFL text at the repository root
  (`LICENSE-FONT`); a `Font` value type arrives only on request.
- **`drop_shadow` is every silhouette-copies look.** It lays `copies` offset copies of the silhouette over each other,
  each at `copy_opacity` falling by `fade`, from `color` to `end_color`: one copy is a shadow, a few with `blur` 0 are
  cut paper, many at a small `offset` in one colour are a long shadow (preset `long_shadow`). *Reason:* three effects
  shared the silhouette, the shift and the layer behind the subject. The old `long_shadow` took the maximum over its
  copies with a subtractive fade; stacked copies compound where they overlap instead, so the preset differs by up to
  0.08 (mean 0.006) at 480 and 1920 px, and its shadow's edge is softer where few copies overlap. The cost is one shift
  per copy: 150 copies take about 3 s at 480 px and half a minute at 1920 px, against the maximum's log-time doubling.

## Parameters

- **Every effect declares its parameters once, as a frozen Pydantic model** (`EffectParams`, `extra="forbid"`, strict
  fields; for an effect the Rust core runs, the model is built from its Rust declaration, see "Rust core"). *Reason:* one declaration drives validation, defaults, `help(fx.<effect>)`, `bt-image effects`,
  `docs/effects/`, `fx.pyi`, the `fx` signatures and the JSON schema, so they cannot disagree. *Consequence:*
  `@effect(category=..., params=...)` requires the model; a misspelt or out-of-range value fails at construction or
  load, naming the effect, the field, the constraint and the value, never deep inside a render.
- **Nothing hidden.** Every value that shapes a result, including options of the libraries an effect calls, is a named,
  documented field with a unit and an honest range; only mathematical and standard constants are exempt, each cited.
  *Reason:* a look must be reproducible and tunable from the recipe alone. *Consequence:* a test scans effect packages
  and `ops` for unexplained numbers; exposing a tunable never changes default output (proved byte-identical).
- **Basic and advanced parameters.** A field is advanced (`param(..., advanced=True)`) when the user would change it to
  tune the technique rather than to choose the look (`edge`, `truncate`, `interpolation`, a secondary shaping control);
  aim at six or fewer basic fields per effect. *Reason:* "nothing hidden" exposes every tunable, and one flat list of
  dozens of them buries the few controls that define the look. *Consequence:* the flag changes listing only: basic
  fields first, then an "Advanced parameters" section (`help`, `fx.pyi`), a count in `bt-image effects` (`--advanced` or
  naming the effect shows all) and a collapsed block in the catalogue. Every field stays settable, validated, in
  declaration order in the signature and in the JSON schema (`"advanced"`). No tiers, no classification by name.
- **An angle that wraps says so: `cyclic=True`.** A degrees field whose value and the value a turn further mean the same
  picture (a direction, a rotation, a hue) is flagged; what the flag means (a range of exactly 360, any real wrapped into
  it) is the [parameter format](parameter-format/parameters.md)'s rule, and image keeps the bounds themselves (360 stays
  360). Other angles (an elevation, a lean, a twist whose strength falls off with distance, a shift scaled by a weight)
  stay bounded and refuse what is out of range. *Reason:* ranges were 0..360, +-180 and +-360 by habit, and a caller that sets an
  angle by a rule could not tell from the description whether 370 wraps or is refused; one flag in the description lets it wrap by
  the flag, with no per-effect knowledge. *Consequence:* the documentation says `(cyclic)` and the schema `"cyclic"`;
  a contract test holds each flagged field to the property that makes it cyclic (the picture at `min` is the picture at
  `max`), which catches a field flagged by mistake (`selective_color.hue_shift` scales with a weight: not cyclic). A
  `Drive` of a cyclic field may run past the turn. The JSON schema keeps the turn as `minimum` and `maximum`
  beside `"cyclic"`, so a generic schema validator refuses a value the model wraps. A half-turn symmetry (a line, a square grid) is not a turn: the field is
  still one full turn wide.
- **Names are the field's own terms** (radius, sigma, amount, threshold, gamma, exposure, seed...), never `strength`,
  `intensity` or `factor`. *Reason:* users bring vocabulary from Photoshop, Lightroom and shader literature.
  *Consequence:* a shared vocabulary table and a contract test flag discouraged names.
- **What lies beyond the picture's edge is always a field named `edge`** (reflect, nearest, mirror, wrap, and constant
  where supported), in every effect that looks past the border. *Reason:* one concept, one name; `mode` is often taken
  by an effect's own choice (`radial_blur`: spin or zoom). *Consequence:* a contract test fails an effect field whose
  choices are edge rules but whose name is not `edge`.
- **`interpolation` is how an effect reads the picture between pixels; `resampling` is the same choice for a generated field.**
  `fog` and `reaction_diffusion` warp or upsample a field they made themselves, then colour or blend it, so the
  contract's "a constant picture stays constant under every interpolation" does not hold for them. *Reason:* the contract
  rule is about warping the photo; a second name keeps it simple instead of an exemption list. *Consequence:* a field
  named `interpolation` promises that rule; use `resampling` when the effect does not.
- **The picture an effect takes is called `image`.** `blend` takes a `Picture` and places it with `fit`, `at`, `scale` and
  `margin`; its picture was `texture`, and the logo of the folded-in `overlay` was `logo`. *Reason:* the same kind of
  parameter has the same name, so a developer who has used one picture effect knows the next. *Consequence:* a later
  effect with one picture names it `image`; one with several names each for its role.
- **A logo is `blend` with `fit="width"`, not an effect of its own.** `overlay` was `blend` in the `normal` mode with the
  picture sized by the photo's width; folding it leaves one placement model and seven parameters instead of twelve, and
  the preset `logo` keeps the short way to write it. `margin` is a length in px (the lengths rule).
- **An effect's controls are named scalars; a list of numbers is for data, not for controls.** `channel_mixer` takes
  nine fields (`red_from_green`...) and `curves` a parametric curve (shadows, midtones, highlights, per master and
  channel), not a `matrix` or a `points` list. *Reason:* only scalars can be driven by a control signal from outside, become a slider
  with a unit and range in the UI generated from the effect metadata, and carry their own documentation and error
  message; a list is validated per item only, and a free-point curve needs a curve editor that sliders cannot give.
  *Consequence:* a draggable-point curve or a free-length LUT is a separate, later effect (`point_curve`, `lut`) with
  its own design; if a list parameter is ever added, any in-range list must be valid by construction (evenly spaced
  output heights, not x/y pairs that must increase), and the contract's limits test then handles list fields.
- **`grain` is one effect for film grain and sensor noise: `amount` at mid-grey and a `balance` toward the shadows or
  the highlights.** Flat grain is `balance` 0; `color` gives each channel its own noise; `distribution` picks the
  noise. *Reason:* `grain`, `film_grain` and `noise` were one idea in three parameter sets; `amount` keeps its meaning
  and the grain at brightness L is `amount` x (1 + `balance` x (2 L - 1)), so `film_grain(shadows=s, highlights=h)` is
  `amount` (s + h) / 2, `balance` (h - s) / (h + s). *Consequence:* grey grain draws the stream of the old `grain` (and
  of `noise(color=False)`, equal to 6e-8); colour grain draws that of `film_grain` (channel seed = `seed << 2 |
  channel`), so colour `noise` keeps its distribution but not its random values.
- **`particles` is one effect for rain, snow, embers, dust and stars: dots and streaks at random depths.** `length` 0 is
  a dot (there is no `shape`); `angle` is the streak's direction clockwise from right. A particle at nearness d (1 =
  nearest; `layers` n puts it on plane k / n) has size and length 1 - `size_falloff` x (1 - d) of the nearest and
  brightness 1 - `brightness_falloff` x (1 - d). *Reason:* the five effects differed only in these terms; snow's full-
  range size with its half-range brightness is why the two falloffs are separate. A preset cannot set `opacity` (a step
  key), so a look's usual opacity (embers 0.9, dust 0.55, rain 0.7, snow 0.8, stars 1) is written in the step.
  *Consequence:* the stars of `starfield` (now a one-colour `gradient` and the preset `stars`) take the common depth direction, so the
  same seed gives the same star positions with other sizes and brightnesses; counts became `density`.
- **`glitch` is a recipe: `slice_shift(preset="glitch")` then `rgb_split(angle=0, edge="wrap")`.** *Reason:* it was a
  band slide followed by a red/blue roll, each already an effect; the two steps are as short to write and sweep as the
  one effect was (`rgb_split.amount`, a negative `split` is `angle = 180`). `slice_shift` gained `bands` (extra
  strips of random height at random rows, laid on the tear) and `slices = 0` (no tear), so a few bands alone make the
  glitch; both stages draw from one `seed`, the tear first. One band slide, `ops.shift.slide_bands`, serves
  `slice_shift` and `vhs`; each draws its own bands. *Consequence:* a whole-pixel `split` is no longer rounded at the
  recipe's scale: `rgb_split.amount` is a float, so a split of 9 px at 0.3125 scale blends 2.8 px instead of
  rounding to 3.
- **A parameter may name a field.** `field: FieldName = param("depth", doc=...)` holds the name of a selection, written
  as `mask` and `Drive.by` are: `depth`, `light`, `subject`, `background` or a region of the recipe (`field = "light"`,
  `fx.relight(field="light")`). The step checks the name when it is made, as it checks `by`: it exists and is a
  selection. The runner builds only the fields some step names (no depth model runs for `field = "light"`) and hands
  the effect the resolved float32 `(h, w)` array 0..1 as the keyword-only argument of the parameter's name
  (`def relight(rgb, params, *, field)`); a direct call passes that array (`registry.apply("relight", rgb, field=arr)`).
  A field is not a number: no address, not drivable. A step may have a field and a `Drive` on another parameter: the
  field is resolved once and every level of the drive gets the same array; the drive keeps its own `by`. Every effect
  that works from a map takes it as a field, defaulting to the map it was made for: `outline`, `rim_light`,
  `reflection`, `drop_shadow` and `disperse` to `subject`, `parallax` and `ambient_occlusion` to `depth`, so an outline
  or a shadow can follow text or a logo, and a shape's threshold is `field_threshold`. *Reason:* what an effect works from (depth, light, a band) is a choice of the look, so
  it is data in the recipe, not a second effect per source or an input only code can pick. *Consequence:* registering
  an effect fails unless each `FieldName` parameter has a keyword-only argument of its name; a map a field needs and
  nobody gave is the usual `MapNotFoundError`.
- **`focus_blur` is one effect; the miniature is a recipe.** The blur depends only on how far a field lies from `focus`, so a
  depth gives a lens, a linear gradient gives a tilt-shift band, `light` or a given map gives any other. The colour a miniature
  adds is not blur: it is a `grade` driven by the band that stays sharp (`examples/effects/focus_blur.toml`). *Reason:*
  `depth_of_field` and `tilt_shift` were the same blur with two ways to find a distance; a saturation in the blur effect
  would be a parameter the depth look never wants. *Consequence:* the blur is byte-identical to both old effects; the
  saturation is not, because `grade` takes its grey from the blurred picture where `tilt_shift` took it from the sharp one
  (up to 0.23 at the old example's settings).

## The code API

- **The code API is the product; a recipe file is one serialisation of a `Recipe`.** As in numpy, polars or PyTorch, a
  look is built, changed and run in code (`Recipe`, `fx`, `render`, `Step.apply`, `replace`, `with_values`); TOML is how
  one is saved and shared (JSON and YAML later), read into the same validated `Recipe`, never a model of its own.
  *Reason:* an API review scored the Python side about 6/10 because it was shaped around the file: the photo contract
  was the caller's to know, the recipe's name was a file-name rule, its description existed only in TOML. Files stay
  because the command line runs them, looks ship and are shared as data, a service takes looks as data, and video hands
  looks over as data. *Consequence:* every capability exists in code first; the README and the docs lead with code and
  introduce TOML as the saved form; the Rust core ports this API, not the file format.
- **The serialised form is one canonical dict (design; the Rust types are this form since R3, Python's follows in R4).** `Recipe.to_dict()` / `Recipe.from_dict()` give
  and take `{"name", "description", "size", "frame": {field: value}, "regions": {name: {"kind": ..., field:
  value}}, "steps": [...]}`: a step tagged by `effect` (`{"effect": "bloom", "preset": "glow", "params": {...}, "mask",
  "opacity", "blend_mode", "name", "partition"}`), an `Each` by `each` (`looks` holding step dicts and `null`), a region
  by `kind`, a picture or lookup table by `{"path": ..., "sha256": ...}`, the preset's name and the description kept.
  The Rust types differ in two ways: they have no `format` (a TOML file's version only gates what it may use, and is
  not part of the recipe), and `frame` and `regions` are opaque tables there until R4 gives them types.
  TOML, JSON and YAML are codecs over that dict (`save_recipe`), the pipeline fingerprints the dict, not `repr`.
  *Reason:* one shape for every format, and the one Rust's serde form takes. *Consequence:* a step must keep its preset's
  name beside the merged values (today it keeps only the values).
- **A photo is checked once, where it enters.** `render`, `trace`, `render_all`, `Step.apply` and `imageio.save` (and
  `to_pil`) take a numpy array through `ops.image.as_image`: unsigned integer levels are divided by their type's largest
  value (uint8 by 255, uint16 by 65535), floats must be finite (`NonFiniteImageError`) and within 0..1; any other shape
  (greyscale, RGBA where only RGB is taken), `bool`, signed integers, `uint32`/`uint64` (levels of no known white), a Pillow image or a list is an `InvalidImageError`
  naming what was found and the fix. Nothing is clipped. *Reason:* a uint8 photo rendered silent garbage and saved all
  white; the contract was the caller's to know. Signed integers are refused, not divided: an `int64` array of 0..255
  levels (numpy's default) over 2^63 would render black without a word. *Consequence:* inside the library nothing
  re-checks the photo; the registry only makes its layout (float32, C-contiguous) for an effect.
- **One argument by position, the thing itself.** Effect parameters are keyword-only (`fx.bloom(radius=12)`), and
  `Recipe(steps)`, `Step(effect)`, `Each(each)`, `Drive`, `Grid` and `Framing` take everything else by keyword.
  *Reason:* a value read by its place silently changes meaning when a field is added or reordered; Rust's struct
  literals have only names. *Consequence:* the editor stub and `help` show keyword-only signatures.
- **Copies are checked; nothing builds an unchecked value.** `Step.replace(**changes)` takes what `fx` takes (parameters
  laid over the step's, step keys replacing its), except `effect` and `preset`: a step keeps the merged values, not
  which came from its preset, so a new preset would silently keep the old one's values (a later batch that keeps the preset's name
  lifts this). `Recipe.replace(**changes)` takes the recipe's fields, both through the
  constructor; pydantic's `model_copy(update=...)` and `model_validate` are routed through the same validation and fail
  with a `RecipeError`. *Reason:* `model_copy` built a step with a negative radius. *Consequence:* `replace` reads no
  file (arrays become a `Picture` or `Lut` as for `fx`; only `fx` reads paths): the data model does no I/O. `Recipe.model_validate(recipe.model_dump())` is the recipe again: a step is read as a
  `Step`, or an `Each` when it has `each` or `looks`, a region as the kind whose fields it has; `Given()`, which has
  none, dumps to `{}` and is refused when read back until R4's form names each kind. `model_validate_json` and
  `model_validate_strings` fail with a `RecipeError` too, and `model_construct` raises. Until R4 wraps pydantic
  instead of inheriting it, these are guards on pydantic's own entrances.
- **`Step.apply` is `render` of the one-step recipe.** `fx.bloom(radius=12).apply(photo, maps=, finders=, size=)` equals
  `render(photo, Recipe([step]), ...)`, and `render`/`trace` take a step as that recipe. *Reason:* trying one effect
  should not need a recipe. *Consequence:* the default subject and depth finders live above the core (they know
  `finding`), so `Step.apply` imports `bullterrier_image.recipe` when it is called, a deliberate upward import that
  import-linter cannot see; done at call time it also works in a fresh process (an unpickled step in a pool worker).
  Later: the structural fix is core-owned default finders, found through entry points, so the core needs no import.
- **A recipe's name is free text; whoever writes files makes it safe.** `Recipe(steps, name=None, description=None)`; a
  file without `name` takes its file stem as it is. The command line and the pipeline name files `safe_name(name)` (runs
  of characters other than letters, digits, `.`, `_`, `-` become one `-`, no leading dot or dash, at most 64, never a
  Windows device name); a recipe without a name, or whose name leaves none, is a `RecipeError` there; two recipes whose
  names give one file name are a `ProjectError`. *Reason:* a name is what a person calls the look; the file-system rule
  (CWE-22) belongs where files are written.
- **What only the library uses is not on a public class.** The runner's questions (`references`, `runs`, `drives`,
  `maps_needed`, `needs_finder`, `frame_photo`, `cut_out`...) and the parameter models' (`scaled`, `signature`, the field
  lists) are functions of their core modules, not methods. *Reason:* `help(Recipe)` and completion show what a caller
  uses; a leading underscore would have made the runner reach into private names.
- **Find things from code.** `bullterrier_image.looks()` lists the looks with their descriptions, `fx.<effect>.presets`
  an effect's presets with their values; hints are written as code (`fx.slice_shift(preset="glitch")`,
  `load_recipe("glitch")`), and a misspelt parameter or look gets the nearest names.

## Recipes

- **Recipes are data.** A look is a `Recipe`, an ordered list of steps built in Python (`fx`) or read from its saved
  form, a TOML file, validated when it is made. *Reason:* data can be diffed, cached by fingerprint, edited by
  non-programmers and generated by tools.
  *Consequence:* a recipe file may carry `format = <n>`, and a release that
  changes the format adds a number and keeps reading the old ones.
- **Presets are per-effect parameter sets, shipped as data in `src/bullterrier_image/presets/<effect>.toml`.** A look that is
  one effect with a set of values (sepia is a gradient map, teal-and-orange a split tone) is a preset of that effect,
  not a new effect: `fx.gradient_map(preset="sepia")`, or `preset = "sepia"` in a recipe step. *Reason:* a new effect
  is code, tests and a picture for what is a few numbers, and a preset is checked by the effect's own parameter model,
  so it can neither drift from nor duplicate the effect's rules. *Consequence:* `Step` merges the preset under the
  parameters it is given (the given ones win) when it is made, so the runner, the loader and the cache never see a
  preset; an effect with presets lists them in its documentation, the catalogue and the editor stub; only built-in
  effects have presets (a third-party effect's author ships data of their own); a look that needs several effects is
  a recipe, not a preset.
- **Looks are recipes shipped as data, loaded by name: `load_recipe("crt")`, `bt-image render PHOTO crt`.** A look that
  needs several steps (anaglyph, crt, glitch, hologram, tilt-shift, stained glass, low poly, spotlight, voronoi cells) is an ordinary recipe
  file in `src/bullterrier_image/looks/<name>.toml` with a top-level `description` (one sentence), found the way presets
  are: by the folder, not by a list in code. `load_recipe` and every command argument that takes a recipe file also take
  a look's name: a `str` that is a look's name is that look (a `Path`, or `./crt`, is always a file, so a folder of yours
  never changes what the name means); `bt-image effects` lists the looks first, under "Looks". *Reason:* one way to load a
  recipe, no `load_look` beside it; a look is a recipe, so it passes the same validation and the same tests, and it is
  data, so a look is added without touching code. *Consequence:* `examples/recipes/` keeps the demonstrations that are
  not looks; a look is not a preset (one effect) and not an effect (one picture in, one out, with parameters).
- **A name that is no effect says where it lives, every place.** `registry.get` is the one place an effect name is looked
  up, so recipes, `fx`, the command line and the gallery give the same answer. The message lists everything the name is:
  each preset (`fx.slice_shift(preset="glitch")`), a look (`load_recipe("glitch")`, and its own `description`), a
  parameter (`'vibrance' is a parameter of grade`), and each effect whose summary mentions it with underscores read as
  spaces, quoting the whole summary sentence (`'depth_of_field'` in `focus_blur`'s). Only a name that is none of these
  gets the close matches among effects, presets and looks. *Reason:* a cut effect lives on as a preset, a look, a
  parameter or a word in the survivor's summary (no aliases), and a name can be several (`glitch` is a preset and a
  look); a person or a model that remembers the old name must reach all of them from the error, and a cut summary hides
  what it says. The answer is read from the shipped presets, looks, parameters and summaries, never from a table of old
  names.
- **Every step can be limited and blended: `mask`, `opacity` and `blend_mode` belong to the step, not to the effect.**
  `mask` (`subject` or `background`), `opacity` (0 to 1) and `blend_mode` (normal, multiply, screen, overlay,
  soft_light, difference, color) are applied by the runner after any effect, so a gradient can
  sit behind the subject, be screened over the photo, or an effect can be mixed in at 40%, with no effect knowing about
  it: every step is a layer over the picture below it, as a filtered copy of a layer is in an image editor. *Reason:*
  one place for the general blend. Eleven effects (`drop_shadow`, `dust_scratches`, `embers`, `floating_dust`,
  `long_shadow`, `outline`, `rain`, `reflection`, `snow`, `soft_focus`, `sparkle`) had an `opacity` of their own that
  rendered exactly what the step's `opacity` renders, and in a recipe file the key always reached the step, so the same
  word meant two things depending on whether a look was written in TOML or Python; an effect's `mask` could not be set
  from a recipe at all. *Consequence:* no effect parameter is named like a step key (`effect`, `mask`, `opacity`,
  `blend_mode`, `preset`), and registering one fails, as a parameter named like an injected input does; the eleven
  fields are removed; an effect whose natural strength is below full declares the default of its steps' opacity
  instead (`@effect(..., opacity=0.7)`, read through `registry.opacity`): `Step`, `fx` and a recipe file use it when no
  opacity is given, an explicit value replaces it, and `fx.pyi`, `help(fx.<effect>)`, `bt-image effects` and the
  catalogue show it, so every bare default looks as it did and `opacity` still means one thing; a strength that is not
  a plain mix keeps a name of its own
  (`drop_shadow`'s `copy_opacity`, whose overlapping copies compound); never add an effect parameter that only mixes
  the effect's result with the picture. The blend-mode formulas live in one `ops` module that the runner and the
  `blend` effect share; `blend` keeps its own `mode` because a step's result is an opaque picture, and only the effect
  knows the transparency of the picture it places. A colour wash (`gradient_overlay`) is a `gradient` step with a
  `blend_mode`, not an effect.
- **The code fingerprint hashes every file of the package** (relative POSIX path and bytes, skipping `__pycache__` and
  dotfiles), presets and the font included. *Reason:* the cache key must change whenever anything that shapes a result
  changes; hashing only `.py` files let an edited preset or font serve stale pictures.
- **A step is written with the same keys in Python as in a recipe file.** `fx.<effect>(...)`
  takes `mask`, `opacity`, `blend_mode` and `preset` as keywords beside the effect's own parameters:
  `fx.bloom(radius=26, opacity=0.8, blend_mode="screen")` is the step `effect = "bloom"`, `radius = 26`,
  `opacity = 0.8`, `blend_mode = "screen"`. *Reason:* one name per concept across the recipe file, Python and the docs;
  `.where("background")` and `.blend(0.8)` were a second vocabulary for `mask` and `opacity`, and "blend" would have
  meant a method, an effect and a mode at once. The keys cannot clash with an effect's parameters because no
  parameter may be named like a step key (above). *Consequence:* `Step.where` and `Step.blend` are removed, no alias
  kept; `fx.pyi` shows the step keys in every signature, so an editor completes them.
- **Size changes frame the photo before the steps; transparency is cut after them.** A recipe's framing (`Framing`,
  `bullterrier_image.core.framing`: `[frame]` in a recipe file, `Recipe(..., frame=Framing(...))` in Python) is applied
  by the runner to the photo and its maps (the subject, the depth, any map handed in) together, before the first step, in one fixed order: `flip` (none, x,
  y, both), `angle` (clockwise degrees, with `fit` crop or expand and a `resample` filter), the crop (`aspect`
  as width over height, put at an `anchor`: center, top, bottom, left, right, or the subject with a `margin` around
  it), the `pad_top`, `pad_right`, `pad_bottom` and `pad_left` borders (shares of the picture's shorter side, in
  `fill`), and then the existing `size`. The subject is found on the photo as given, before any of it, so a crop can
  follow the subject, and the matte moves with the photo (it travels as a fourth channel through the same geometry in
  `ops/frame`, so there is one geometry to get right). A recipe's transparency (`transparent`: `background`, the
  subject matte becomes the alpha; `circle`, the circle inscribed in the finished picture; `rounded`, corners rounded by
  `corner_radius`, a share of the shorter side) is cut after the last step. Effects keep the same-shape RGB contract.
  *Reason:* cropping and rotating decide which picture the look is applied to (a vignette is centred on the final
  crop), and a masked step after them needs a matte that moved with the photo; one module that owns the geometry of both
  keeps it out of every effect, where shape-changing effects would each have to transform the matte as well. A fixed
  order is a decision made once instead of a setting. *Consequence:* `crop`, `smart_crop`, `resize`, `pad`, `rotate`,
  `flip` and `avatar` are recipe settings, not effects, their fields documented like effect parameters (enlarging by
  interpolation is a `size` above the photo's own); `render` returns RGBA (h, w, 4) when the recipe asks for
  transparency, `trace` ends in that cut, `imageio.save` takes RGBA as one array (there is no separate alpha argument,
  so a picture and its alpha cannot disagree) and refuses a JPEG with transparency, and a project that exports such a
  recipe to `jpg` fails before anything renders; a finder is now asked about the whole photo rather than the scaled
  one; a frame that needs a canvas larger than the photo (polaroid, film strip) belongs to a layout package; a border
  drawn inside the picture's edge is an ordinary effect. The transparency keys live in `[frame]` with the geometry,
  because they are the same model and one place says how a recipe's picture is framed and cut.
- **Maps of the photo are injected by name.** A map is one value per pixel, 0..1, the photo's size, known by a name:
  `subject`, `depth`, or the name of a `given` region the recipe declares (`format = 3`). `render`, `trace` and
  `render_all` take `maps={name: array}` and `finders={name: Finder}`; `pipeline.run` takes `finders=`;
  `bullterrier_image.recipe` binds the default `subject` and `depth` finders, lazily, only when a recipe will ask. An
  effect takes a map as a field parameter naming it (`field = "subject"`), never as an input of its own; a region of kind `given` makes any other map usable as a
  selection (`mask = "star"`). *Reason:* the engine stays testable without a model, and the package serves a video tool
  that hands in maps of its own every frame (a drawing's silhouette from vector, a motion map from several frames) which
  image can neither make nor know in advance; one route for every map, not a keyword per map (`matte=`, `depth=`, then
  whatever comes next). A name no recipe declares is a `RecipeError` listing the names there are, so a misspelt map
  fails instead of being ignored. *Consequence:* `matte=`, `depth=`, `find_subject=`, `find_depth=`, `--matte`,
  `--depth`, `load_matte`, `load_depth` and the effect input `matte` are gone, with no alias before 1.0 (`--map
  NAME=PICTURE`, `imageio.load_map`, `subject`); the five effects' `matte_threshold` is `subject_threshold` (since `field_threshold`); the `depth`
  region name is built in beside `subject` and `background`, so a `given` region cannot take it; `MapNotFoundError` is
  the base of `SubjectNotFoundError` and `DepthNotFoundError`; a project (`bt-image run`) holds no maps of its own, so a
  recipe with a `given` region needs a finder of that name. A map handed in is framed with the photo in one stack
  (`framing.frame_photo`), so its geometry is the photo's and every map pads with 0.
- **One reader for map files: transparency gives the alpha, anything else the brightness.** `imageio.load_map` replaces
  `load_matte` (alpha) and `load_depth` (brightness only). *Reason:* a cut-out PNG and a drawing carry the map in their
  alpha, a greyscale or black-and-white picture in its levels, and a depth map an editor saved with an opaque alpha
  channel must still read as brightness, so alpha counts only when some pixel is not fully opaque. *Consequence:* a
  depth map with real transparency now gives its alpha (save it without), and a cut-out whose subject covers the whole
  frame reads as brightness (it is no cut-out).
- **A finder carries its identity: `Finder(identity, find)`.** A frozen dataclass of a non-blank string and a function
  from the photo to a map; `finders={name: Finder}` names it for each map.
  `finding.subject.subject_finder(segmenter=None)` returns the default subject finder, named by its backend and model,
  and `finding.depth.depth_finder()` the depth's. The pipeline's subject, depth and crop caches are keyed by the
  identity. *Reason:* the cache key must tell finders apart, and a bare function or `functools.partial` has no stable
  name (the partial-finder cache collision). *Consequence:* the crop-finder alias in regions is `BoxFinder`; the package
  is `finding/` (`subject`, `depth`, `download`) and the models' variables are `BT_IMAGE_SUBJECT_MODEL` and
  `BT_IMAGE_DEPTH_MODEL`.

- **Pixel values are design pixels.** The [parameter format](parameter-format/parameters.md) makes a `px` value a design
  pixel; image's design picture has a long side of 480, always:
  `recipe.size` is only how big the output is (the working size: `render(..., size=)`, else the recipe's size, else the
  framed photo's longest side). A render multiplies the values by the working size over 480, in the one place a step's
  values reach its effect (`params.scaled`, from the parameter description: no per-effect code). A whole number is
  rounded; a value shrinks no further than its field's `min` and grows without limit, because a range is in design
  pixels. *Reason:* the README promised a look that matches at any resolution, but with `size=None` a draft at half size
  meant scaling every radius by hand, and with a size the output was always rescaled to it; defaults and presets are
  tuned for 480, so a `size` that also set what the numbers meant made one preset look different in every recipe it sat
  in, and a photo's own long side made them tiny on a large photo. *Consequence:* the scaled values are not checked
  against the range (the model is copied, not rebuilt); regions and the framing hold no pixel values (shares and
  angles), so only effects scale; noise and random placements keep their character at another size, not their exact
  positions. An effect whose light or density is per working pixel (`starburst` with every bright pixel radiating)
  declares `size_ratio`, an injected input (working px per design px; 1.0 outside a recipe), and gives each pixel its
  share, so the total stays the same; `jpeg_crush` crushes the picture shrunk to design size and adds only the damage
  back (repeated, hard-edged), so its 8 px blocks are design px and fine detail stays. *Reason:* a value on the frozen
  parameters was hidden render context that dumps, equality and fingerprints could miss. A reach follows the long side,
  never the width or the height, so a portrait photo gets the same values as a landscape one.

- **Marks: a count or a density.** A few marks (`dead_pixels.count`, `lens_drops.count`, `scratches`, `pieces`, `bands`)
  are a `count`: it reads naturally, and the marks' sizes are `px`, so the same count covers the same share at any size.
  Numerous marks (`particles`, `dust_scratches`, `flow_lines`) take a `density`, marks per 1000 design px², the same
  everywhere; `params.scaled` divides it by the size ratio squared, so the count of marks over the picture stays the
  same at any size. *Reason:* a density counted on the working pixels grew sixteen-fold in count at four times the size
  while each mark grew sixteen-fold in area, so a 1920 px render was far denser than a 480 px one. `flow_lines.length` is
  px along the line (steps = length / step), not a number of steps, so it scales with the picture like every length.

## Errors and I/O

- **One error hierarchy.** Library code raises `BullterrierImageError` subclasses (`RecipeError`, `UnknownEffectError`,
  `SubjectNotFoundError`, `BackendUnavailableError`, `ImageReadError`, `ImageWriteError`, `LutError`, `InvalidImageError` with `NonFiniteImageError`), each also inheriting the builtin it replaces.
  *Reason:* callers catch one type; `except ValueError` keeps working. *Consequence:* the CLI catches them and prints a
  message, never a traceback, for a user's mistake; a broken effect definition is a programming error (plain
  `ValueError`/`TypeError` at import).
- **`bt-image effects` is a short list: looks first, then one line per effect.** The default output is the looks
  (name and description), then each effect's name, summary and presets, then where the details are (`bt-image effects
  NAME`, or `--all`). *Reason:* a person or a model that reads it to find a style must see all of it on a few screens;
  the full parameter lists (about 15 times longer) bury the looks and the presets. *Consequence:* `--all` and
  `--advanced` give the full documentation; `--category` narrows either form; the generated `docs/effects/` is the full
  reference.
- **Picture files are read and written by the Rust core; `imageio` is a thin Python facade over it.** The core
  (`crates/core/src/files.rs`) decodes with the `image` crate (PNG, JPEG, GIF, WebP, TIFF, BMP), reads 32-bit integer and
  float TIFFs with `tiff`, takes a CMYK JPEG's inks from `zune-jpeg`, and converts an embedded RGB, grey or CMYK profile
  to sRGB with `moxcms`, all pure Rust (no C library, so the wheels and the sdist build everywhere). `imageio` adds only
  what a Python caller needs: compositing over white, the array checks of `save`, `to_pil`/`from_pil`, `load_lut`.
  *Reason:* a Rust user of the crate needs the same files without Python, and one reader means the CLI, the library and
  the Rust API see the same pixels. *Consequences, each decided:* `write` makes png, jpg/jpeg and webp, and any other
  extension is a `BackendUnavailableError` naming it; WebP is lossless and `quality` applies to JPEG only (the lossy
  encoder is a C library); a palette TIFF is not read; a map (`load_map`) ignores colour profiles, being data; a JPEG cut
  short is refused (the core walks the segments to the end marker, as Pillow's strict read does); the pixel limit is fixed
  at 178,956,970, Pillow's own, checked from the header before decoding, with the decoder's memory capped at 8 bytes a
  pixel; the embedded sRGB profile is littlecms's with its date fixed (`crates/core/data/srgb.icc`), and is read back as
  sRGB without conversion; PNG is written at the best compression, JPEG at 4:4:4. *Differences from Pillow, measured:* a
  JPEG decodes within 3 to 4 levels of 255 on photos (more on a few-pixel image), a CMYK press profile can differ by up to
  10 levels from littlecms, and a PNG or JPEG write is 2 to 3 times slower for a smaller file. Pillow stays a
  dependency for fonts and the Python resizers until the core has its own.
- **This is a Rust project with a Python interface, as polars is; Python stays only where Rust has no equivalent.** The
  product is the crate and, later, a backend service for creators, so nothing in the hot or service path is Python. Python is
  the binding, numpy interop, Pydantic builders for the Python interface (Rust validates on its own: enums, and `Table::check`
  from the `#[param]` declarations) and the hook for Python effects (`register`). *Roadmap, in order:* (1) effects E1 to E6
  into `crates/core/src/effects/`, each batch with the primitives it shares and the parameter kinds its effects need added to
  the derive; (2) everything that is not an effect, in Rust: fonts for `ascii` and the contact sheet (a pure-Rust glyph
  rasteriser; spike against Pillow's atlas first), the subject and depth finders (ONNX through `ort`, Apple Vision through the
  `objc2` crates; spike the depth model first), `bt-image` as a Rust binary shipped in the wheel (as ruff and uv are), the
  pipeline, manifest, contact sheet and gallery, recipe files and `.cube` tables, the presets and looks embedded in the crate,
  and a version for the derive crate; (3) E7: delete the Python `ops`, scipy, Pillow where nothing needs it and the
  transitional duplicates, re-render the gallery, measure. *Decided:* `ort` for ONNX (speed and model coverage; the cost is
  bundling the ONNX Runtime library in the wheel and the service). *Not yet:* publishing the crate, until the effects are in.

## Finding the subject and depth

- **ONNX is optional, and installing it is one command with a clear error before it.** Apple Vision is the default on macOS
  (nothing to install); elsewhere, or when `segmenter` says `onnx`, the ONNX Runtime library and the model are needed. The
  library finds the runtime by an explicit path (argument or `BT_IMAGE_ONNX_LIB`), then its cache, then the system, then a
  download if allowed; otherwise a `BackendUnavailableError` whose message is the fix (`bt-image install onnx`, or
  `allow_download=True`, or `BT_IMAGE_ALLOW_DOWNLOAD=1`). `bt-image install onnx` fetches runtime and default model together;
  `bt-image models` lists the model catalogue (name, url, sha256, input size, normalisation, output kind), so a model is data
  and swappable; `bt-image doctor` says which backends work and why not. The CLI asks before downloading and shows progress;
  the library and a service never prompt or print. *Reason:* the library must not surprise anyone with network use or output,
  and a person must never have to guess the fix. *Consequence:* every download has a pinned SHA-256 and leaves no partial
  file, and nothing touches the network at import.
- **Two backends behind one function**, `subject_matte(rgb)`: Apple Vision, the default on macOS, and IS-Net on ONNX
  Runtime, the default everywhere else; `segmenter=` or `BT_IMAGE_SEGMENTER` picks either, `--map subject=FILE` (or
  `maps={"subject": ...}`) skips both. *Reason:* the owner's choice: Vision cuts cleaner (the portable model smears at fine
  details), and a Mac needs no download; a look made on a Mac cuts a little differently on Linux. Neither makes the core
  depend on it. *Consequence:* `subject_finder()`'s identity follows the platform, so remembered subjects are found again
  when a project moves between a Mac and Linux. The test suite pins `onnx` so it runs alike on every OS. The portable model (about 180 MB) is downloaded on first use into the
  user's cache, verified against a pinned SHA-256, never at import, never left half-written; `BT_IMAGE_SUBJECT_MODEL` points at a local
  copy for offline use.
- **`bt-image cutout` makes a cut-out file; `transparent = "background"` ends a look.** The command writes the photo at
  its own size with the subject's alpha, after an optional edge clean-up (`--shrink`, `--feather`), and needs no
  recipe; a recipe's transparency cuts out the finished look at the recipe's size. *Reason:* a cut-out file is the
  input of `--map subject=`, of `maps=` and of picture parameters (a logo, the example assets), so it is made once, checked
  and reused, which a look's last stage is not meant for. *Consequence:* both stay; the README says which to use when.
- **Depth is found like the subject.** An effect that needs distance (`parallax`, `ambient_occlusion`) takes it as a
  field (`field = "depth"`, the default): float32 (h, w), 0..1, 1 = nearest; regions made of it (a `band` or `bands` of
  `depth`) read it too.
  The runner is given a depth map or a `Finder` for one (`maps={"depth": ...}`, `finders={"depth": ...}`; neither, when needed, is
  `DepthNotFoundError` naming the step, and the region when a region needs it), and the public API binds the default:
  Depth Anything V2 Small (`onnx-community`, fp32, Apache-2.0) on ONNX Runtime, pinned to a commit and checked against
  a SHA-256, downloaded like the portable subject model (`BT_IMAGE_DEPTH_MODEL` for a local copy); `--map depth=FILE` (or
  `maps={"depth": ...}`) takes a map you already have (brightness, white = near). The model sees the photo bicubic-scaled to a short
  side of 518 px, both sides rounded to multiples of 14, as the original repository feeds it (the export's
  `preprocessor_config.json` scales by whichever side needs the least, which fits a photo under 518 px by its long
  side); its inverse depth (`inverse_depth`) is stretched bilinear to the photo's size, then `depth_map` min-max scales it per photo (a flat answer gives
  zeros). *Reason for both:* the model's answer is relative, so a map scaled per photo is what every effect and region can
  rely on (nearest = 1), but a caller that knows more (several photos of one scene, a known range) cannot scale them alike
  from it; the unscaled answer is public for that, and the caller hands its own scaling in as `maps={"depth": ...}`. The input keeps the photo's aspect ratio, so an extreme one (about 1:10 and beyond) needs a lot of memory;
  there is no cap. Further:
  - the input keeps the name `depth`; the six fields that clashed are renamed: `depth` becomes `depth_range` in `embers`,
    `floating_dust`, `rain`, `snow`, `starfield`, `width` in `torn_edge`;
  - the depth is a field (see "Fields"): `depth-haze` and `depth-grade` are recipes of a `band` and `bands` of it
    (`examples/recipes/`), not effects;
  - framing pads the depth map with 0 (farthest), like the matte;
  - `imageio.load_map(path)` reads a map you already have; no depth found is `DepthNotFoundError` (a
    `BullterrierImageError` and a `RuntimeError`); the gallery runs the real model, like the matte.
  *Reason:* the matte's shape keeps the engine testable without a model and lets a phone's own depth map or a better model
  replace the default without touching an effect. *Consequence:* framing transforms the depth map with the photo; depth is
  found at most once per photo and cached like the matte, under the finder's identity.

## Determinism

- **Same inputs and seed give the same output on the same machine, library versions and models; across platforms equal
  within float tolerance.** Stochastic code takes `seed: int` and uses `numpy.random.default_rng(seed)`, never global
  random state.
  *Reason:* BLAS, SIMD and library builds round differently, so "any machine" cannot be promised byte for byte; caching,
  debugging and reviews need repeatable output on one setup. *Consequence:* byte-identical comparison is the proof for
  refactors; tests assert properties with tolerances, not exact pixels.
- **External models are replaceable assets, not frozen.** A new subject or depth model changes outputs, and that is
  accepted; determinism holds for a given model. Updating one changes its URL, SHA-256 and size in its one module
  (`finding/_isnet.py`, `finding/_depth_anything.py`) and the real-model test skips until the cached file matches the
  new pin; the cached copy with the old digest is replaced on next use, and the finder's identity is the model's
  SHA-256, so the pipeline's cache finds every photo again on the next run. Tests never pin a model's output: logic is
  tested with stand-in models, a test of a real model asserts coarse properties only. *Reason:* better models keep
  appearing, and pulling one in should be cheap.

## Performance

- **Cost grows with the picture, not with the parameters, and memory stays a few pictures deep.** A 12 MP photo is
  the working case, so every Gaussian goes through `ops/blur.gaussian_blur`: scipy's direct filter, byte for byte,
  below a kernel radius of 32 px, and from there an FFT convolution over the same edge extension, within 1e-5 of
  scipy (float32 rounding), whatever the sigma. Code that builds per-pixel geometry works a band of rows at a time.
  *Reason:* a radius at the top of its documented range must still render in seconds, and a direct convolution costs
  in proportion to its kernel (a 30 px sharpen spent 3 s per 12 MP photo in it). *Consequence:* a contract test holds
  every effect at its defaults on a 1 MP photo to 4 s of process CPU time (about five times the slowest effect,
  `dither`, at 0.8 s with the memory tracing the test runs under; a shared CI runner is about four times slower than a
  laptop, and `dither` reached 3.2 s on one, so 3 s failed the build without a regression; the effects themselves aim
  far lower, and the Rust port of `dither` should bring it down) and 12 picture-sized buffers of memory (`tracemalloc`), with a known-bad effect
  for each limit; an effect that needs more is redesigned, or the budget is raised in its own reviewed commit. The FFT is single-threaded, so CPU time stays predictable when photos are
  rendered in parallel.

## The Rust core

- **The numeric work moves to Rust; Python stays the interface.** Two crates in a Cargo workspace beside `src/`:
  `crates/core` (the crate `bullterrier-image`, the PyPI name, for Rust users later; pure Rust, no PyO3,
  `#![forbid(unsafe_code)]`: `ops/` and one file per effect in `effects/`) and `crates/python` (PyO3 bindings that
  build `bullterrier_image._native._core`, built by maturin). Each effect is ported once and its Python body deleted;
  there is no second implementation. *Reason:* measured on a proof of concept, a Rust core
  renders several times faster per thread, needs no pickling between worker processes, starts in milliseconds, and
  can serve a video API and live rendering later. *Consequence:* contributors need rustup (the toolchain file installs
  the rest), and wheels are per platform (abi3: one per platform for every Python from 3.12).
- **One generic boundary: `_core.apply(effect, rgb, params)`.** It borrows the picture (C-contiguous float32
  `(h, w, 3)`) without copying, releases the GIL while the effect runs, and returns a new array. The bindings never
  name an effect, so porting one never changes them. The core finds its effects without a list: its build script
  makes one module per `effects/<name>.rs` and the name table. A wrong name, parameter set, dtype or shape is a
  programming error (`ValueError`): a user's values are validated before they get there. The registry, the one place
  that calls effects, hands every effect the contract's picture, float32 and C-contiguous (a float64 photo is
  converted there once). A field or injected number arrives with the first ported effect that needs one.
- **A picture is borrowed while it renders, so it must not be written meanwhile.** `_core.apply` reads the caller's
  array in place with the GIL released; numpy's borrow flags only stop Rust borrows from clashing, so a Python thread
  that writes into the same array during the call makes the core read memory that is changing (undefined behaviour).
  The contract is "do not modify a picture while it is being rendered", stated on `apply`, here and in the user docs.
  *Reason:* copying the input would add about 25 MB of memory traffic per 1080p frame, as much as a whole light effect
  costs in a live frame budget, and NumPy itself borrows arrays the same way. *Check made (2026-10-29):* the library
  starts no threads, and the two arrays it caches and shares (the dither tile, the glyph atlas) are already read-only.
  The photo and maps a project run holds for a photo's recipes are used one recipe at a time; when batch rendering
  moves to threads (`todo`: parallel batch processing), each worker must own its frame and share maps read-only.
- **A ported effect's parameters are declared once, in Rust.** Its `Params` struct is the declaration: each field has a
  type, a `///` doc comment and `#[param(default, unit, min, max, drivable, advanced, cyclic)]`, and a choice is a
  fieldless enum (`crates/derive`: `#[derive(Params)]`, `#[derive(Choice)]`). The derives write `Default` and a table;
  the declaration rules (a doc, a unit and range holding the default, `cyclic` only for a turn of degrees, `drivable`
  only on `f32`) are compile errors naming the field (an unknown unit is rustc's own error, and a choice's doc naming
  every variant is checked when Python builds the model). The Python class is `class GradeParams(EffectParams,
  core="grade")` with an empty body: `EffectParams` fills the annotations and `param(...)` specs from
  `_core.params("grade")` before pydantic reads them, so strict validation, `fx`, `help()`, the schema, scaling and
  drives are the same code as for a hand-written model. A default or bound keeps the literal as written (a whole number
  stays an `int`, a decimal a `float`), so the schema, docs and `fx.pyi` do not change when an effect moves. An effect
  that works from a map keeps a Python model until the core takes maps (`FieldName`, `Picture`, `Lut`, lists and
  optional colours arrive with the first port that needs one). *Reason:* the Rust struct and the Python model
  repeated every field name and could drift; with one declaration they cannot. The derive's output refers to
  `crate::params::...`, so it compiles only inside the core crate; if the core is ever published, the derive crate
  is published with it. Because Python does not read the fields
  of a ported effect, vulture needs no whitelist. *Typed params (R3b):* `Params`, its fields and the choice enums are
  `pub`, in a public `effects` module, so a Rust user writes `grade::Params { exposure: 1.0, ..Default::default() }` and
  `Step::of` turns it into a step that carries every parameter and no preset. `META` and `render` stay `pub(crate)`.
  `Step::of` takes the parameters through `Declared::values`, which the derive writes (a decimal as the shortest number
  that reads back: `0.2`, not the `f32` widened to `0.20000000298`), and the effect's name through `EffectParams`, which
  `build.rs` implements from the file's name. *The unread-field guard moved:* rustc's dead-code lint no longer reports a
  parameter `render` never reads (the field is `pub`, and `values` reads every field), so `tests/parameters_are_read.rs`
  does: each effect is applied with each parameter changed and one that changes nothing fails; a parameter that matters
  only while another is set (`grade.pivot`, `pattern.radius`) has a row in its `conditional` table, checked both ways. A
  public `Params` has no `#[non_exhaustive]`, so `..Default::default()` works outside; adding a field to one is a
  breaking change, accepted until the crate is published (R5). There is no typed apply (`Effect::apply` takes the parameters
  as JSON); running a step is the runner's (R4a).
  *Validation of a user's values is in Rust since R3* (`Table::check`, next to the recipe): running an effect still
  trusts its parameters, and Python's models keep validating a user's values when a step is made; since R4e a `Recipe` is then held to the
  core's `Recipe::checked` too, so the rules live in two languages while the parity tests (`tests/_native/test_recipe_parity.py`)
  hold them equal.
- **An effect is a Rust value that knows its place in the catalogue (R2).** `Effect` is a trait (name, category, default
  opacity, declared inputs, parameters described, `apply`); the core's registry holds the core's own effects and any
  registered at run time under a lock, and refuses a name that is taken, native or not. A ported effect's file declares
  `pub(crate) const META: Meta` (category, opacity, inputs) beside `Params` and `render`; `build.rs` makes the effect
  from the three and the file's name, so a name is written once and adding an effect still edits nothing else. Python
  reads category and opacity of a ported effect from the core (`@effect(params=...)` refuses them there), the
  vocabulary of categories and inputs comes from the core (`_core.vocabulary`), and a test keeps `CATEGORIES` and
  `INPUTS` equal to it. A Python function can be registered as an `Effect` (`_core.register`, which takes the
  interpreter lock back, since `_core.apply` releases it): the trait fits it. *Reason:* one home for each fact. *Presets in
  Rust came with R3; `Inputs`, a `render` that takes them and the Python registry's mirror came with R4a.*
- **A recipe is a Rust value with one check (R3).** `Recipe`, `Step` and `Each` (crate `recipe`) are plain public
  values, built in code (`Step::new("bloom").param("radius", 12).opacity(0.4)`) or read from TOML or JSON; serde's
  nested form `{effect, params, preset, partition, mask, opacity, blend_mode, name}` is the JSON form, and
  unknown keys are refused by name. A step keeps its parameters as written (a preset's lie under them only in the
  check), an opacity left out stays unset until the runner resolves it, and regions and the framing are opaque
  tables the runner reads. `Recipe::checked(&Presets)` is the one place the rules of a look live (known effects,
  parameters through `Table::check`, drives, partitions, names, opacity, regions that resolve) and `checked_look` adds
  the description a look needs; every consumer calls it first. `from_toml` does what the Python loader does: a step's
  other keys become its parameters, `"none"` a skipped look, and a feature a later format version added is refused in
  an earlier one. *Why presets and looks are not embedded yet:* a crate cannot `include_str!` files outside itself
  and still be published, so the data under `src/bullterrier_image/{presets,looks}` stays package data; Rust users
  pass text (`Presets::add_toml`), and shipping them in the crate is R5's (publish). *Moved:* rendering a step
  (mask, opacity, blend, drives, scaling) came with R4a; `each`, framing and the region kinds are still R4's, hence the
  one-off `fx.bloom(radius=12).apply(photo)` too; a drive is the public `Drive` since R4a; the typed per-effect `Params` entry (`Step::of(grade::Params {..})`) came
  with R3b. *Known losses:* a Rust map kept keys sorted,
  not in file order, until R4c: both readers now keep file order (`preserve_order`) and `Regions` is an ordered list (see
  "Regions and `each` are Rust"); Python keeps its Pydantic model until R4.
- **The runner is Rust (R4a): steps, maps, drives, blend, scaling.** `Photo::render`/`trace` run a checked recipe's
  steps; the core refuses what it does not run yet (`each`, a region other than `given`, a step that takes a
  partition) with an error saying so (every kind runs since R4d). `core/bridge.py` runs a recipe for users; the parity
  tests (`tests/_native/test_runner_parity.py`) hold the core to the definitions. Decisions:
  - *Inputs.* `Effect::apply(picture, params, &Inputs)`, and `render(&Picture, &Params, &Inputs)` for every ported effect
    (unused: `_inputs`). The core hands an effect only what it declares; one it declares but is not offered, or of another
    size than the picture, refuses the run. *Rejected:* two `render` shapes picked per `META` (a build-time switch to save
    three lines).
  - *Drives scale per level.* Each level's value is checked and scaled on its own, as Python's runner does, not the two
    endpoints once: with a whole number rounded or a floor at `min`, scaled endpoints and a scaled middle level differ, and the
    mix would differ from the plain step at that value. A level no pixel is on or next to is not run; the mix needs no clamp
    (a pixel's weights add up to 1).
  - *Whole numbers round half to even* (`round_ties_even`), as Python's `round`. A whole-number density is rounded too
    (Python leaves it a float).
  - *`Layer` has one form: finite and within 0..1 by construction* (`Layer::new` refuses, `Error::Inputs`), so the runner checks
    only its size and no unchecked map exists. A binding that builds one from an array reports the failure as the map's kind
    (`SubjectNotFoundError` for `subject`). *Rejected:* a checked and an unchecked type.
  - *A finder's answer for the whole photo is kept: it is asked once.* The answer is kept behind a lock held across `find`, so
    threads sharing the photo cannot both ask; an error is not kept (the next render asks again). R4c adds one exception: a
    `Subject` region found `per` piece asks the `subject` finder about the crop of each piece, uncached, whatever was handed in
    or kept (see "Regions and `each` are Rust"). `Finder` is `find` only: the identity only the
    pipeline's fingerprint reads is the pipeline's. *Rejected:* releasing the lock before `find` (two asks).
  - *`Photo` borrows the picture* (zero copy from numpy), and `light` is made from the working picture (framed and sized, before
    any step; the pictures as given are only what a finder is asked about), not from a step's output. `background` is 1 - `subject`.
  - *`render` and `trace` are one engine* (`Run::advance` replaces the newest image), so the last stage of a trace is
    `render`'s result byte for byte and `render` keeps only the newest image, by construction. `Stage` is a flat
    `#[non_exhaustive]` enum (`Input`, `Subject`, `Depth`, `Step { number, image }`, from R4b `CutOut`, from R4c `Selection` and `Partition`
    for each region the steps use); labels are the facade's, since they come from Python's `describe()`.
  - *Errors keep their class.* `Error::kind()` (`ErrorKind`) is the one mapping onto the package's exceptions. The extension,
    a leaf that cannot import `errors`, raises `_core.CoreError(ValueError)` with a `kind`; one function (`native.entry`) wraps
    every entry point and raises the exception of the kind (`other` stays the `CoreError`; a kind it lacks is a `KeyError`: the
    two lists disagree). The exception a Python effect or finder raised is boxed in `Error::Foreign` and re-raised unchanged.
    *Rejected:* a global `use_errors({kind: class})` (state set at import, order-dependent).
  - *The registry mirrors Python effects into the core* at registration and `unregister`, in one place, so the two never
    disagree; an effect with a Rust model is skipped (the core has it). The core runs the wrapper, which validates, scales and
    calls the effect, and passes `size_ratio` on only if the effect declares it: a Python effect has no parameter table, so the
    core hands it design values unscaled. A `Picture` or `Lut` parameter cannot be JSON: it travels as `{"$object": n}`, held
    by `registry.holding` for the call (a `ContextVar`; the core calls back on the calling thread). `_core.effects()` therefore
    lists mirrors too, and `effects(declared=True)` the effects whose parameters the core declares, which is what a model
    declared with `core=` needs.
  - *ClipColor deviates from the W3C text.* The `color` mode's ClipColor pulls toward the luminosity `SetLum` set instead of
    recomputing `Lum(C)`: equal in exact arithmetic, but in `f32` a black backdrop under a grey source made 0 / 0 (NaN).
    Found by a property test; pinned by a row of the reference table.
  - *No `Photo::render_all`.* The unknown-map check is per recipe, so a photo carrying the `given` maps of several recipes
    needs a union check; `bridge.render_all` does it (R4e), finds each map once and hands each recipe the maps it knows.
    `as_image` (uint8/uint16, non-finite) stays Python's.
- **Framing, size and the cut-out are Rust (R4b).** A recipe's `frame` and `size` are run by the core. Decisions:
  - *Size derives the ratio.* `Options` has `with_size(u32)`, not a ratio; the runner takes the working picture's longest side
    over 480 (`DESIGN_SIZE`). *Reason:* with `frame` and `size` the working picture is made inside the render, so the ratio
    cannot be an input; one source, and no caller can hand in a ratio that disagrees with the picture (which closed the
    "huge `size_ratio`" item). *Rejected:* an override (tests build a picture of a known longest side instead).
  - *Geometry is planned once and applied per plane.* Crop box, turn size and borders are computed once from the shape and the
    (flipped, turned) subject map into a `Geometry`, then applied to the photo (3 channels) and to each map as it is first
    used (1 channel). Every operation is per channel, so this equals Python's stacking without building the stack.
  - *A picture the core makes is limited to 2^26 pixels* (67,108,864, 8192 x 8192; `ops/length::within_limit`, the one
    definition). The size a `frame` plans (the turned picture, then the finished one: borders and an expanding turn can make
    any size) and the size a `size` asks for are planned from numbers and refused (`RecipeError` naming `` `frame` `` or
    `` `size` ``, the pixels and the limit) before a pixel of either picture is made; a picture no machine can hold would
    otherwise abort on allocation. *Reason:* a render holds up to three pictures of the working size at once (the working
    picture, a step's result, the blend of the two; 12 bytes a pixel each) besides the photo handed in; a framed and resized 6000 x 6000 photo measured 2.2 GB
    (45 bytes a pixel), so the cap keeps a render near 3 GB. The earlier cap, twice Pillow's decompression-bomb limit
    (178,956,970 pixels), would have allowed about 8 GB. A policy the owner may change. It does not limit a photo handed in,
    nor reading a file (Pillow's own limit, `imageio`): a larger photo renders as it is but cannot be framed or resized to
    a picture over the cap.
  - *A resized side is rounded exactly.* The other side is `side * longest / long` in integer arithmetic, halves to the even
    pixel, so a tie is a tie (85 x 455 / 170 = 227.5 is 228; the float product 227.49999999999997 gave 227). It can differ by one
    pixel from Python's float `round` on a float-inexact tie only.
  - *Resample and turn follow Pillow, measured.* Resize (Lanczos shrinking, bicubic growing, support widened by the shrink) is
    within 1.19e-7 of Pillow, pinned at 2e-7. A turn is Pillow's transform: nearest exact (16.16 fixed point as Pillow), bilinear
    1.19e-7, bicubic 2.98e-7 (Keys' kernel at a = -1 for the transform, -0.5 for resize), pinned at 0, 2e-7, 4e-7. The cut-out
    alpha is within 5.96e-8 of Python's, pinned at 2e-7.
  - *`Image` has an optional alpha; effects never see it.* `channels()` is 3 or 4 and `into_values` gives that many per pixel
    (straight alpha); only the cut-out after the last step makes one, so an effect's `Picture` is RGB by type. A trace ends with
    `Stage::CutOut`, exactly what render returns. `background` uses the framed working subject as the alpha (the render needs the
    subject, as a crop around it does); `circle` and `rounded` come from the working picture's shape (`corner_radius` of the
    shorter side), a float32 pixel-centre signed distance with a one-pixel soft edge.
  - *Memory is measured.* `crates/core/tests/memory.rs` counts the heap (dev-dependency `peak_alloc`): render of a 12-step chain
    holds 2 pictures, trace 14.
- **Regions and `each` are Rust (R4c).** The parity tests hold the core to numpy definitions. Decisions:
  - *Order is data.* `serde_json` and `toml` keep file order (`preserve_order`; `Zlib` is allowed in `deny.toml` for `foldhash`,
    which serde_json then hashes with); `Regions` is an ordered list. *Reason:* the order regions are declared in is the order a
    trace shows them and the order a refusal names the first offender, for a TOML file and a Python dict alike.
  - *Regions are typed and a kind is required.* `Region` is an enum of 13 documented structs, not a table; ranges are hand-written
    in `Region::check` (the `Params` macro knows `f32`, `i32`, `bool`, `Color` and choices only) and held to Python's by a table.
    A region without `kind` fitted every Python kind and `Given()` dumped `{}`: the canonical form names the kind. A grid's
    shares are written as the list (`3` reads as `[1, 1, 1]`).
  - *References are walked once.* `Recipe::checked` types every place a region is named (a selection for `mask`, `by`, fields,
    `fit`, `of`; a partition for `each`, `Pick.of`, `Subject.per`), refuses cycles (iteratively: a 5,000-link chain overflows no
    stack) and names the closest region for a misspelt one. The same walk gives `Recipe::needs` (one struct, instead of Python's
    `maps_needed`, `needs_finder` and `derived_maps`) and when each region is last used. A preset can set a field parameter, so
    `needs` takes the presets.
  - *Maps up front, regions on demand, dropped after their last use.* A missing map or an unfit finder answer fails before step 1;
    any other region is made when its step comes and released after the last step that names it or the first step that needs a
    region made from it (that one is made and kept, so what it was made from is not needed for it again), so a render holds
    the regions the steps ahead need, not all it made (measured in `tests/memory.rs`). A recipe's work before its first pixel
    is linear in its regions and steps (`tests/it/scale.rs`): lookups by name are indexed, and "made from" and "needed until" are
    worked out once per region. A region the core cannot make
    yet, and a `Subject` found per piece with no subject finder, are refused before step 1, which Python did at first use.
  - *Precision follows the data.* `Band` and `Combine` work in `f32` with the parameters narrowed to `f32` first, as NumPy does
    with float32 maps (a depth of `f32(0.7)` is inside a band from 0.7; a double comparison would drop it); `Bands` and `Grid`
    work in `f64`; gradients are computed in `f64` and narrowed once (within 1e-7 of Python's `f32`).
  - *A subject found per piece asks the finder uncached*, about the crop of each piece grown by `context`; the answer for the
    whole photo is no stand-in. "Found nothing" is the finder's not-found error: the piece adds nothing. The facade turns the
    package's not-found errors into a `CoreError` of that kind, so a finder's not-found error now comes back rebuilt (same class and
    text), not as the same object; every other exception still comes back as it was.
  - *`each` holds one look at a time.* A look that has a piece is run once over the whole picture and added as
    `weight x (look - before)`; the weights of all looks and of the pieces left as they were add up to 1. The Gaussian
    (`feather` design px times the ratio) is three box blurs of matched variance (`ops/blur`, Kovesi 2010; the edge repeated
    beyond the picture, made once before the boxes), so the cost does not depend on the feather: a 3 x 3 grid with feather
    24 took 0.32 s at 1024 x 1024 and 9.6 s at 3000 x 3000 with scipy's direct kernel, and takes 0.02 s and 0.16 s. *Cost:*
    the weights differ from the Gaussian's by at most 0.035 from a sigma of 1.5 working px (0.12 at 1; under 0.6 the
    blur is none), so a pixel differs from scipy's by that share of (look - photo). *Rejected:* the direct kernel (cost
    grows with the feather, so a 3000 px photo took seconds); an IIR filter (more code, edge conditions to match).
  - *Region sizes are bounded by `Recipe::checked`.* A grid has at most 10,000 pieces in all (100 x 100; a list of shares
    alone allows 1000), a `Pick` lists at most 10,000 pieces and a `Combine` joins at most 1000 regions: a `Subject` found
    `per` piece asks the finder once for each, an `each` look runs once, so 1000 x 1000 pieces was a million finder calls.
    A `Pick` names a piece by `id` only of an `Svg` region, so the render cannot fail at the step that uses it.
  - *Random streams are new.* `assign = "random"` draws with xoshiro256++ seeded through SplitMix64 (`ops/random`; Lemire's
    unbiased bounded draw): the same seed gives the same picture, not the picture NumPy's generator gave. Published vectors
    pin the generators; the assignment is tested by properties.
  - *A trace shows the regions the steps use*, after the maps and before the steps, in declaration order, made before step 1;
    it fails where the render fails and ends in the picture the render makes.
- **During the migration an `ops` function may exist twice.** A Python `ops` function still used by unported effects
  stays while its Rust twin serves ported ones (`srgb_to_linear`, `luminance`); the Python copy is deleted with its
  last Python user (`linear_to_srgb` went with `grade`). *Reason:* porting by payoff, a batch at a time, without
  breaking the effects not yet ported.
- **The Rust gates match the Python ones**, each in `poe check` with a known-bad test (`tests/tools/test_rust_gates.py`):
  rustfmt; clippy (pedantic plus no unwrap, expect, panic or printing; dead code, missing docs and unreachable `pub`
  as errors); rustdoc; cargo-shear; cargo-deny (bans, licences, sources; advisories in `poe audit`); stubtest of
  `_native/_core.pyi`; cargo test (proptest for `ops`). The hidden-tunable scan, one config home and effect independence cover
  the Rust sources. Warnings are errors only in the gate, so a newer compiler never breaks a user's build. No
  `.cargo/config.toml` and no `target-cpu=native`: a wheel must run on every CPU of its platform.
- **Platforms: wheels for five, the sdist for Windows on Arm, none for musl.** One abi3 wheel each (every Python
  from 3.12) for Linux x86_64 and aarch64 with glibc 2.28 or newer (manylinux_2_28), macOS arm64 and x86_64, and
  Windows x86_64, built by maturin-action on GitHub's runners (Linux natively in the manylinux container of each
  architecture; macOS x86_64 cross-compiled). Windows on Arm installs from the sdist, which needs Rust (rustup; the
  sdist carries `rust-toolchain.toml`, so it builds with the compiler CI tests). Not supported: musl Linux (Alpine)
  and glibc older than 2.28. Every wheel is installed from the artifact alone, and the sdist built, on its own
  platform, each rendering a picture and importing onnxruntime, before anything is published. *Reason:* onnxruntime,
  a dependency, sets the floor: its wheels need glibc 2.28 (manylinux2014, glibc 2.17, would promise an install that
  fails; it is also end-of-life), it publishes neither a musl wheel nor an sdist, and its newest releases have no
  macOS x86_64 wheel, so on Intel Macs pip picks an older onnxruntime (1.23.2 when this was written). A Windows arm64
  wheel waits for someone to ask. A user on a common platform never needs a compiler, and nothing published goes
  untried.
- **The smallest supported thread stack is 256 KB.** The walks over regions are iterative: `tests/it/deep.rs` pins a chain of
  2,000 regions and a diamond of 2,000 levels, read, checked, rendered, traced and dropped on a 128 KB thread. (Measured
  once, not guaranteed by a test: a chain of 50,000 and a diamond of 20,000 on 64 KB, in a debug build, 355 s.) The depth of a value inside a
  step is not ours: serde_json (128 levels in all) and toml (80) recurse, and the Python dict reaches the core through
  `to_json`, which refuses more than 128 levels (the readers' own limit) before anything walks it. The deepest such value
  needs 160 KB in the shipped build and overflows at 128 KB. *Reason:* glibc, macOS and Windows threads have 512 KB or
  more and musl (128 KB) is unsupported; `stacker` around the readers fixed the read but not the later `check`, which
  still overflowed at 128 KB. *Consequence:* `tests/_native/test__core.py` reads and checks the deepest
  JSON and TOML recipe and the deepest dict on a 256 KB thread.
- **Printing a picture or a map shows its shape, never its pixels** (`Picture`, `Image`, `Layer`, `Labels` write their own
  `Debug`). *Reason:* a derived `Debug` printed every pixel (15 MB for a 4-stage trace of a 300 x 400 photo) into any
  assertion message, `unwrap` or `dbg!`; at 1080p one value is about 40 MB. *Consequence:* a test keeps its pictures small and never
  `assert_eq!`s a large buffer.

- **Threads are a run option, default 1, and never change the result.** `Options::with_threads` (Python `threads=`),
  not an effect parameter: a thread count is no property of a look, and a look must give the same bytes on any machine.
  *Default 1:* the usual use of several cores is rendering many pictures at once (a video renders frames side by side),
  where each render taking every core only competes; one picture asks for 0 (every core). `Photo`, `Recipe`, `Presets`,
  `Options` and `Image` are `Send + Sync`, so frame-level parallelism needs no API. *An installed pool, not a parameter on
  `Effect::apply`:* the render installs one rayon pool and a kernel calls `par::rows`, so no signature of the Python
  effects or the ported ones changes and an effect joins when it is ported, only if its output rows depend on input rows
  alone (per-pixel work, separable passes); a global reduction or a sequential scan stays single-threaded, and a
  reduction chunks by the data so the sum is the same at any count. *The render stays on the calling thread:*
  `pool.install(render)` would run Python effects on a pool thread, where the `ContextVar` behind picture parameters is
  empty, and `use_current_thread` leaks the pool's registry and fails a second render on the thread; the bands alone run
  on the pool (an install per `par::rows` call, 4 to 16 us). *A pool per render, no cache:* creating, using and dropping
  10 threads took 0.1 ms (M4), under 1 percent of a 16 ms frame. *No feature flag:* rayon is a plain dependency (MIT or
  Apache-2.0, small). *No GPU yet:* the CPU benchmark below is the baseline to beat, and the effects are still mostly
  numpy; revisit after the E-batches. *Numbers* (M4, 1080p, 12-step all-native look, 543 ms at 1 thread): 294 ms at 2,
  204 at 4, 145 at 10 (3.7x); laying a step is memory bound (2.4x at 10), grade 4.3x. In the Python-bodied recipes of
  today, effects are about 70 percent of the time and stay single-threaded, so the gain is small until they are ported;
  whole renders from Python threads side by side scale 2.8x at 4 and 3.6x at 10.

## Effects

- **`edges` 'dog' draws the difference of Gaussians itself, not its gradient** (XDoG, Winnemöller et al.). *Reason:*
  the gradient of the difference doubles every line and shows texture as busy rings; the response is positive on the
  dark side of an edge and on dark strokes, which is one ink line, the sketch look, and unlike Sobel and Canny.
  *Consequence:* the line sits about one radius beside the edge, on its dark side; 'dog' still uses `threshold` and
  `softness` like the other methods.

## Fixing defects

- **A fix removes the cause; it does not add a defence.** The bug is reproduced at the public interface, fixed where the invalid state is created (a stricter parameter model, one source of truth, a removed path),
  and the same class is searched for elsewhere. Inside the library code trusts its contracts and fails loudly; inputs are validated once at the boundary with a `BullterrierImageError`. *Reason:* agents and automated loops
  patch where a failure shows (fallbacks, defaults, swallowed exceptions, extra checks), which "makes the system less understandable while appearing more robust" and keeps the cause (Armin Ronacher,
  "The Coming Loop", 2026); the repo's own instances were library options hidden behind `# constant:` and two copies of the same edge table. *Consequence:* the rules live in `CLAUDE.md` ("Fixing defects"),
  duplicated knowledge (code or tests) is merged, and what a tool can see becomes a gate (blind `except`, literal library options, the marker rule).

- **Python is a facade over the core (R4e).** `recipe.render`, `trace` and `render_all` call `core/bridge.py`, which runs the
  core; the Python runner, the region and framing engine, `ops/frame`, `ops/raster`, `core/outlines` and `svgelements` are
  deleted. Decisions:
  - *The models stay Pydantic builders.* `Recipe`, `Step`, the region kinds and `Framing` keep their field declarations and
    range checks (they document, type and error-word the API; moving the ranges would change wording public tests pin), and
    `steps.recipe_dict` writes them as the dict the core reads. The one rule that left Python is a path's syntax
    (`Path.d`, `Curve.path`): only the core reads path data, so `Recipe(...)` calls `_core.check_recipe` and a bad path is
    refused when the recipe is made, not when the region is. *Cost:* every `Recipe` (and so every `with_values`) is checked
    twice, by its models and by the core. *Revisit when* the models are generated from the core's declarations (R5).
  - *`Recipe.parameters` and `with_values` stay a Python walk* over the steps, regions and frame (addresses are file-order
    data, but the walk is 150 lines against a Rust twin plus a binding).
  - *`render_all` finds, the core renders.* Each map the recipes need is handed in or found once by `find_map`, then each
    recipe is rendered with the maps it knows; a single `render` takes the same path. The core is only asked for a
    finder for the subject of each piece of a `Subject` region with `per`.
  - *The facade owns what the core cannot:* `size` a whole number (a `bool` or a float is not), the names in `maps` and
    `finders` against all the recipes, the wording of a missing map (`maps=`/`finders=`), a Python effect's result of
    another shape (`RuntimeError`, in the registry's wrapper), the labels of a trace's stages and the drawing of numbered
    pieces (`ops/pieces`), and `svg_bytes` (the pipeline's digest of a file). `Stage` and `Finder` keep their public names.
  - *`as_image` returns an aligned array,* since the extension borrows the buffer and a misaligned one (a view onto a
    received buffer) cannot be a Rust slice.

## Regions

- **A recipe declares named regions; a region is a partition or a selection.** `[region.<name>]` with a `kind`. A
  *partition* gives every pixel to exactly one piece (pieces numbered 0..n-1 in piece order: reading order, `bands`
  highest first; see "Piece numbers come from one rule"); a *selection* says how much each pixel is chosen (0..1). *Reason:* grids, cuts, shards, curves,
  SVG shapes and the subject are all "which pixels", so one idea serves them all and effects never learn about regions.
- **Steps use regions by name, with one word per idea.** `mask` is where a step applies (on every step, any selection;
  `subject` and `background` are built in); `each` + `looks` gives each piece of a partition its own look; `partition`
  is the partition an edge-aware effect draws. A hard-edged selection is also a two-piece partition. *Reason:* `mask`
  already means "where".
- **A look is a single inline step, or None (the piece is left as it is; a file writes `"none"`).** Named multi-step
  looks wait for a real need. Each distinct look is rendered once over the whole picture and blended through its pieces' weights, which sum
  to 1 and are then multiplied by `mask`: the cost is the number of looks, not pieces.
- **Positions are shares of the frame, lengths are design px.** The frame is the picture, or the box of the selection
  named by `fit`. A length (a torn `Edge`'s `amount` and `scale`, an `Each`'s `feather`) is `px` written for a picture
  whose long side is 480 and scaled by the runner like an effect's, so the same look comes out at any resolution, torn
  edges too (their noise is laid on the frame, not on the pixel grid). An extent relative to something else stays a
  share with its reference named: `radial_gradient.radius` (the shorter side, as `swirl` and `pinch_bulge`),
  `Shards.jitter` (the lattice spacing), `Band.feather` (the field's range), `Framing` `margin` (the subject's box),
  `pad_*` and `corner_radius` (the shorter side).
- **A field is a selection read for its values; `band`, `bands` and `combine` cut and join fields, `light` and two
  gradients make them.** A selection is already a float map 0..1, so no second concept is declared: a field is whatever
  selection a region's `of` names, built in (`depth`, `light`, `subject`) or the recipe's own. `band` (a selection)
  chooses where a field lies from `low` to `high`, fading linearly over `feather`; `bands` (a partition) cuts the field
  into `count` equal ranges, highest first; `combine` multiplies, takes the least or the greatest of several;
  `linear_gradient` and `radial_gradient` rise evenly with position (shares of the picture and of its shorter side, so a
  look has one shape at every resolution). `light` is the Rec. 709 brightness of the picture the steps work on, made by
  the runner after the framing and scaling, once per render, so it is not found, handed in or cached, and the first step
  does not change it. *Reason:* the depth selection and depth bands were already a band and bands of one field; making
  `of` a name instead of a built-in gives the same thing for brightness, a drawing's silhouette or a gradient with no
  new mechanism, and video needs a recipe to cut any map it hands in. *Consequence:* `Depth` and `DepthBands` (`kind =
  "depth"`, `"depth_bands"`) are `band` and `bands` with `of = "depth"` (`far` is `low`, `near` is `high`; the same
  values give the same pixels), recipe format 3 adds the new kinds and a file of format 2 that uses them, or still uses
  the old ones, is refused saying what to write; `light` is a built-in region name, so none may be declared with it.
  Left out until a recipe needs them: edges, saturation and colour distance as fields, `1 - x`, and a feathered gradient
  shape.
- **Every number has an address: `Recipe.parameters()` lists them, `Recipe.with_values()` sets them.** An address is
  `<step>.<number>`, `<step>.<number>.start|end|levels` for a driven number, `<step>.opacity`, `each.looks[0].<number>`
  for a look, `region.<name>.<number>` and `frame.<number>`; a step is its optional `name`, else its effect (`each` for an
  `Each`) when the recipe has one such step, else the effect and its number from 1 (`bloom#2`, which `sweep` already
  wrote). `parameters()` gives each a `Parameter` (value, unit, minimum, maximum, doc, whole) from the number's own
  declaration; `with_values` returns a copy, re-validating only what changed. *Reason:* the video package maps a control
  signal onto a number within its limits, every frame, and must learn the numbers, units and ranges from image, not
  hold a copy of them; before this only `sweep` could name a value, and only a top-level step's parameter. One grammar
  serves `sweep`, a driver and an editor. *Consequence:* `name` is a step key (reserved, so no effect may have a parameter
  called it) and a name is a word that cannot start another address (`region`, `frame`, `each`), is unique in the recipe
  and is not an effect the recipe uses; a driven number is addressed by its ends, never by itself; a value out of range
  fails at once naming the address; `sweep` takes any address; looks count from 0 like pieces. Not addressable: a
  colour, a choice, a list, a text, a picture, `size`, the names of regions, and the fields of a table inside a region (a
  torn `edge`, a cut's `lines`); a look takes no name, so it is reached by its place.
- **A field varies a parameter: `Drive(by, start, end, levels)`, rendered at levels and mixed.** A number an effect marks
  drivable (the format's `drivable`, `param(..., drivable=True)`, floats only) takes a `Drive` instead of a value: `start` where the field `by` is 0,
  `end` where it is 1. Effects do not change: the runner renders the effect at `levels` evenly spaced values and each pixel
  mixes the two renders either side of its field value (`ops/levels`, the rule `graded_blur` already used), a level no pixel
  needs is not rendered, and the step's `mask`, `opacity` and `blend_mode` apply after as for any step. *Reason:* a mask only
  cross-fades the original and the result, so it cannot make blur, grain or exposure itself grow with distance or
  brightness; a per-pixel parameter inside every effect would change every effect and its tests, and the mix of renders is
  exact at every level and a hand-checkable mix between. The cost is `levels` times the effect (2 to 16, default 5), so
  it is a named, documented key and not a hidden constant. *Consequence:* only parameters whose result changes smoothly
  with the value are opted in (24 so far; each is tested to equal the plain effect at a constant field and the hand mix
  of two renders), whole numbers, choices, seeds and colours never are; `start` and `end` and every level between are
  validated by the effect's own model when the step is made, so a driven value cannot leave its range; one step varies
  by one field and one `levels`; the field is a `by` reference like `mask`, so the recipe checks it exists and is a
  selection, and the maps it is made from are found like any other; a recipe file writes `{ by, start, end }`, needs
  `format = 3`, and Python writes `Drive(by=..., start=..., end=...)` with the same keys (no `by()` helper: a second way
  to write the same thing). Left for later: a parameter an effect receives as a per-pixel map (exact and one render, for
  the effects where it matters, once the video benchmark says where), and driving a colour.
- **Region kinds are a closed set in one module, with presets as data.** Not plugins: nobody else writes kinds, and a
  shared plugin registry would be speculative. Python writes regions with the kinds' own Pydantic models. *Revisit
  when* another package wants to add kinds.
- **Seams are solid lines or bands; no faked materials.** A paper or metal look may come only from a real texture
  picture. *Reason:* generated paper fringes read as 2000s camera presets.
- **Apple Vision is the default subject finder on macOS; the portable model elsewhere.** *Reason:* the owner prefers
  the better cut over the same cut on every platform (2026-10-27, reversing "the portable model everywhere").
- **Piece numbers come from one rule with no constants.** The first pixel in the raster scan decides the order, for
  every kind, so a looks list is predictable and a partition's numbers need no per-kind code. A picture too small to
  give every piece a pixel has fewer pieces, still numbered 0..n-1, and a `Pick` of a number it has no piece for
  selects nothing, so a recipe also works on a thumbnail. One documented exception: `Bands`
  numbers its ranges highest first, because "piece 0 is the nearest, the brightest" is the one rule a look over a field can
  state without knowing the photo.
- **`Shards.focus` is an exponent offset.** A seed at a share r of the way to the farthest corner moves to r^(1+focus)
  of it; 0 spreads the seeds evenly. *Reason:* the prototype's `(r/R)**(1 + 3*focus)` had a hidden 3 (rule 5); the
  `glass` preset is `focus = 2.5`.
- **`Edge.octaves` and `Edge.persistence` are advanced fields.** They shape a torn edge (the layers of noise and how
  strong each finer one is), so they are named, documented parameters, not constants; lacunarity 2 is the standard
  octave doubling, cited. A torn edge keeps its size and character at any resolution, not its exact ripples: the noise
  lattice has a number of points that depends on the picture's size.
- **The step keys as built.** `mask` takes any selection name, checked when the recipe is made (a step knows no
  regions; the recipe that holds it does, and `fx` declares none, so by itself it knows the built-in two). `partition`
  names the region an effect with the injected input `partition` draws (int32 piece numbers; a selection is split into
  what it chooses and what it does not). `each` and `looks` make an `Each` step, a step type of its own: it has no
  effect or parameters, only `each`, `looks`, `assign`, `seed`, `name` and `feather` (in px, so the same look
  comes out at any resolution; how softly the looks blend across a border, default 0.5 to smooth the pixels'
  stair-steps) beside the layer keys `mask`, `opacity` and `blend_mode`, which
  apply to the blended looks. `partition`, `each` and `looks` are reserved like the other step keys, so no effect
  parameter takes them (`name` too, see "Every number has an address"); `seed` and `feather` are not, as an `Each` has
  no effect parameters to clash with. A look is a step with its own `opacity` (its effect's default unless given) and
  `blend_mode`.
- **Regions are made for the working picture, once per render, and only as steps ask.** After framing and scaling, so a
  piece number means the same on the picture that is rendered. A recipe needs the subject only if a region *in use*
  (named by a step, or what such a region is laid over or picked from) is made from it, so declaring a region that
  nothing uses costs nothing. Recipe format 2 adds regions and the new keys; a file of format 1 may not use them, so its
  meaning never changes.
- **Grid first is a `Subject` kind with `per`: one finder call per piece, the finds united.** The finder sees the
  piece's box grown by `context` (0.25 of the piece's size on each side, clipped to the picture) and only the piece's own
  pixels count, so a subject cut by a border is seen whole yet belongs to the piece it lies in. The finder is the injected
  one (portable IS-Net by default), asked about crops of the *working* picture, so a `subject` map of the whole photo never
  stands in for it. Without `per` it is the built-in `subject`. *Reason:* a search of the whole picture misses small
  things beside big ones (the calf, the fox on its rock, the peak), and the model, made to find a subject in every crop,
  invents faint ones in plain pieces.
- **One filter against invented subjects; two limits were measured and left out.** `threshold` (0.9) keeps only pixels the
  finder is surer of than that. On eight photos (study
  grids of 3 cells on the short side, real model, `.superpowers/reports/G-grid-first.md`) the invented blobs the study
  showed peak at 0.83 or less, while the sure finds (calf, fox, cormorants, the elephants) are at 0.9 and up; legit
  subjects fill a whole piece up to 0.98 and touch the view's edge up to 100%, so a limit on the share of the view's
  border a find touches would drop real subjects before invented ones and is not offered. A coverage limit (drop a find
  that fills most of its piece) was tried and not needed: no photo gave a confident invented find; add it back when
  one does. *Cost:* the finder is unsure of
  big plain parts of a real subject seen in a crop (the white head of the eagle, a dog's chest), so those are dropped
  too; lower `threshold` to keep them with the invented ones. *Revisit when* a model is calibrated better.
- **The pipeline remembers each crop by what it holds.** A find in a crop is stored by the finder, the photo and the
  crop's pixels (and its size), so whatever recipe, framing or size made the crop, a repeat run asks the model nothing;
  "nothing found" is remembered too. Every remembered map, of a crop or of the whole photo, is kept in 16 bits and the
  first run uses the kept value, so the first and later runs give the same pixels.
- **Cuts, paths and SVG shapes are drawn in the frame's units and filled by one scanline fill.** A path's coordinates
  (`Cuts` curves, `Path.d`) are shares of the frame, and the unit square is stretched to it, so an arc is a circle only
  in a square frame; a straight cut's `angle` is measured on the picture's pixels, clockwise from the right like every direction.
  `ops/raster` fills outlines by the nonzero or even-odd rule a few rows at a time (no supersampled picture, so 4K costs
  about 1 s torn and 0.1 s straight): exactly across a pixel, `Path.samples` scanlines down it (advanced, default 4).
  A cut curve is closed along the frame's border and its inside read as one side; an end not on the border is carried
  straight on to the nearest. Every kind reads its drawing at the noise-shifted coordinates, so `edge = torn` works on
  all of them. *Reason:* one rule for position and one for borders; a library rasteriser would add a dependency for a
  fill that is a page of numpy.
- **SVG files were read by `svgelements` (MIT) behind a guard (the Python engine, deleted in R4e with the dependency; the core's reader is below).** We never wrote an SVG reader;
  the library handled shapes, transforms, `use`, arcs and `viewBox`. A shape is a piece: colours and strokes are
  ignored, `display="none"` shapes are skipped, the `fill-rule` is kept, later shapes lie on top, and the area outside
  every shape is one more piece, numbered like any by its first pixel (0 unless the top left pixel is inside a shape). `Pick` takes a number
  or an `id` (every piece of every shape with it; an id the file lacks is an error, a shape too small or too covered to
  have a pixel selects nothing, as a number does). The file is placed by its `viewBox` and `preserveAspectRatio`
  (library rule, default fitted inside and centred), and without a `viewBox` its `width` and `height`; a `transform` on
  the root element is refused. *Reason:* the owner's files are unknown, so the common SVG subset is read whole and the
  rest draws nothing.
- **An SVG file is untrusted input; each limit is a named field or a cited constant, never silent.** XML entity
  declarations are refused (a DOCTYPE with an internal subset, found by expat before the library sees the file:
  billion laughs and external entities cannot start, whatever the encoding; a DOCTYPE that only names a public DTD is
  harmless and never fetched). Nothing is run or fetched: scripts, links, `href` and external images are ignored.
  `Svg.max_kilobytes` (default 1024) and `Svg.max_shapes` (default 1000) are advanced fields; shapes are counted before
  any `use` is expanded (a use of a use of a use would otherwise multiply, as entities do), a `use` that contains
  itself is refused, and nesting deeper than 64 levels (the reader recurses once per level) is refused, each with a
  `RecipeError` naming the limit and its value. A curve that would need more than 1024 chords or a point farther than
  10^7 px is refused (hostile coordinates, never a drawing). *Revisit when* an owner's real file needs more.
- **The core reads SVG with `roxmltree` and `svgtypes` (8 crates, MIT or Apache-2.0, no new licence), not `usvg`.** Measured
  on 9 inputs, `usvg` keeps a bad `d` as the prefix before the error (Python refuses it), invents a 5 x 5 size for a file with
  neither `viewBox` nor `width`/`height` (Python refuses it), and bakes the `viewBox` into its tree, which we would undo; it
  is 22 crates and adds BSD-3-Clause (`tiny-skia-path`), and we would still need `roxmltree` for the entity scan, the
  limits and the root refusals. The reader is `ops/svg` (about 1,400 lines with its tests).
- **Placement is the SVG specification's; four things `svgelements` did differently are not copied.** A nested `<svg x y
  width height viewBox>` is placed by its own viewBox (`svgelements` applied the offset twice); a `symbol` used with
  `width`/`height` is scaled to them (ignored); a `symbol` nothing uses is not drawn (drawn in place); a `transform` that
  is no number is refused (ignored). Also, on purpose: UTF-8 files only (a UTF-16 file is refused, not decoded: no encoding
  table to trust), lengths are numbers of px (`10cm` and `100%` are refused, not converted), and a rounded `rect`, a
  circle and an ellipse go through the path parser as arcs, one flattening for all.
- **Shapes are painted back to front into one raster (the painter's algorithm), with budgets, not a z-order sweep.** A sweep
  is output-sensitive but three times the code for the case a budget refuses: crossings of edges with scanlines (2^28, all
  shapes together), painted pixels (2^30, each shape's box clipped to the picture) and corners (2^20) are counted before
  anything is painted, and a refusal names the limit and the count. The real constants are reviewed, not run; the tests
  run the same code at a small `Limits`. Shapes (`max_shapes`) and visited elements (2^22; a group of uses of groups
  repeats elements though it adds no shape) are counted before any `use` is expanded, by memoised sizes,
  so a small file cannot become 10^10 shapes or visits; a `use` that contains itself and nesting past 64 levels (a `use` is the parent
  of what it uses) are refused.
- **A torn edge is hashed-lattice value noise sampled by Catmull-Rom, not a port of Python's spline.** Python fits a
  `RectBivariateSpline` (a global banded solve, a lattice array of points^2 for each octave: 30 GB at octave 7 on a big
  picture). The core hashes the lattice value from (seed, axis, octave, row, column), so nothing grows with octaves or
  scale and the same tear is laid on the frame at any size; each axis is standardised to mean 0 and sd 1, so `amount` stays
  the standard deviation of the shift. The numbers differ from Python's, which no one compares (seeded streams differ by
  design).
- **Nearest site: the lower index wins a tie.** `cKDTree` only promised the same answer every time. `ops/nearest` bins the
  sites in a grid and scans rings round the query's clamped cell; the ring stop is `best <= r * cell`. A honeycomb finer than
  2^20 points is capped and the rest of the sites drawn at random in the chosen pixels (Python allocated
  `count x box / chosen` points: unbounded).
- **Pieces of cuts are numbered by a sparse map.** One bit for each of up to 32 cuts is up to 2^32 kinds; `renumber_sparse`
  remembers the kinds that occur, not all that could.
- **Shard presets are embedded from the one data file and expanded when a region is read.** `recipe/region/presets.rs`
  includes `crates/core/data/regions/shards.toml` (one copy, inside the crate so it can be published, still data);
  given fields win over the preset's, and a given `edge` replaces the preset's whole `edge`. Python reads names,
  sentences and values through `region_presets` (`_core.region_presets`), never the file; `tests/tools/test_rust_embeds.py`
  fails on any `include_*!` that leaves its crate.
- **Every user value that drives allocation or work has a maximum, and a test at it and one past it** (`crates/core/tests/it/limits.rs`):
  `Shards.count` 500, `Cuts.lines` 32, `Path.samples` 16, `Edge.octaves` 8, `Edge.scale` 480, `Edge.amount` 48, `Grid` shares
  1000 and 10000 pieces, `Bands.count` 100, a `Pick` 10000 pieces, `Svg.max_shapes` 100000, `Svg.max_kilobytes` 100000,
  nodes 2^21, visited elements 2^22, nesting 64, chords 1024, coordinates 10^7 px, corners 2^20, crossings 2^28, painted pixels 2^30. The new regions
  hold a bend (8 bytes a pixel) and a raster (4) beside the picture, at any size of photo: the pixel limit applies to `size`
  only, and no cap is added now.
- The plan and the deferred list of regions (each item with the need that would justify it) are in
  `.superpowers/plans/regions.md`.

## Testing and gates

- **Properties, not golden images.** Tests assert the property that defines a behaviour (a blur stays in range, a sort
  keeps each column's pixels) against an independent oracle. *Reason:* a golden image written with the code proves
  nothing and churns binary diffs. *Consequence:* no goldens, no change-detector tests of configuration.
- **Behaviour through the public interface.** No private imports, no mocks of our own code, no call-count assertions;
  only true external boundaries (the model, the network, the clock) are substituted, preferably by injection.
  *Reason:* a test must survive any behaviour-preserving refactor. *Consequence:* hard-to-test code gets a better
  interface, not a reach-in.
- **The gates are the review.** `uv run poe check` runs format, lint, dead code, dependencies, build, types and tests;
  each gate has a known-bad test proving it can fail. *Reason:* reviews stay light and consistent. *Consequence:* a
  defect a review finds twice becomes a gate.
- **Tests run under a memory cap, not only a timeout.** `tools/run_capped.py` sums the RSS of the test command's process
  group and kills the group, exit 99, past `[tool.run-capped] cap-mb`; the poe test tasks use it, so `poe check` and CI have it.
  *Reason:* a runaway (43, 152 and 57 GB; the last was a mutant) freezes the machine in seconds, far before any timeout
  fires, and macOS enforces no `RLIMIT_AS`. The tool is polling and `ps`, so it works the same on macOS and Linux; Windows
  runs uncapped (no process groups or `ps`). *Consequence:* a mutation run counts exit 99 (cap) and 98 (`--deadline`) as
  killed. A process that leaves the group is not seen.
- **The Rust integration tests are one binary.** `crates/core/tests/it/main.rs` declares a module per area; only
  `memory.rs` (a counting `#[global_allocator]`) and `parameters_are_read.rs` (it walks every effect in the registry,
  which other tests fill with doubles) are binaries of their own. *Reason:* every `tests/*.rs` is an executable that macOS
  scans on its first launch (12 s measured, up to 40 s under load); 62 of them made `cargo test` after a core change take
  27 minutes while the tests ran in milliseconds. *Consequence:* the tests share a process, so each registered double has
  a name no other has, and nothing asserts the whole registry.
- **Files have a size budget.** Source files up to 1000 lines, test files up to 1500, checked by `tools/check_file_size.py`.
  A file already over when the gate began is recorded at its size and may only shrink; an entry for a file that is gone or
  within budget fails the gate, so the list empties as files are split or deleted. *Reason:* agents read in chunks, and a
  huge file costs them tokens and makes edits land in the wrong place; no linter measures file length. *Consequence:* a
  growing file is split by concept, never exempted.
