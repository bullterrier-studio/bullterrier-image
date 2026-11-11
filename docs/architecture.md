# Architecture reference

The Rust core, the runner, regions, files and the Python bindings. The rules that bind are in `CLAUDE.md`
("Architecture rules"); read this before touching the runner, regions, files, threads or bindings.

**Rust core:** `crates/core` (crate `bullterrier-image`, pure Rust: `ops/` by concept, one `effects/<name>.rs` per
ported effect, found by `build.rs`), `crates/derive` (the `Params` and `Choice` derives), `crates/finders` (Apple Vision through `objc2`, the choice of a segmenter by
what the platform supports; the models, ONNX Runtime and the download are the core's `finding`, feature `onnx`) and `crates/python` (PyO3
bindings building `bullterrier_image._native._core`; generic, names no effect). A ported effect has **no Python folder**: the registry registers every
effect the core declares (`registry._register_core_effects`) with a function that calls `_core.apply("<name>", rgb,
values)`. Its Rust file declares the parameters (`Params`, see "Parameters"), its section of the catalogue, default
opacity and inputs (`pub(crate) const META: Meta`) and has `pub(crate) fn render(picture: &Picture, params: &Params, inputs: &Inputs) -> Image`; its
documentation is the `///` doc of `Params`, its table (`native.params("x")`) is that declaration as data, which every Python caller reads; category and opacity come from the core. The core's registry (`Effect` trait, `register`) also
takes foreign effects, such as a Python function (a `table=` in the parameter format gives it the parameters of a declared effect: checked and scaled by the core; `_core.register(name, category, opacity, fn, *, inputs=, fields=, table=)`; `fn(rgb,
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
`imageio.py` calls them through `core/native.py` and keeps what is Python's (composite over white, array checks). `Error::kind()` names the package exception of an error (`ErrorKind`, one mapping
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

The runner is given each map by name, as an array (`maps={"subject": alpha}`) or a `Finder(identity, find)`
(`finders={"subject": ...}`); the core's `Photo` takes the same (`with_map`, `with_finder`). `subject`, `depth` and `light` (made
from the working picture: framed and sized, before any step) are built in, and `background` is 1 - `subject`; any other map is a recipe's `given` region.
`bullterrier_image.recipe` binds the default `subject` and `depth` finders when a recipe needs them.
