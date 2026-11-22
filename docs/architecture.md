# Architecture reference

The Rust core, the runner, regions, files and the Python bindings. The rules that bind are in `CLAUDE.md`
("Architecture rules"); read this before touching the runner, regions, files, threads or bindings.

**Rust core:** `crates/core` (crate `bullterrier-image`, pure Rust: `ops/` by concept, one `effects/<name>.rs` per
ported effect, found by `build.rs`), `crates/derive` (the `Params` and `Choice` derives), `crates/finders` (Apple Vision through `objc2`, the choice of a segmenter by
what the platform supports; the models, ONNX Runtime and the download are the core's `finding`, feature `onnx`) `crates/python` (PyO3
bindings building `bullterrier_image._native._core`; generic, names no effect) and `crates/wasm` (the WebAssembly bindings: a `Photo`, a `Recipe`, `effects()`; [javascript-api.md](javascript-api.md)). A ported effect has **no Python folder**: the registry registers every
effect the core declares (`registry._register_core_effects`) with a function that calls `_core.apply("<name>", rgb,
values)`. Its Rust file declares the parameters (`Params`, see "Parameters"), its section of the catalogue, default
opacity and inputs (`pub(crate) const META: Meta`) and has `pub(crate) fn render(picture: &Picture, params: &Params, inputs: &Inputs) -> Image`; its
documentation is the `///` doc of `Params`, its table (`native.params("x")`) is that declaration as data, which every Python caller reads; category and opacity come from the core. The core's registry (`Effect` trait, `register`) also
takes foreign effects, such as a Python function (its `table=`, in the parameter format and required, gives it its parameters, checked and scaled by the core, none for an effect without; `_core.register(name, category, opacity, fn, *, table, inputs=, fields=)`; `fn(rgb,
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
`Step`, `Each`, `Recipe`, regions and `Framing` are frozen dataclasses over these types: the core checks each when it is
made (`check_step`, `region`, `frame`), and a `Recipe` is held to `Recipe::checked`, which alone reads a region's path data.
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
profile converted to sRGB), `read_map`, `write` and `write_with` (png, jpg, webp; only the sRGB profile
embedded, and a `dpi` if asked: `pHYs`, JFIF density); a fixed pixel limit and a decoder memory cap; `ErrorKind::ImageRead`, `ImageWrite` and `BackendUnavailable`.
`imageio.py` calls them through `core/native.py` and keeps what is Python's (composite over white, array checks). `Error::kind()` names the package exception of an error (`ErrorKind`, one mapping
for every binding). **Regions:** `Recipe.regions` is an ordered `Regions` of the 15 typed kinds in `bullterrier_image::region`
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
`trace` install the rayon pool of that size (`ops/par.rs`: one for each count, started the first time and shared by every render at that count in the
process, so renders running at once on several threads of the caller use n threads in all); the render stays on the calling thread, only row bands run on the
pool, and rayon's global pool is never used. A kernel fills its output with `par::rows(out, row_len, |first_row, band| ..)`
(`Picture::map` and `map_at` do, so a per-pixel effect is threaded as written); outside a pool it runs inline, and
`Effect::apply` takes no thread parameter. Rule: the output looks the same at any thread count and is identical where that comes naturally (a band reads only
input rows and carries nothing from one row to the next; a reduction chunks by the data, never by the thread count). Never add
real complexity for the last bit: if a kernel's rounding differs between runs with no visible effect, compare within a small
tolerance. Tests: render at threads 1, 2, 3, 10 and compare (`crates/core/tests/it/threads.rs`). Python: `render`/`trace`/`render_all`
take `threads=`; `Step.apply` and `fx` use 1.
**Canvas:** `Options::with_canvas(width, height)` names the full canvas the returned picture is placed on, in the pixels of that picture
(after `size`). The design scale (`px` per design px: effect parameters, the `each` feather, a region's edge) is then the canvas's
longest side over 480, not the picture's, so a `px` value is the same number of output pixels on every picture of one canvas.
Nothing else follows the canvas yet: `Place` is still the picture. A side of 0 is refused (`Error::Recipe`, naming
`canvas`); a canvas smaller than the picture is fine (it only fixes the scale). Not in Python.
**Window:** `Options::with_window(first_row, canvas_width, canvas_height)` says the picture handed to `render` is rows of a larger canvas
from canvas row `first_row`: the runner builds every effect's `Place` from it (`run/size.rs` `Origin`; the steps of a band the runner cuts add their
offset), and the design scale follows the window's canvas unless `with_canvas` is given: the canvas fixes the scale, the window the place. No effect reads it differently: a position-aware effect already
reads `Place`, so a band of rows equals the same rows of the whole render. Refused (`Error::Recipe`, naming `window`): a width other than the picture's,
rows beyond the canvas, a side of 0, and a `frame` or `size` (they change the rows). `Recipe::band_margin(presets, longest_side) -> Option<usize>` is the
caller's query: the rows to hand each side of a band (the steps' margins add), none for a recipe a window cannot make: a `frame` or `size`, a step that
reads the whole picture, an `each` or driven step, one with a `mask`, `partition` or field parameter (the maps of a band are not the canvas's), or one
that sums a blur by the Fourier transform (`Footprint::Fourier`, from a reach of 16 working px, or a kernel of more than 128 taps: its sum depends on the
rows it is given), or reads rows at a distance of less than 2^-26 px (`Footprint::Rounded`: `echo`'s `dy`; the position rounds by the row). What it clears is exact: the rows of a window are the whole render's, byte for byte. (`Options::with_band` stays within 1e-4.) `Recipe::close_band_margin(presets, longest_side)` is the same query for a window that need only look the same: it also clears `Footprint::Fourier` and `Footprint::Rounded` steps, whose rows are the whole render's to within 32 of 65,535 on a value of 0 to 1 (0.13 of an 8-bit level; each such step, a later step that raises contrast multiplies it); where `band_margin` has an answer, the two agree and the window is exact.
`tests/it/window.rs` renders every effect it clears as windows of band plus margin against the whole render, byte for byte, and a table of recipes on
both sides of the Fourier switch. Not in Python.

**Spread:** `Effect::spread(params) -> Spread` says how far an effect paints beyond the edge of the box of its content (none by default; `Spread::NONE` for an effect that
recolours or paints inside its shapes), per side in working px, or `Spread::Unbounded`. It is a separate promise from `Footprint`, which is about the rows
an effect reads: a sideways reach costs a band nothing but is room to a caller. The promise: on a plain background with some content, whatever the effect makes
that differs from what it makes of the plain background lies within the content's box grown by the spread. A file in `src/effects/` declares it as
`pub(crate) fn spread(&Params) -> Spread` (the build script finds it; an effect without one spreads none): `Spread::of_footprint(footprint(params))` for an
effect that reaches as far sideways as it reads rows (the radius and a bicubic window's 2 px), `Spread::moved` for a shift, an explicit sum otherwise,
`Spread::Unbounded` where it is not worked out (a warp about the picture's centre, a statistic of the whole picture, a tiling). `tests/it/spread.rs` holds every
effect to its word with a block on a grey canvas, over the parameter sweep, so one that paints outward and says nothing fails. `Recipe::spread(presets,
longest_side) -> Result<Spread, Error>` is the room of a whole recipe, in px of a picture (or canvas) of that longest side: the steps' spreads add, a step of opacity 0 adds none, an
`each` step and a driven step add their widest look and level; it reads parameters only and loads no picture or table. `Spread::pixels()` rounds up
to whole pixels (top, right, bottom, left). **A caller that grows the picture by that room** renders it with `Options::with_room(top, right, bottom, left)` (below), so that the effects that
place themselves by the picture (the vignette, a `x`/`y` centre, `Place`-numbered noise, `frame`, a statistic of the whole picture) take the original box, not the grown picture, as theirs.

**Data:** An effect that makes data beside its picture (ink planes, a palette-index map or a grid of characters: `halftone`, `dither`, `ascii`) declares it as `pub(crate) fn data(&Picture, &Params, &Inputs) -> Result<Data, Error>` in its file (the build script finds it, as it finds `spread`); one that does not is refused with one message, `effect 'name' makes no data beside its picture`. `Data` is crate-private: callers use `Photo::ink_planes`, `palette_map` and `characters` (Python the same), each the last step's data of the very picture `render` makes, so the effect draws once for both.

**Check:** An optional `pub(crate) fn check(&Params, &Inputs) -> Result<(), Error>` in the effect's file (the build script finds it, as it finds `spread`) refuses a run with an error naming the parameter, for what the parameter table cannot hold: a picture parameter's size or presence (`dither`'s `matrix`).

**Room:** `Options::with_room(top, right, bottom, left)` says the picture handed to `render` is the box grown by that room, and the render is of the box. The design scale
follows the box's longest side (the window's canvas less the room, else the picture less it). A *boxed* step is run on the box alone, as on a picture without room, and the
room round it is left as the step received it (`run/step.rs`, `Prepared::apply`, `Origin::part`): an `each` step, a driven step, an effect whose `Footprint` is `Whole`
(statistics and warps about the middle read the box only) and an effect that declares `Input::Place` (it places itself by the picture; `lenticular` and `melt` declare it for
strips and noise counted from the picture's left edge). Every other step runs on the whole grown picture, so a halo it paints into the room is kept and it reads the room
beside the box's edge. **The limit:** a boxed step does not reach what an earlier step painted into the room (a vignette after a bloom leaves the halo undarkened); a look
that wants it continuous orders the halos last. Regions (`mask`, fields, partitions) are made for the grown picture and cut to the box. With `with_window` the room is in the
window's canvas; with `with_band` the runner's bands compose. Refused (`Error::Recipe`, naming `room`): a room that leaves no box, and a `frame` or `size`. `tests/it/room.rs`
renders every effect on a grown picture (its edge repeated) against the box alone: bit for bit for a boxed step, and inside its margin and spread for the others. Not in Python.

**GPU** (feature `gpu`; `crates/core/src/gpu.rs`, `gpu/`, `run/gpu.rs`; why: design-decisions, "GPU backend"). One backend
choice, the render options; the CPU is the reference and byte promises are CPU promises.

```text
let gpu = Gpu::request_blocking()?;                       // or Gpu::new(device, queue): the caller's wgpu device
let options = Options::default().with_size(1080).with_gpu(&gpu);
let image: Image = photo.render(&recipe, &presets, options.clone())?;             // on the GPU, one readback
let resident: GpuImage = gpu.render_blocking(&photo, &recipe, &presets, options)?; // stays on the device
let next = gpu.render_from_blocking(&resident, &recipe2, &presets, Options::default())?;
let back: Image = gpu.download_blocking(&next)?;          // async: `download`, `render`, `render_from`, `request`
```

`Gpu` (Clone, shared; equal only as clones): the device and queue, pipelines by kernel name, the largest picture, a transfer
count; wgpu calls run in error scopes (`Gpu::guarded`), so a failure is `Error::Gpu`. `GpuImage`: one `vec4<f32>` a pixel in a
storage buffer, straight alpha, owned by its `Gpu` (another's is refused). Upload: 3 or 4 floats a pixel, spread by the
`unpack` kernel; readback: a copy and a map. A kernel: `gpu/prelude.wgsl` (bindings: 0 uniform `Header` then the effect's
`Block` of vec4s, 1 `source`, 2 `dest`, 3 `data` floats, 4 `other` picture; `inside`, `pixel_index`, `store`, `luminance`,
the sRGB transfer) plus its own `main`, `@workgroup_size(16, 16)`. An effect file declares `fn kernel() -> Kernel<Params>`
(WGSL, the block from `Params`, an optional table, default bound 1e-4) beside `<effect>.wgsl`; `build.rs` adds it under the
feature, `Native` erases it into the public `GpuKernel` that `Effect::kernel` returns (None for every other effect). The
executor (`run/gpu.rs`): `begin()` on the CPU, the working picture uploaded once, runs of kernel steps (`Prepared::on_gpu`:
a plain step of an effect with a kernel, no drive, field or partition) recorded into one encoder, each laid over its input by
`gpu/lay.wgsl` unless plain; any other step through `Prepared::apply` between a readback and an upload. `frame.transparent`:
`Cut` after the readback, or `gpu/alpha.wgsl` for `Gpu::render`. Refused by name (`Error::Gpu`): `with_band`, `with_window`,
`with_room`; `trace`, `region_map` and the data calls with a GPU; `render_from` with a `frame`, another `size` or a map. Tests:
`tests/it/gpu/` (`poe rust-test-gpu`; `BT_GPU`, `docs/development.md`); Python: `bt.Gpu`, `render(..., gpu=)`.

The runner is given each map by name, as an array (`maps={"subject": alpha}`) or a `Finder(identity, find)`
(`finders={"subject": ...}`); the core's `Photo` takes the same (`with_map`, `with_finder`). `subject`, `depth` and `light` (made
from the working picture: framed and sized, before any step) are built in, and `background` is 1 - `subject`; any other map is a recipe's `given` region.
`bullterrier_image.recipe` binds the default `subject` and `depth` finders when a recipe needs them.
