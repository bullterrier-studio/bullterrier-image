# JavaScript API

The core built for WebAssembly (`crates/wasm`), for a browser page or Node: a recipe rendered on a photo, with the pixels the native
build gives at the same size. Built by `uv run poe wasm-test` (a release build with `simd128`, then `wasm-bindgen`) into the
ignored `pkg/`; `wasm-bindgen --target web` gives the ES module a page imports, `--target nodejs` the CommonJS one. Why and how
fast: [design decisions](design-decisions.md), "A WebAssembly package". The names are Python's (`Recipe`, effects, errors).

```js
import init, { Photo, Recipe, effects, looks, parameters } from "./pkg/bullterrier_image_wasm.js";
await init();                                            // the web target loads the .wasm; Node's needs no call

const photo = new Photo(imageData.data, imageData.width, imageData.height);   // 8-bit RGBA, the alpha is not read
const recipe = Recipe.fromToml(text);                    // a recipe file's text; also Recipe.fromJson(text), Recipe.look("crt")
const out = recipe.render(photo, { size: 577 });         // longest side in px, else the recipe's `size`, else the photo's
ctx.putImageData(new ImageData(out.data, out.width, out.height), 0, 0);
```

| | |
|---|---|
| `new Photo(rgba, width, height)` | `rgba`: a `Uint8Array` or an `ImageData`'s `Uint8ClampedArray`, `width * height * 4` bytes. `InvalidImageError` for any other length. |
| `photo.setMap(name, values)` | Hands in a map: a `Float32Array` of 0..1, a value per pixel (`"subject"`: 1 where the subject is, `"depth"`: 1 nearest, or a region a recipe declares `given`). The subject is not found in the browser. `MapNotFoundError` (`SubjectNotFoundError` for `"subject"`) for the wrong count or a value outside 0..1. |
| `Recipe.fromToml(text)`, `Recipe.fromJson(text)`, `Recipe.look(name)` | A recipe from a recipe file's text, from the JSON `toJson()` writes, or a shipped look. `RecipeError` with the line and the rule. |
| `recipe.toJson()` | The recipe as JSON: `{ steps: [{ effect, params: {...} }], ... }`. Change a value and read it back with `fromJson`: how a slider edits a recipe. |
| `recipe.render(photo, options?)` | A `Rendered`: `width`, `height`, `data` (a `Uint8ClampedArray`, a new copy per read; 4 bytes a pixel, alpha 255 unless the recipe cuts the picture out). `options.size`: a whole number of pixels, else `TypeError`. The core checks the recipe first: `RecipeError`, `UnknownEffectError` (with the closest names), `SubjectNotFoundError` when it needs a subject that was not handed in. |
| `effects()`, `looks()` | The names of the effects and of the shipped looks. |
| `parameters(effect)` | The effect's parameters in declaration order: `name`, `kind`, `meaning`, `default`, `unit`, `min`, `max`, `choices`, ... : what a slider needs. `UnknownEffectError` for another name. |

Every error is a JavaScript `Error` whose `name` is the Python exception's (`RecipeError`, `UnknownEffectError`, `InvalidImageError`,
...) and whose message is the core's; one with no Python class is a `BullterrierImageError`. `Photo`, `Recipe` and `Rendered`
hold memory in the module: `free()` them when a page makes many (a slider makes one of each per move).

A render runs on the calling thread and blocks it; run it in a worker to keep a page responsive. At 0.25 MP it takes about
30 ms, at 1 MP about 90 ms (about 1.3 times native, one thread).
