# Dither, halftone and print looks

This file records what shipped. Planned work is tracked outside this repository.

## `dither`: methods

Today: `floyd_steinberg`, `atkinson`, `jarvis_judice_ninke`, `stucki`, `burkes`, `sierra`, `sierra_two_row`, `sierra_lite`,
`stevenson_arce`, `ostromoukhov`, `dot_diffusion`, `riemersma`, `bayer2`, `bayer4`, `bayer` (8 x 8), `clustered`, `clustered45`,
`blue_noise`, `white_noise`, `gameboy_camera`, `matrix` (a picture of thresholds), and `serpentine`. One `method` list; how much of
the pattern shows is `pattern`. Done 2026-11-16.

| Addition | What it is |
|---|---|
| ~~Diffusion kernels~~ **Done** (2026-11-16) | Jarvis-Judice-Ninke, Stucki, Burkes, Sierra, two-row Sierra, Sierra Lite, Stevenson-Arce: each a weight table on the one diffusion path. |
| ~~`serpentine`~~ **Done** (2026-11-16) | Rows scanned in alternating direction: removes the diagonal worms of diffusion. |
| ~~Ostromoukhov~~ **Done** (2026-11-16) | Diffusion whose weights change with the tone (Ostromoukhov, 2001): fewer artefacts in mid-tones. |
| Dot diffusion | Error passed in the order of a class matrix (Knuth, 1987): parallel, no worms. **Done** (2026-11-16) with the class matrix quoted in Knuth's 1987 paper (his program `dot-diff.w` builds a variant differing in about 20 cells). |
| Riemersma | Diffusion along a space-filling curve with a short error memory. **Done** (2026-11-16) along the Hilbert curve. |

## `dither`: modes

| Addition | What it is |
|---|---|
| ~~Attribute cells~~ **Done** (2026-11-16) | The palette search limited per block: each n x n block (8 by default) gets two colours of the palette, ink and paper, and dithers between them (ZX Spectrum colour clash; MSX rows of 8 x 1, C64 hires as presets). |
| ~~Pattern tiles~~ **Done** (2026-11-16, `method="tiles"` with a caller's strip; the bundled sets wait for presets that can name a picture) | Tone picks one tile from an ordered set (classic 2 x 2, weave, brick, grid, fishnet, spiral), the set a preset or a caller's picture strip. |
| ~~`phase`~~ **Done** (2026-11-16, ordered and noise methods; diffusion ignores it) | A state that shifts the threshold or noise tile by a step. A caller rendering a series sets it per picture: the pattern holds still or moves. With `seed`, the same for diffusion's noise. Keeping a dither steady over a moving sequence needs the previous picture and is not image's. |
| ~~Named palettes as data~~ **Done** (2026-11-17) | `crates/core/data/palettes.toml`: a table per palette, the build writes `dither`'s `Palette` choices from it (20 named: hardware, greys, colour schemes). The cubes stay in code (a formula). `bullterrier_image.named_palettes()` (Python) and `Palette::NAMED` with `colours()` and `description()` (Rust) list them; `palette` names a palette only in its doc's words, never in a list. |

Tone maps (a curve before the dither) need no parameter: a `curves` or `levels` step before `dither` is one.

## `halftone`

Today: a rotated screen of a fractional `cell`, on a grid, wave, rings, spiral or rays; `shape` dot, square, ellipse, diamond,
euclidean, matrix or line; `separation` none, cmyk, spot or rgb; offset misregistration; dot gain, ink noise and paper specks.

| Addition | What it is |
|---|---|
| ~~Dot shapes~~ **Done** (2026-11-16) | Square, ellipse (with an aspect), diamond, euclidean (round, a diamond at mid tone, as print screens), and a caller's spot function as a `picture` of one cell. |
| ~~Screen geometry~~ **Done** (2026-11-16) | The screen laid in other coordinates: wave (rows displaced by a sine), rings (concentric about `x`, `y`), spiral, rays. With `shape` line these are the wavy-line, ring, spiral and sunburst looks. One coordinate map before the existing screen; every other parameter keeps its meaning. |
| ~~RGB separation~~ **Done** (2026-11-16) | Three additive screens (red, green, blue) on black, each at its own angle, beside cmyk and spot. |
| ~~Fractional `cell`~~ **Done** (2026-11-16) | `cell` is a length in px: a fine screen no longer jumps between sizes. |
| ~~Print realism~~ **Done** (2026-11-16) | Dot gain (dots grow with ink spread), uneven ink (seeded noise in coverage), paper specks. Also for `dither`'s `spot`. |

One idea, one name: both effects call separate inks `separation="spot"` (`dither` said `"inks"`; renamed). **Done** (2026-11-16).

A ruling in lines per inch is a size question: `cell` px = the print's DPI over the ruling, scaled to the recipe's size.
Documented with the DPI write option, no parameter.

## GPU

Where a GPU render (`Gpu`, `render(..., gpu=)`) runs each look on the device; the rest runs on the CPU inside it, counted. Bounds and
reasons: design-decisions, "GPU backend".

| Look | GPU | CPU |
|---|---|---|
| `dither` method | `bayer2`, `bayer4`, `bayer`, `clustered`, `clustered45`, `blue_noise`, `white_noise`, `gameboy_camera`, `matrix`, with any palette (`photo` included) | the error-diffusion methods, `riemersma`, `dot_diffusion`, `tiles` (a pixel depends on earlier ones) |
| `dither` mode | `phase`, `pattern`, `cell` | `block` attribute cells, `separation="spot"` |
| `halftone` screen | `grid`, `wave` | `rings`, `spiral`, `rays` |
| `halftone` separation | `none`, `cmyk`, `rgb`, `spot`; shapes, misregistration, fractional `cell`, dot gain, ink noise | |
| `halftone` paper | | `specks` |

## New effects

| Effect | What it does |
|---|---|
| ~~`modulation`~~ **Done** (2026-11-16) | Rows (or columns, or diagonals) of dashes whose length or pitch carries tone: one-dimensional error diffusion along each row. `sway` bends the rows, `weave` alternates their direction, `dash` sets a length. Its parameters mean nothing to the 2-D methods, so not a `dither` method. |
| ~~`stipple`, `crosshatch`, `engraving`~~ **Done** (2026-11-16) | Rows in [effects-roadmap.md](effects-roadmap.md) ("Stylised looks"). |

Photocopy is a look, not an effect: `grade` contrast, `dither` threshold, `grain`, `displace`, `dust_scratches`.

## Presets

| Addition | Where |
|---|---|
| ~~Gradient maps~~ **Done** (2026-11-17) | `data/presets/gradient_map.toml`: 30 curated (tints, natural, scientific scales, duotones). `space` = `oklab` mixes stops in Oklab (default `rgb`): measured, lightness steps along thermal's ramp vary 2.8x in rgb and 1.75x in Oklab, and blue-to-yellow dips in lightness in rgb but steps evenly in Oklab. |
| ~~Palettes~~ **Done** (2026-11-17) | See `dither`: modes. |
| ~~Tone curves~~ **Done** (2026-11-17) | `curves` presets: `low_contrast`, `contrast`, `strong_contrast`, `fade`, `matte`, `crush`, `lift_shadows`, `film`, `shoulder`. |

## Data beside the picture

Image gives a picture. These give the data a caller needs to print or trace what an effect made.

| Addition | Why |
|---|---|
| Ink coverage planes | `halftone` and `dither` with `separation` already compute a coverage per ink. Handed out as one `Layer` per ink, scene writes printing plates. |
| Palette-index map | `dither`'s colour index per pixel, as `Labels` (the type regions use): vector traces it into SVG regions, one group per colour. |
| Character grid as text | `ascii`'s characters as a string, with colours for ANSI or HTML: copy-paste text, and real text in SVG through vector. Like `palette()`, a function beside the recipe. |
| DPI on write | `write_with(path, image, &WriteOptions)` takes an optional resolution (`WriteOptions::with_dpi`): the PNG `pHYs` chunk, the JPEG density. Never in a recipe. Scene's PNG goes through this writer and passes its DPI on. |

**Done (2026-11-17):** functions beside the recipe that ask its last step (`Photo::ink_planes`, `palette_map`, `characters`; Python `ink_planes`, `palette_map`, `characters`), and `write_with(path, image, &WriteOptions)` (`imageio.save(..., dpi=)`). Reasons and limits: design-decisions, "Data beside the picture".
