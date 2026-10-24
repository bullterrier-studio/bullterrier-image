# Design decisions

The decisions that shape photo-fx and still hold, each with its reason and what it costs or commits us to. Rules for
contributors are in [CLAUDE.md](../CLAUDE.md); what is planned is in the [effects roadmap](effects-roadmap.md).

## Architecture

- **Linear layers: app > adapters > effects > core > ops > errors.** Each layer imports only the layers below it; the
  exception hierarchy (`photo_fx.errors`) imports nothing, so it is the bottom layer and every layer may raise it.
  *Reason:* one direction of dependency is the simplest rule that stays true as the project grows; a forbidden import
  means the code is in the wrong layer, so the fix is always a move, never an exception. *Consequence:* the rules are
  import-linter contracts in `pyproject.toml`, run by the test suite (`uv run lint-imports`); `core` never imports
  `effects`: the registry loads the effect library through an entry point (see "Effects are found through entry
  points").
- **A microkernel with plugins.** A small, stable `core` (registry, parameter models, recipe engine) and many effects that
  register themselves and are found by discovery. *Reason:* adding an effect must not edit shared files, so many effects
  can be written in parallel without conflicts. *Consequence:* an effect is new files named for it (its package, tests,
  example recipe, gallery picture) and nothing else; `poe new-effect` writes them from `effects/_template/`, and
  `gallery.effect_files(name)` is the one place that knows where they live.
- **Effects are pure functions in packages.** `effect(rgb, params, *, matte=None) -> rgb` on float32 RGB `(h, w, 3)` in
  0..1, deterministic, never modifying its input, one package per effect holding code only; its tests mirror `src/`
  (`tests/effects/<name>/`), its example and picture live outside the package. *Reason:* a pure function is trivially
  testable and composable, and keeping artefacts out of `src/` keeps the wheel small. *Consequence:* effects never import
  each other; shared numeric code goes to `ops/`, named by concept.
- **Effects are found through entry points, on first use.** The registry imports every module of the entry-point group
  `photo_fx.effects` the first time it is queried, once, under a lock; photo-fx's own effect library is one such entry
  point and a third-party package declares its own, written against the public `photo_fx.plugin`. *Reason:* importing
  every effect (and scipy, pydantic) made `import photo_fx` cost a quarter of a second before anything was used, and
  left outside packages no way into recipes or the command line; entry points are PyPA's standard plugin mechanism
  (pytest, Sphinx, flake8 use it). *Consequence:* a checkout needs `uv sync` to install the entry point; a plugin that
  fails to import is a `PhotoFxError` naming it, `PHOTO_FX_PLUGINS=builtin` skips third-party packages, effect names
  are unique across every installed package, and registering is for import time (queries are thread-safe once loaded).
- **`ops` modules arrive with their first consumer.** *Reason:* code nothing uses is a cost with no benefit, and a
  primitive designed before its user is designed for the wrong user. *Consequence:* the dead-code gate fails on unused
  code; the removed primitives (`colorspace`, `noise`, `edges`, `warp`, `voronoi`, `sprites`) are restored from commit
  3b9b2ed (`git show 3b9b2ed:src/photo_fx/ops/<module>.py`) together with the effect that needs them.
- **One public path per name.** The public API is `photo_fx` (with `render` & co. binding the default subject finder),
  `photo_fx.recipe`, `photo_fx.fx`, `photo_fx.imageio` and `photo_fx.plugin`, each with `__all__`. *Reason:* two paths to one name with
  different behaviour (a `render` that finds the subject and one that does not) is a trap. *Consequence:* `photo_fx.core`
  and `photo_fx.ops` are internal and re-export nothing; no compatibility shims before 1.0.
- **No time in the library.** Effects are pure functions of one picture and a set of values; a parameter describes a state
  (`progress`, `phase`, `seed`), never a duration. Easing, ramps, animated values, the animation timeline, frame
  rendering and GIF/MP4 output were removed. *Reason:* photo-fx is the image package of a wider ecosystem
  (`bullterrier-design`) in which one package owns each concept: `control` shapes values over time, `video` maps them
  onto parameters and renders frames, `audio` supplies the beats. Animation done here would exist twice, and a parameter
  measured in seconds would tie an effect to one clip and one clock. *Consequence:* a parameter that moved with time
  becomes a state parameter in `cycles` or `fraction` (disperse's shimmer is `ambient_phase`); the removed code is in git
  history (before the "remove animation" commits) for when video is built.

## Parameters

- **Every effect declares its parameters once, as a frozen Pydantic model** (`EffectParams`, `extra="forbid"`, strict
  fields). *Reason:* one declaration drives validation, defaults, `help(fx.<effect>)`, `photo-fx effects`,
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
  fields first, then an "Advanced parameters" section (`help`, `fx.pyi`), a count in `photo-fx effects` (`--advanced` or
  naming the effect shows all) and a collapsed block in the catalogue. Every field stays settable, validated, in
  declaration order in the signature and in the JSON schema (`"advanced"`). No tiers, no classification by name.
- **Names are the field's own terms** (radius, sigma, amount, threshold, gamma, exposure, seed...), never `strength`,
  `intensity` or `factor`. *Reason:* users bring vocabulary from Photoshop, Lightroom and shader literature.
  *Consequence:* a shared vocabulary table and a contract test flag discouraged names.
- **What lies beyond the picture's edge is always a field named `edge`** (reflect, nearest, mirror, wrap, and constant
  where supported), in every effect that looks past the border. *Reason:* one concept, one name; `mode` is often taken
  by an effect's own choice (`radial_blur`: spin or zoom). *Consequence:* a contract test fails an effect field whose
  choices are edge rules but whose name is not `edge`.
- **`interpolation` is how an effect reads the picture between pixels; `resampling` is the same choice for a generated field.**
  `smoke` and `reaction_diffusion` warp or upsample a field they made themselves, then colour or blend it, so the
  contract's "a constant picture stays constant under every interpolation" does not hold for them. *Reason:* the contract
  rule is about warping the photo; a second name keeps it simple instead of an exemption list. *Consequence:* a field
  named `interpolation` promises that rule; use `resampling` when the effect does not.
- **An effect's controls are named scalars; a list of numbers is for data, not for controls.** `channel_mixer` takes
  nine fields (`red_from_green`...) and `curves` a parametric curve (shadows, midtones, highlights, per master and
  channel), not a `matrix` or a `points` list. *Reason:* only scalars can be animated with `ramp()`, become a slider
  with a unit and range in the UI generated from the effect metadata, and carry their own documentation and error
  message; a list is validated per item only, and a free-point curve needs a curve editor that sliders cannot give.
  *Consequence:* a draggable-point curve or a free-length LUT is a separate, later effect (`point_curve`, `lut`) with
  its own design; if a list parameter is ever added, any in-range list must be valid by construction (evenly spaced
  output heights, not x/y pairs that must increase), and the contract's limits test then handles list fields.

## Recipes

- **Recipes are data.** A look is an ordered list of steps in TOML (or built in Python with `fx`), validated when it is
  made. *Reason:* data can be diffed, cached by fingerprint, edited by non-programmers and generated by tools.
  *Consequence:* a recipe file may carry `format = <n>`, and a release that
  changes the format adds a number and keeps reading the old ones.
- **Presets are per-effect parameter sets, shipped as data in `src/photo_fx/presets/<effect>.toml`.** A look that is
  one effect with a set of values (sepia is a gradient map, teal-and-orange a split tone) is a preset of that effect,
  not a new effect: `fx.gradient_map(preset="sepia")`, or `preset = "sepia"` in a recipe step. *Reason:* a new effect
  is code, tests and a picture for what is a few numbers, and a preset is checked by the effect's own parameter model,
  so it can neither drift from nor duplicate the effect's rules. *Consequence:* `Step` merges the preset under the
  parameters it is given (the given ones win) when it is made, so the runner, the loader and the cache never see a
  preset; an effect with presets lists them in its documentation, the catalogue and the editor stub; only built-in
  effects have presets (a third-party effect's author ships data of their own); a look that needs several effects is
  a recipe, not a preset.
- **Every step can be limited and blended: `mask` and `opacity` belong to the step, not to the effect.** `mask`
  (`subject` or `background`) and `opacity` (0 to 1) are applied by the runner after any effect, so a gradient can sit
  behind the subject, or an effect can be mixed in at 40%, with no effect knowing about it. An effect may still have its
  own `opacity` when it means that effect's own element (the brightness of a particle, the share a soft focus replaces);
  the two multiply. *Reason:* one place for the general blend, standard names for the specific one. *Consequence:*
  never add an effect parameter that only mixes the effect's result with the picture.
- **The subject is injected.** The runner is given a `matte` or a `find_subject` function; `photo_fx.recipe` binds the
  default. *Reason:* the engine stays testable without a segmentation model. *Consequence:* tests inject stand-ins
  through public parameters instead of patching.

## Errors and I/O

- **One error hierarchy.** Library code raises `PhotoFxError` subclasses (`RecipeError`, `UnknownEffectError`,
  `SubjectNotFoundError`, `BackendUnavailableError`, `ImageReadError`, `NoFramesError`, `NonFiniteImageError`), each also inheriting the builtin it replaces.
  *Reason:* callers catch one type; `except ValueError` keeps working. *Consequence:* the CLI catches them and prints a
  message, never a traceback, for a user's mistake; a broken effect definition is a programming error (plain
  `ValueError`/`TypeError` at import).
- **`imageio` is the only module that knows file formats**, EXIF rotation, colour profiles and 8-bit conversion.
  *Consequence:* everything else works on one in-memory image type.

## Finding the subject

- **Two backends behind one function**, `subject_matte(rgb)`: Apple Vision on macOS (built in, no download) and IS-Net on
  ONNX Runtime everywhere else; `--matte` (or `matte=`) skips both. *Reason:* the best cut-out per platform without making
  the core depend on either. *Consequence:* the portable model (about 180 MB) is downloaded on first use into the user's
  cache, verified against a pinned SHA-256, never at import, never left half-written; `PHOTO_FX_MODEL` points at a local
  copy for offline use.

## Determinism

- **Same inputs and seed give the same output on the same machine.** Stochastic code takes `seed: int` and uses
  `numpy.random.default_rng(seed)`, never global random state. *Reason:* caching, debugging and reviews need repeatable
  output. *Consequence:* byte-identical comparison is the proof for refactors; small floating-point differences between
  platforms or library builds are tolerated, so tests assert properties with tolerances, not exact pixels.

## Performance

- **Cost grows with the picture, not with the parameters, and memory stays a few pictures deep.** A 12 MP photo is
  the working case, so every Gaussian goes through `ops/blur.gaussian_blur`: scipy's direct filter, byte for byte,
  below a kernel radius of 32 px, and from there an FFT convolution over the same edge extension, within 1e-5 of
  scipy (float32 rounding), whatever the sigma. Code that builds per-pixel geometry works a band of rows at a time.
  *Reason:* a radius at the top of its documented range must still render in seconds, and a direct convolution costs
  in proportion to its kernel (clarity at 30 px spent 3 s per 12 MP photo in it). *Consequence:* a contract test holds
  every effect at its defaults on a 1 MP photo to 1 s of process CPU time and 12 picture-sized buffers of memory
  (`tracemalloc`), with a known-bad effect for each limit; an effect that needs more is redesigned, or the budget is
  raised in its own reviewed commit. The FFT is single-threaded, so CPU time stays predictable when photos are
  rendered in parallel.

## Fixing defects

- **A fix removes the cause; it does not add a defence.** The bug is reproduced at the public interface, fixed where the invalid state is created (a stricter parameter model, one source of truth, a removed path),
  and the same class is searched for elsewhere. Inside the library code trusts its contracts and fails loudly; inputs are validated once at the boundary with a `PhotoFxError`. *Reason:* agents and automated loops
  patch where a failure shows (fallbacks, defaults, swallowed exceptions, extra checks), which "makes the system less understandable while appearing more robust" and keeps the cause (Armin Ronacher,
  "The Coming Loop", 2026); the repo's own instances were library options hidden behind `# constant:` and two copies of the same edge table. *Consequence:* the rules live in `CLAUDE.md` ("Fixing defects"),
  duplicated knowledge (code or tests) is merged, and what a tool can see becomes a gate (blind `except`, literal library options, the marker rule).

## Testing and gates

- **Properties, not golden images.** Tests assert the property that defines a behaviour (a blur stays in range, a sort
  keeps each column's pixels) against an independent oracle. *Reason:* a golden image written with the code proves
  nothing and churns binary diffs. *Consequence:* no goldens, no change-detector tests of configuration.
- **Behaviour through the public interface.** No private imports, no mocks of our own code, no call-count assertions;
  only true external boundaries (the model, the network, `ffmpeg`, the clock) are substituted, preferably by injection.
  *Reason:* a test must survive any behaviour-preserving refactor. *Consequence:* hard-to-test code gets a better
  interface, not a reach-in.
- **The gates are the review.** `uv run poe check` runs format, lint, dead code, dependencies, build, types and tests;
  each gate has a known-bad test proving it can fail. *Reason:* reviews stay light and consistent. *Consequence:* a
  defect a review finds twice becomes a gate.
