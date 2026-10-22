# Build order

How to build the [effects roadmap](effects-roadmap.md) so that many effects can be written in parallel, by several people or
agents, without colliding. Status: section 0 is implemented and merged; the rest is the plan being followed.

Parallel work pays only when tasks share no files and no unsettled decisions. So the order is: (0) remove the places where
every new effect would collide, (1) settle the shared contract and build the shared toolbox, (2) run independent streams.

## 0. Make adding an effect conflict-free

**Status: implemented.** Adding an effect now means adding new files named for it and editing no shared file:
`src/photo_fx/effects/<effect>/` (its package: `effect.py` and `__init__.py`), `tests/effects/<effect>/test_<effect>.py`
(its tests; `tests/` mirrors `src/`), `examples/effects/<effect>.toml` (its example recipe) and later
`docs/gallery/<effect>.jpg` (its picture). `uv run photo-fx new-effect <effect> --category <category>` writes the first
three from the template in `src/photo_fx/effects/_template/`. Each row was something every new effect had to touch.

| Was | Problem | Now |
| --- | --- | --- |
| `effects/__init__.py` listed every module | Everyone edits the same line. | Every effect package (folder) in `effects/` whose name does not start with `_` is imported automatically, in sorted order (and, until the last ones are migrated, every loose module); a folder that does not register exactly one effect named like it fails the import. |
| The registry stored `_EFFECTS[name] = fn` | Two effects with the same name silently replace each other. | A second, different function under an existing name raises `ValueError` naming both modules (a module reload is allowed). |
| `fx.pyi` was committed and a test failed if it was stale | Every branch changes it; every merge conflicts. | Still committed, but regenerated only at integration; the test checks that the generator's output is valid Python with one function per effect. |
| The README effect table was test-enforced | Every effect edits the README. | `docs/effects.md`, generated from the effects, one table per category with picture, summary, parameters and example recipe; the README links to it. |
| `tests/test_effects.py` held the per-effect tests | One shared file. | One test file per effect, `tests/effects/<effect>/test_<effect>.py`; the generic contract tests are in `tests/effects/test_contract.py` and apply to every effect automatically. |
| Example pictures regenerated whenever any code changed | Binary merge conflicts. | One gallery picture per effect, `docs/gallery/<effect>.jpg`, rendered from its example recipe; only missing pictures are rendered unless forced. `gallery.effect_files(name)` is the one place that says where an effect's files are. |
| An effect declared nothing about itself | Docs, gallery and checks cannot be generated. | `@effect(category="...")`, one of the categories in `effects.CATEGORIES`; every parameter must have a default; the example recipe is found by name. All checked when the effect registers, or by a test. |
| Parameter names were free-form | Independent authors drift: `strength`, `amount`, `intensity`. | `effects/_vocabulary.py` (now `core/vocabulary.py`) lists discouraged names and the name to use instead (`strength` -> `amount`, `rng` -> `seed`, ...); a contract test flags them. `glow(strength=)` became `glow(amount=)`. |

**The one integration command** is `photo-fx regenerate`: it writes `src/photo_fx/fx.pyi` and `docs/effects.md` and
renders any missing gallery pictures. Feature branches do not run it or commit those files; whoever merges runs it once
afterwards and commits the result. (`photo-fx gallery [--only NAME ...] [--force]` renders pictures and writes the page
without the stub; rendering needs the subject cut-out, so macOS.)

## 1. The shared contract

These change how recipes run, and all of them pass through the recipe modules. `recipe.py` (396 lines mixing easing, the
step model, TOML loading and running) has been split (**done**) into `timing`, `steps`, `loader` and `runner`, which now
live in `src/photo_fx/core/` (**done**, M1; `photo_fx/recipe.py` stays as the public namespace and binds the default subject
finder), so that each step below touches different files. **Do them one at a time.** The import-linter contracts in
`pyproject.toml` (run by `tests/test_layering.py`) enforce which of these modules may import which, and the package's layers.

1. **Inputs beyond the image.** Generalise today's `matte` into declared inputs injected by name: `matte`, then `edges`, `depth`, `saliency`, assets. One protocol; later effects only declare what they need.
2. **Masks and blend modes on the step.** `blend = "multiply"`; masks beyond subject and background (luminance, highlights, shadows, edges, gradient), invertible and feathered. Effects stay unaware of both.
3. **Presets.** `preset = "vhs"` in a recipe: named, parameterised lists of steps. Needs 2.
4. **Assets.** Images, LUTs and fonts referenced relative to the recipe and fingerprinted by the cache. Needs 1.
5. **Size-changing effects.** The runner allows them and carries the subject cut-out along. Needs 1.
6. **Parameter ranges and `photo-fx sweep`.** Declared bounds checked at load; a command that renders one parameter across a range, so a look can be tuned without a model in the loop.

## 2. The shared toolbox

Internal modules under `photo_fx/ops/`, not effects. Each is independent, small, and tested on its own, so they can be built
in parallel with each other and with section 1. They exist because several effects need the same non-trivial piece.

| Module | What it provides | Used by |
| --- | --- | --- |
| `colorspace` | sRGB / linear light, HSV, Lab. Also the option to blur, glow and blend in linear light. | `vibrance`, `selective_color`, `color_balance`, `white_balance`, `color_transfer`, `equalize`, `bloom`, `lens_blur`, blend modes |
| `noise` | Seeded value, simplex and Worley noise fields. | `displace`, `flow_warp`, `melt`, `fog`, `smoke`, `paper_texture`, `dust_scratches`, `signal_noise`, `dissolve`, `noise_field`, `plasma`, `aurora`, `watercolor`, `torn_edge`, `topographic` |
| `edges` | Sobel, Canny and difference of Gaussians, with direction. | `neon_edges`, `sketch`, `line_art`, `toon`, `comic`, `blueprint`, `xray`, `woodcut`, `ascii_edges`, `engraving`, `crosshatch` |
| `palette` | Named palettes, k-means, nearest-colour mapping. Needs `colorspace`. | `palette_quantize`, `dither`, `pixel_art`, `risograph`, `pop_art`, `brick_mosaic`, `cross_stitch`, `led_wall`, `screenprint` |
| `warp` | Resample an image through a displacement field, with edge modes. | `swirl`, `pinch_bulge`, `ripple`, `displace`, `lens_distortion`, `fisheye`, `polar`, `kaleidoscope`, `shear`, `perspective`, `rotate`, `flow_warp`, `melt`, `droste`, `shatter`, `ken_burns`, `parallax` |
| `distance` | Signed distance from the subject's edge (exists inside the disperse helpers). | `outline`, `drop_shadow`, `long_shadow`, `rim_light`, `aura`, `sticker`, `layered_cutouts`, `silhouette` |
| `glyphs` | Glyph atlas and character-grid rendering (exists inside `ascii.py`). | `braille`, `half_blocks`, `ascii_edges`, `digital_rain`, `text_mosaic`, `typographic_portrait`, `barcode`, `cross_stitch`, `segments`, `text`, `type_on` |
| `cells` | Character-sized pieces (exists as `_pieces`). | `explode`, `shatter_fall`, `pixel_dissolve`, `melt_down`, `dissolve` |
| `voronoi` | Voronoi and Delaunay on scipy. | `voronoi_mosaic`, `low_poly`, `stipple`, `pointillism`, `shatter`, `voronoi_cells` |
| `sprites` | Draw many small alpha sprites with seeded placement and depth layers. | `rain`, `snow`, `sparkle`, `floating_dust`, `embers`, `lens_drops`, `digital_rain`, `stipple` |

Deliberately not here: blur and convolution. `scipy.ndimage` already is that module, and wrapping it adds an interface and nothing else.

## 3. Independent streams

One set of effects per stream, owned by one worker; each effect is its own package, tests, recipe and picture, so streams
share no file. Within a stream, build primitives first and presets last.

| Stream | Effects | Can start |
| --- | --- | --- |
| Colour and tone | `levels`, `curves`, `auto_levels`, `invert`, `posterize`, `solarize`, `threshold`, `fade`, `channel_mixer` | now |
| | `vibrance`, `selective_color`, `color_balance`, `white_balance`, `equalize` | after `colorspace` |
| Light and detail | `sharpen`, `highpass`, `denoise`, `vignette`, `gaussian_blur`, `motion_blur`, `radial_blur`, `lens_blur`, `bloom`, `chromatic_aberration`, `anamorphic_streaks`, `starburst`, `lens_flare`, `light_rays`, `shadows_highlights`, `dehaze`, `tone_map` | now (scipy only) |
| Texture and finish | `film_grain`, `noise`, `scanlines`, `pixel_grid`, `emboss`, `bump_light`, `border`; then `dust_scratches`, `paper_texture` | now; the latter two after `noise` |
| Glitch and signal | `rgb_split`, `jpeg_crush`, `scan_roll`, `interlace`, `echo`, `prism`, `anaglyph`, `dead_pixels`, `byte_corrupt`, `pixel_sort`, `slice_shift`, `stretch`, `scatter`, `signal_noise` | now; `signal_noise` after `noise` |
| Stylised looks | `pixelate`, `line_halftone`, `cmyk_halftone`, `oil_paint`, `led_wall`, `pointillism`; then `dither`, `edges`-based looks, `voronoi_mosaic`, `low_poly`, `stipple` | the first group now; the rest after `palette`, `edges`, `voronoi` |
| Distortion | `swirl`, `pinch_bulge`, `ripple`, `displace`, `lens_distortion`, `fisheye`, `polar`, `mirror`, `kaleidoscope`, `tile`, `shear`, `melt`, `flow_warp`, `shatter` | after `warp` (`mirror`, `tile` now) |
| Text and symbols | `ascii_edges`, `braille`, `half_blocks`, `digital_rain`, `text_mosaic`, `typographic_portrait`, `barcode`, `cross_stitch`, `segments` | after `glyphs` |
| Subject-aware | `outline`, `drop_shadow`, `long_shadow`, `rim_light`, `reflection`, `aura`; then `background`, `sticker`, `silhouette`, `color_pop`, `spotlight`, `double_exposure` | after `distance`; the second group after contract steps 2 and 3 |
| Atmosphere | `fog`, `rain`, `snow`, `smoke`, `sparkle`, `floating_dust`, `lens_drops`, `embers` | after `sprites` and `noise` |
| Generated backgrounds | `gradient`, `pattern`, `starfield`; then `noise_field`, `plasma`, `voronoi_cells`, `topographic`, `flow_lines`, `aurora` | the first group now; the rest after `noise` and `voronoi` |
| Compositing | `blend`, `overlay`, `gradient_overlay`, `text`, `split_screen`, `collage`, `lut`, `color_transfer`, `transition` | after contract steps 2 and 4 |
| Animation | `dissolve`, `pixel_dissolve`, `scan_reveal`, `wipe`, `type_on`, `explode`, `flicker`, `breathe`, `camera_shake`, `trails`, `slit_scan` | after `cells`; `trails` and `slit_scan` need effects that see earlier frames |
| Geometry | `crop`, `smart_crop`, `resize`, `pad`, `rotate`, `flip`, `circle_crop`, `rounded_corners`, `ken_burns` | after contract step 5 |
| Presets | `sepia`, `cyanotype`, `thermal`, `xray`, `vhs`, `crt`, `comic`, `toon`, `risograph`, `polaroid_frame`, and the rest of the compositions | last, after contract step 3 and the primitives they combine |

## 4. One effect, one task

Before coding, write a short spec card: the name, its parameters using the standard names, defaults, a one-sentence
description, the example recipe and any inputs it needs. Have it reviewed once, only for naming consistency; that is cheap and
it is what stops 150 independent efforts from diverging.

An effect is done when:

- it was started with `photo-fx new-effect` and lives in its own package, `src/photo_fx/effects/<effect>/`, with its
  metadata (category, example);
- `tests/effects/<effect>/test_<effect>.py` tests its defining property (for example, `pixel_sort` keeps every column's set of pixels; `mirror` is symmetric), not a golden image;
- its example recipe renders, and its gallery image is generated;
- the generic contract tests pass (shape, range, determinism, does not modify its input);
- no shared file was edited.

## 5. First wave

1. **Section 0**, all of it. It is the only thing that has to come before everything else.
2. **In parallel** with each other:
   - ~~split `recipe.py`~~ (done), then contract steps 1 and 2, one after the other;
   - the toolbox modules, each as its own task, starting with `colorspace`, `noise`, `edges`, `warp`;
   - two streams that need nothing, to prove the workflow: Colour and tone (the "now" group) and Glitch and signal.
3. The remaining streams as their toolbox modules land. Presets last.

Do **not** parallelise the contract steps (they all edit the runner), and do not start Presets or Compositing before the steps
they need.

## 6. A web app on top (later)

Not on the near-term path; recorded so the architecture does not close the door. The core stays Python and light. A browser app is a thin service and a thin UI over it, not a rewrite:

```
 browser UI (TypeScript)  ──  HTTP  ──  photo-fx serve (FastAPI)  ──  photo_fx core
  upload, step list,                     /effects  schema of every effect
  generated parameter panels,            /photos   upload; subject found once, cached
  live preview, per-step view            /render   photo + recipe + size (+ time) -> image
                                         /export   full-size and video as a background job
```

What this asks of the core (all of it also improves the developer experience):

1. **Effect metadata first**: parameter type, range, default and **unit** (`px`, `fraction`, `degrees`). It drives validation, docs, `sweep` and the generated parameter panels. It is contract step 6 above; a web app would need it first, but the web app is not on the near-term path, so it keeps its place in the order.
2. **Resolution-independent previews.** Pixel parameters (`cell`, `radius`, `split`) are relative to the recipe's `size`, so a 512 px preview with the same numbers looks different from the 1024 px result. With units in the metadata the runner scales them, so a preview is the same picture, smaller.
3. **A stage cache in the runner**: reuse every step's result while its inputs (photo, the steps before it, size) are unchanged. Dragging a slider on step 5 then recomputes only step 5. The runner already produces the stages; they only need keys.
4. **Recipes as plain data** (`Recipe` to and from a dict/JSON), so the UI edits a document and the server renders it.
5. **Optional install**: the server is an extra (`photo-fx[serve]`), so the core keeps numpy, scipy and Pillow only.

On this Mac, plain numpy at 512 px takes under 100 ms for 11 of the 13 effects (`clarity` and `glow`, about 150 ms, are slow only
because of their large blurs and can be sped up by blurring a smaller copy). A whole recipe is roughly 0.1 to 0.4 s per preview,
enough for previews while dragging and a full render on release.

Hosting adds three things: a cross-platform subject cut-out (Apple Vision exists only on macOS), a queue and limits (a preview
costs a fraction of a CPU second), and photo privacy (delete uploads after a short time, strip metadata, no accounts at first).

A TypeScript or WebGPU renderer is justified only by needs this design cannot meet: 60 fps dragging at high resolution, a
zero-install static demo, or per-user compute cost. If it ever comes, build it as a second renderer for the per-pixel effects,
used for live preview only, with Python authoritative for export. The two cannot match pixel for pixel (different random
generators and float behaviour).

