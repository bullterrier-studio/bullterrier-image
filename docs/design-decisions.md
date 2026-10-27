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
  points").
- **A microkernel with plugins.** A small, stable `core` (registry, parameter models, recipe engine) and many effects that
  register themselves and are found by discovery. *Reason:* adding an effect must not edit shared files, so many effects
  can be written in parallel without conflicts. *Consequence:* an effect is new files named for it (its package, tests,
  example recipe, gallery picture) and nothing else; `poe new-effect` writes them from `effects/_template/`, and
  `gallery.effect_files(name)` is the one place that knows where they live.
- **Effects are pure functions in packages.** `effect(rgb, params, *, subject=None) -> rgb` on float32 RGB `(h, w, 3)` in
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
  fields). *Reason:* one declaration drives validation, defaults, `help(fx.<effect>)`, `bt-image effects`,
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
  picture (a direction, a rotation, a hue) is declared with `min` to `max` exactly 360 and takes any real number; a value
  outside is stored as the same direction inside (270 on -180..180 is -90; the bounds themselves are kept). Other angles
  (an elevation, a lean, a twist whose strength falls off with distance, a shift scaled by a weight) stay bounded and
  refuse what is out of range. *Reason:* ranges were 0..360, +-180 and +-360 by habit, and a caller that sets an
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
  field is resolved once and every level of the drive gets the same array; the drive keeps its own `by`. Effects declare
  a field only for what picks where or how much they work; the maps `subject` and `depth` stay inputs until the effects
  that take them move onto this. *Reason:* what an effect works from (depth, light, a band) is a choice of the look, so
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

## Recipes

- **Recipes are data.** A look is an ordered list of steps in TOML (or built in Python with `fx`), validated when it is
  made. *Reason:* data can be diffed, cached by fingerprint, edited by non-programmers and generated by tools.
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
  each preset (`effect = "slice_shift", preset = "glitch"`), a look (how to render it, and its own `description`), a
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
  y, both), `angle` (counter-clockwise degrees, with `fit` crop or expand and a `resample` filter), the crop (`aspect`
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
  effect takes a map by the same name (`subject`, `depth`); a region of kind `given` makes any other map usable as a
  selection (`mask = "star"`). *Reason:* the engine stays testable without a model, and the package serves a video tool
  that hands in maps of its own every frame (a drawing's silhouette from vector, a motion map from several frames) which
  image can neither make nor know in advance; one route for every map, not a keyword per map (`matte=`, `depth=`, then
  whatever comes next). A name no recipe declares is a `RecipeError` listing the names there are, so a misspelt map
  fails instead of being ignored. *Consequence:* `matte=`, `depth=`, `find_subject=`, `find_depth=`, `--matte`,
  `--depth`, `load_matte`, `load_depth` and the effect input `matte` are gone, with no alias before 1.0 (`--map
  NAME=PICTURE`, `imageio.load_map`, `subject`); the five effects' `matte_threshold` is `subject_threshold`; the `depth`
  region name is built in beside `subject` and `background`, so a `given` region cannot take it; `MapNotFoundError` is
  the base of `SubjectNotFoundError` and `DepthNotFoundError`; a project (`bt-image run`) holds no maps of its own, so a
  recipe with a `given` region needs a finder of that name. A map handed in is framed with the photo in one stack
  (`Framing.apply`), so its geometry is the photo's and every map pads with 0.
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

- **Pixel values are design pixels.** A `px` value is a length on a picture whose long side is 480, always:
  `recipe.size` is only how big the output is (the working size: `render(..., size=)`, else the recipe's size, else the
  framed photo's longest side). A render multiplies the values by the working size over 480, in the one place a step's
  values reach its effect (`EffectParams.scaled`, from the parameter description: no per-effect code). A whole number is
  rounded; a value shrinks no further than its field's `min` and grows without a ceiling, because a range is in design
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
  everywhere; `EffectParams.scaled` divides it by the size ratio squared, so the count of marks over the picture stays the
  same at any size. *Reason:* a density counted on the working pixels grew sixteen-fold in count at four times the size
  while each mark grew sixteen-fold in area, so a 1920 px render was far denser than a 480 px one. `flow_lines.length` is
  px along the line (steps = length / step), not a number of steps, so it scales with the picture like every length.

## Errors and I/O

- **One error hierarchy.** Library code raises `BullterrierImageError` subclasses (`RecipeError`, `UnknownEffectError`,
  `SubjectNotFoundError`, `BackendUnavailableError`, `ImageReadError`, `LutError`, `NonFiniteImageError`), each also inheriting the builtin it replaces.
  *Reason:* callers catch one type; `except ValueError` keeps working. *Consequence:* the CLI catches them and prints a
  message, never a traceback, for a user's mistake; a broken effect definition is a programming error (plain
  `ValueError`/`TypeError` at import).
- **`bt-image effects` is a short list: looks first, then one line per effect.** The default output is the looks
  (name and description), then each effect's name, summary and presets, then where the details are (`bt-image effects
  NAME`, or `--all`). *Reason:* a person or a model that reads it to find a style must see all of it on a few screens;
  the full parameter lists (about 15 times longer) bury the looks and the presets. *Consequence:* `--all` and
  `--advanced` give the full documentation; `--category` narrows either form; the generated `docs/effects/` is the full
  reference.
- **`imageio` is the only module that knows file formats**, EXIF rotation, colour profiles and 8-bit conversion.
  *Consequence:* everything else works on one in-memory image type.

## Finding the subject and depth

- **Two backends behind one function**, `subject_matte(rgb)`: IS-Net on ONNX Runtime, the default on every platform, and
  Apple Vision on macOS, used only when asked (`segmenter="vision"`, `BT_IMAGE_SEGMENTER=vision`); `--map subject=FILE` (or
  `maps={"subject": ...}`) skips both. *Reason:* a recipe must cut the same on a Mac and on Linux, so the portable model decides unless someone
  asks for the platform's own; neither makes the core depend on it. *Consequence:* `subject_finder()`'s identity by default
  is the portable model's (the cache keys of subjects remembered under Vision change once, so they are found again). The portable model (about 180 MB) is downloaded on first use into the
  user's cache, verified against a pinned SHA-256, never at import, never left half-written; `BT_IMAGE_SUBJECT_MODEL` points at a local
  copy for offline use.
- **`bt-image cutout` makes a cut-out file; `transparent = "background"` ends a look.** The command writes the photo at
  its own size with the subject's alpha, after an optional edge clean-up (`--shrink`, `--feather`), and needs no
  recipe; a recipe's transparency cuts out the finished look at the recipe's size. *Reason:* a cut-out file is the
  input of `--map subject=`, of `maps=` and of picture parameters (a logo, the example assets), so it is made once, checked
  and reused, which a look's last stage is not meant for. *Consequence:* both stay; the README says which to use when.
- **Depth is injected like the subject.** An effect that needs distance (`parallax`,
  `ambient_occlusion`) declares the input `depth`: float32 (h, w), 0..1, 1 = nearest; regions made of it (a `band` or `bands` of `depth`) read it too.
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
  every effect at its defaults on a 1 MP photo to 3 s of process CPU time (about ten times the slowest effect, so a
  shared CI runner, which is several times slower than a laptop, has room, while a ten-fold regression still fails;
  the effects themselves aim far lower) and 12 picture-sized buffers of memory (`tracemalloc`), with a known-bad effect
  for each limit; an effect that needs more is redesigned, or the budget is raised in its own reviewed commit. The FFT is single-threaded, so CPU time stays predictable when photos are
  rendered in parallel.

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

## Regions

- **A recipe declares named regions; a region is a partition or a selection.** `[region.<name>]` with a `kind`. A
  *partition* gives every pixel to exactly one piece (pieces numbered 0..n-1 in piece order: reading order, `bands`
  highest first; see "Piece numbers come from one rule"); a *selection* says how much each pixel is chosen (0..1). *Reason:* grids, cuts, shards, curves,
  SVG shapes and the subject are all "which pixels", so one idea serves them all and effects never learn about regions.
- **Steps use regions by name, with one word per idea.** `mask` is where a step applies (on every step, any selection;
  `subject` and `background` are built in); `each` + `looks` gives each piece of a partition its own look; `partition`
  is the partition an edge-aware effect draws. A hard-edged selection is also a two-piece partition. *Reason:* `mask`
  already means "where".
- **A look is a single inline step, or `"none"` (the piece is left as it is).** Named multi-step looks wait for a real
  need. Each distinct look is rendered once over the whole picture and blended through its pieces' weights, which sum
  to 1 and are then multiplied by `mask`: the cost is the number of looks, not pieces.
- **Positions are shares of the frame, lengths shares of the picture's long side.** The frame is the picture, or the
  box of the selection named by `fit`. The same look comes out at any resolution, torn edges too (their noise is laid
  on the frame, not on the pixel grid).
- **A field is a selection read for its values; `band`, `bands` and `combine` cut and join fields, `light` and two
  gradients make them.** A selection is already a float map 0..1, so no second concept is declared: a field is whatever
  selection a region's `of` names, built in (`depth`, `light`, `subject`) or the recipe's own. `band` (a selection)
  chooses where a field lies from `low` to `high`, fading linearly over `feather`; `bands` (a partition) cuts the field
  into `count` equal ranges, highest first; `combine` multiplies, takes the least or the greatest of several;
  `linear_gradient` and `radial_gradient` rise evenly with position (shares of the picture and of its long side, so a
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
  drivable (`param(..., drivable=True)`, floats only) takes a `Drive` instead of a value: `start` where the field `by` is 0,
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
- **The portable model is the default subject finder on every platform.** Apple Vision is used only when asked.
  *Reason:* `fit = "subject"` must cut the same on a Mac and on Linux.
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
  effect or parameters, only `each`, `looks`, `assign`, `seed`, `name` and `feather` (a share of the long side, so the same look
  comes out at any resolution; how softly the looks blend across a border, default 0.001 to smooth the pixels'
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
  in a square frame; a straight cut's `angle` is measured on the picture's pixels, counter-clockwise from horizontal.
  `ops/raster` fills outlines by the nonzero or even-odd rule a few rows at a time (no supersampled picture, so 4K costs
  about 1 s torn and 0.1 s straight): exactly across a pixel, `Path.samples` scanlines down it (advanced, default 4).
  A cut curve is closed along the frame's border and its inside read as one side; an end not on the border is carried
  straight on to the nearest. Every kind reads its drawing at the noise-shifted coordinates, so `edge = torn` works on
  all of them. *Reason:* one rule for position and one for borders; a library rasteriser would add a dependency for a
  fill that is a page of numpy.
- **SVG files are read by `svgelements` (MIT), behind a guard in `core/outlines`.** We never write an SVG reader;
  the library handles shapes, transforms, `use`, arcs and `viewBox`. A shape is a piece: colours and strokes are
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
