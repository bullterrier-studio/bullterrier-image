# JavaScript API

The core built for WebAssembly (`crates/wasm`), for a browser page or Node: a recipe rendered on a photo, with the pixels the
native build gives at the same size (byte for byte, except the effects that filter through the Fourier transform, which may
differ by one level of a channel: see [design decisions](design-decisions.md), "A WebAssembly package", which also has the
speed). The names are Python's (`Recipe`, effects, errors, `parameters()`, `withValues()`).

## Build

`uv run poe wasm-test` builds the package for Node (CommonJS, into the ignored `pkg/`) and tests it. For a browser, build the
same `.wasm` for the web, which gives the ES module a page imports (`wasm-bindgen` is the version pinned in `Cargo.toml`):

```sh
RUSTFLAGS="-C target-feature=+simd128" cargo build -p bullterrier-image-wasm --target wasm32-unknown-unknown --release --locked
wasm-bindgen --target web --out-dir web-pkg target/wasm32-unknown-unknown/release/bullterrier_image_wasm.wasm
```

## Use

```js
// a browser page: the web build
import init, { Photo, Recipe } from "./web-pkg/bullterrier_image_wasm.js";
await init();                                            // loads the .wasm

const photo = new Photo(imageData.data, imageData.width, imageData.height);   // 8-bit RGBA, the alpha is not read
const recipe = Recipe.fromToml(text);                    // a recipe file's text; also Recipe.fromJson(text), Recipe.look("crt")
const out = recipe.render(photo, { size: 577 });         // longest side in px, else the recipe's `size`, else the photo's
ctx.putImageData(new ImageData(out.data, out.width, out.height), 0, 0);

// Node: the package `poe wasm-test` builds, no init
const { Photo, Recipe } = require("./pkg/bullterrier_image_wasm.js");
```

| | |
|---|---|
| `new Photo(rgba, width, height)` | `rgba`: a `Uint8Array` or an `ImageData`'s `Uint8ClampedArray`, `width * height * 4` bytes. `TypeError` naming `rgba`, `width` or `height` for another type or a size that is not a whole number from 1; `InvalidImageError` for the wrong length. |
| `Recipe.fromToml(text)`, `Recipe.fromJson(text)`, `Recipe.look(name)` | A recipe from a recipe file's text, from the JSON `toJson()` writes, or a shipped look. Checked as Python's `load_recipe` does: `RecipeError` with the step and the rule (an unknown parameter, a value out of range), `UnknownEffectError` with the closest names. |
| `recipe.toJson()` | The recipe as JSON: `{ steps: [{ effect, params: {...} }], ... }`. |
| `recipe.parameters()` | Every number of the recipe that can be set, as Python's `Recipe.parameters()`: an object of the address (`"halftone.cell"`) to `{ value, unit, min, max, meaning, whole, cyclic }`; `value` is `null` for an optional number left unset, `unit` is `""` for none. What a slider needs. |
| `recipe.withValues({ address: number })` | A new recipe with those numbers set, as Python's `with_values()`; the recipe itself is left as it is. `RecipeError` naming the address for one that is not there or a value out of its range; `TypeError` for a value that is not a number. |
| `recipe.render(photo, options?)` | A plain object `{ width, height, data }`: `data` is a `Uint8ClampedArray` the page owns (4 bytes a pixel, alpha 255 unless the recipe cuts the picture out; nothing to `free()`). `options.size`: a whole number of pixels; `options.maps`: an object of name to `Float32Array` (`subject`: 1 where the subject is, `depth`: 1 nearest, or a region a recipe declares `given`), a value from 0 to 1 for each pixel of the photo; `TypeError` naming `options`, `size` or `maps.<name>` for another type. `SubjectNotFoundError` / `DepthNotFoundError` / `MapNotFoundError` when the recipe needs a map `maps` does not give or one has the wrong length; `RecipeError`, `UnknownEffectError` as above. The subject is not found in the browser: hand it in. |
| `effects()`, `looks()` | The names of the effects and of the shipped looks. |
| `parameters(effect)` | The effect's parameters in declaration order: `name`, `kind`, `meaning`, `default`, `unit` (`""` for none), `min`, `max`, `choices`, ... `UnknownEffectError` for another name. |

Every error is a JavaScript `Error` whose `name` is the Python exception's (`RecipeError`, `UnknownEffectError`, `InvalidImageError`,
...) and whose message is the core's; one with no Python class is a `BullterrierImageError`. `Photo` and `Recipe` hold memory in
the module: `free()` them when a page makes many (a slider makes one `Recipe` per move with `withValues`).

No file system is there: a recipe that names a file (the `image` of `blend`, the `reference` of `color_transfer`, the `file`
of `lut`, an svg region, a picture by path) throws `ImageReadError` or `LutError` when it renders, and one that reads the
depth (`ambient_occlusion`, `parallax`, `relight`, `topographic`, any `mask = "depth"`) throws `DepthNotFoundError` unless the
page hands in `maps.depth`.

A render runs on the calling thread and blocks it; run it in a worker to keep a page responsive. At 0.25 MP it takes about
30 ms, at 1 MP about 90 ms (about 1.3 times native, one thread).
