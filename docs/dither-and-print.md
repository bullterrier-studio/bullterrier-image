# Dither, halftone and print looks

Planned 2026-11-15, not scheduled; the order is at the end. Gaps found by comparing the catalogue with the dither and halftone
tools in common use. Their "dozens of algorithms" are mostly variants (a kernel, a matrix size, a direction), so most rows
below extend `dither` or `halftone`, by the rule of [effects-roadmap.md](effects-roadmap.md): a new look extends an existing
effect when every one of its parameters still matters for every look. The ecosystem view (who owns what across packages) is
the design repository's `dither-and-print.md`.

## `dither`: methods

Today: `floyd_steinberg`, `atkinson`, `bayer` (8 x 8), `blue_noise`. How the parameters are shaped (one longer `method` list,
or `method` plus `matrix`) is the batch's decision. How much of the pattern shows is `pattern` already.

| Addition | What it is |
|---|---|
| Diffusion kernels | Jarvis-Judice-Ninke, Stucki, Burkes, Sierra, two-row Sierra, Sierra Lite, Stevenson-Arce: each a weight table on the one diffusion path. |
| `serpentine` | Rows scanned in alternating direction: removes the diagonal worms of diffusion. |
| Ostromoukhov | Diffusion whose weights change with the tone (Ostromoukhov, 2001): fewer artefacts in mid-tones. |
| Dot diffusion | Error passed in the order of a class matrix (Knuth, 1987; Guo-Liu and Mese matrices): parallel, no worms. |
| Riemersma | Diffusion along a space-filling curve (Hilbert; Peano) with a short error memory. |
| Threshold matrices | Bayer 2 x 2 and 4 x 4 (8 x 8 today), clustered dot (straight and 45 degrees), white noise, a four-shade camera set (the Game Boy Camera look; the `gameboy` palette exists), and a caller's matrix as a `picture` parameter. |

## `dither`: modes

| Addition | What it is |
|---|---|
| Attribute cells | The palette search limited per block: each n x n block (8 by default) gets two colours of the palette, ink and paper, and dithers between them (ZX Spectrum colour clash; MSX rows of 8 x 1, C64 hires as presets). |
| Pattern tiles | Tone picks one tile from an ordered set (classic 2 x 2, weave, brick, grid, fishnet, spiral), the set a preset or a caller's picture strip. |
| `phase` | A state that shifts the threshold or noise tile by a step. A caller rendering a series sets it per picture: the pattern holds still or moves. With `seed`, the same for diffusion's noise. Keeping a dither steady over a moving sequence needs the previous picture and is not image's. |
| Named palettes as data | The named palettes are `Palette` variants today. A library of dozens belongs in `data/presets`, like the gradient maps, not in an enum. |

Tone maps (a curve before the dither) need no parameter: a `curves` or `levels` step before `dither` is one. The gap is
presets (below).

## `halftone`

Today: a rotated square grid; `shape` dot (round) or line; `separation` none, cmyk or spot; offset misregistration.

| Addition | What it is |
|---|---|
| Dot shapes | Square, ellipse (with an aspect), diamond, euclidean (round, square at mid tone, as print screens), and a caller's spot function as a `picture` of one cell. |
| Screen geometry | The screen laid in other coordinates: wave (rows displaced by a sine), rings (concentric about `x`, `y`), spiral, rays. With `shape` line these are the wavy-line, ring, spiral and sunburst looks. One coordinate map before the existing screen; every other parameter keeps its meaning. |
| Tone moves the line | With `shape` line, tone displaces the line instead of widening it: ridgelines and waveforms. Check against the rule above when built; a new effect if half the parameters stop mattering. |
| RGB separation | Three additive screens (red, green, blue) on black, each at its own angle, beside cmyk and spot. |
| Print realism | Dot gain (dots grow with ink spread), uneven ink (seeded noise in coverage), paper specks. Also for `dither`'s `inks`. |

A ruling in lines per inch is a size question: `cell` px = the print's DPI over the ruling, scaled to the recipe's size.
Documented with the DPI write option, no parameter.

## New effects

| Effect | What it does |
|---|---|
| `modulation` | Rows (or columns, or diagonals) of dashes whose length or pitch carries tone: one-dimensional error diffusion along each row. `sway` bends the rows, `weave` alternates their direction, `dash` sets a length. Its parameters mean nothing to the 2-D methods, so not a `dither` method. |
| `mosaic` | The picture rebuilt from caller-given tile pictures, each cell the tile nearest in mean colour (Oklab), optionally tinted. No tile art is bundled (design decision): tiles are a `picture` list or folder. |
| `stipple`, `crosshatch`, `engraving` | Rows in [effects-roadmap.md](effects-roadmap.md) ("Stylised looks"), moved from "On demand" to planned here. |

Photocopy is a look, not an effect: `grade` contrast, `dither` threshold, `grain`, `displace`, `dust_scratches`.

## Presets

| Addition | Where |
|---|---|
| Gradient maps | `data/presets/gradient_map.toml`: 5 today; dither tools ship well over a hundred. Check whether stops should interpolate in Oklab as well as sRGB. |
| Palettes | Named palettes as data (see `dither`: modes). |
| Tone curves | `curves` presets: contrast, fade, crush, film-like shoulders. |

## Data beside the picture (design change)

Image gives a picture. These give the data a caller needs to print or trace what an effect made; recorded in the design
repository's `image.md`.

| Addition | Why |
|---|---|
| Ink coverage planes | `halftone` and `dither` with `separation` already compute a coverage per ink. Handed out as one `Layer` per ink, scene writes printing plates. |
| Palette-index map | `dither`'s colour index per pixel, as `Labels` (the type regions use): vector traces it into SVG regions, one group per colour. |
| Character grid as text | `ascii`'s characters as a string, with colours for ANSI or HTML: copy-paste text, and real text in SVG through vector. Like `palette()`, a function beside the recipe. |
| DPI on write | `files::write` takes an optional resolution: the PNG `pHYs` chunk, the JPEG density. Never in a recipe. Scene's PNG goes through this writer and passes its DPI on. |

Whether these come from a run option or from functions beside the recipe is the batch's decision.

## Finders

An object detector: boxes, labels and scores for one picture, an ONNX model kind in `finding`, permissively licensed only.
It gives data; drawing the boxes is vector's and scene's, following them over a sequence is video's.

## Order

1. `dither` methods and matrices; `serpentine`.
2. `halftone` dot shapes, screen geometry, RGB separation, print realism.
3. `modulation`, `stipple`, `crosshatch`, `engraving`; attribute cells, pattern tiles, `phase`.
4. Presets: gradient maps, palettes as data, tone curves.
5. Data beside the picture, when scene's plates or vector's tracing is built.
6. `mosaic`, the detector.
