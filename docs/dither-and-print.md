# Dither, halftone and print looks

Planned 2026-11-15, not scheduled; the order is at the end. Gaps found by comparing the catalogue with the dither and halftone
tools in common use. Their "dozens of algorithms" are mostly variants (a kernel, a matrix size, a direction), so most rows
below extend `dither` or `halftone`, by the rule of [effects-roadmap.md](effects-roadmap.md): a new look extends an existing
effect when every one of its parameters still matters for every look. The ecosystem view (who owns what across packages) is
the design repository's `dither-and-print.md`.

## `dither`: methods

Today: `floyd_steinberg`, `atkinson`, `jarvis_judice_ninke`, `stucki`, `burkes`, `sierra`, `sierra_two_row`, `sierra_lite`,
`stevenson_arce`, `ostromoukhov`, `dot_diffusion`, `riemersma`, `bayer2`, `bayer4`, `bayer` (8 x 8), `clustered`, `clustered45`,
`blue_noise`, `white_noise`, `gameboy_camera`, `matrix` (a picture of thresholds), and `serpentine`. One `method` list; how much of
the pattern shows is `pattern`. Done 2026-11-16, bar the later items below.

| Addition | What it is |
|---|---|
| ~~Diffusion kernels~~ **Done** (2026-11-16) | Jarvis-Judice-Ninke, Stucki, Burkes, Sierra, two-row Sierra, Sierra Lite, Stevenson-Arce: each a weight table on the one diffusion path. |
| ~~`serpentine`~~ **Done** (2026-11-16) | Rows scanned in alternating direction: removes the diagonal worms of diffusion. |
| ~~Ostromoukhov~~ **Done** (2026-11-16) | Diffusion whose weights change with the tone (Ostromoukhov, 2001): fewer artefacts in mid-tones. |
| Dot diffusion | Error passed in the order of a class matrix (Knuth, 1987): parallel, no worms. **Done** (2026-11-16) with the class matrix quoted in Knuth's 1987 paper (his program `dot-diff.w` builds a variant differing in about 20 cells); later: the Guo-Liu and Mese-Vaidyanathan class matrices. |
| Riemersma | Diffusion along a space-filling curve with a short error memory. **Done** (2026-11-16) along the Hilbert curve; later: the Peano curve. |
| Camera four-shade set | The Game Boy Camera's three thresholds per cell (four shades from one matrix, at its sixteen contrast settings). `gameboy_camera` uses the cell order only, as a black-and-white matrix. |

## `dither`: modes

| Addition | What it is |
|---|---|
| ~~Attribute cells~~ **Done** (2026-11-16) | The palette search limited per block: each n x n block (8 by default) gets two colours of the palette, ink and paper, and dithers between them (ZX Spectrum colour clash; MSX rows of 8 x 1, C64 hires as presets). |
| ~~Pattern tiles~~ **Done** (2026-11-16, `method="tiles"` with a caller's strip; the bundled sets wait for presets that can name a picture) | Tone picks one tile from an ordered set (classic 2 x 2, weave, brick, grid, fishnet, spiral), the set a preset or a caller's picture strip. |
| ~~`phase`~~ **Done** (2026-11-16, ordered and noise methods; diffusion ignores it) | A state that shifts the threshold or noise tile by a step. A caller rendering a series sets it per picture: the pattern holds still or moves. With `seed`, the same for diffusion's noise. Keeping a dither steady over a moving sequence needs the previous picture and is not image's. |
| ~~Named palettes as data~~ **Done** (2026-11-17) | `crates/core/data/palettes.toml`: a table per palette, the build writes `dither`'s `Palette` choices from it (20 named: hardware, greys, colour schemes). The cubes stay in code (a formula). Open: the `palette` doc lists the names by hand (a test fails when one is missing). |

Tone maps (a curve before the dither) need no parameter: a `curves` or `levels` step before `dither` is one. The gap is
presets (below).

## `halftone`

Today: a rotated screen of a fractional `cell`, on a grid, wave, rings, spiral or rays; `shape` dot, square, ellipse, diamond,
euclidean, matrix or line; `separation` none, cmyk, spot or rgb; offset misregistration; dot gain, ink noise and paper specks.

| Addition | What it is |
|---|---|
| ~~Dot shapes~~ **Done** (2026-11-16) | Square, ellipse (with an aspect), diamond, euclidean (round, a diamond at mid tone, as print screens), and a caller's spot function as a `picture` of one cell. |
| ~~Screen geometry~~ **Done** (2026-11-16) | The screen laid in other coordinates: wave (rows displaced by a sine), rings (concentric about `x`, `y`), spiral, rays. With `shape` line these are the wavy-line, ring, spiral and sunburst looks. One coordinate map before the existing screen; every other parameter keeps its meaning. |
| Tone moves the line | With `shape` line, tone displaces the line instead of widening it: ridgelines and waveforms. Check against the rule above when built; a new effect if half the parameters stop mattering. |
| ~~RGB separation~~ **Done** (2026-11-16) | Three additive screens (red, green, blue) on black, each at its own angle, beside cmyk and spot. |
| ~~Fractional `cell`~~ **Done** (2026-11-16) | `cell` is a length in px: a fine screen no longer jumps between sizes. |
| ~~Print realism~~ **Done** (2026-11-16) | Dot gain (dots grow with ink spread), uneven ink (seeded noise in coverage), paper specks. Also for `dither`'s `spot`. |

One idea, one name: both effects call separate inks `separation="spot"` (`dither` said `"inks"`; renamed). **Done** (2026-11-16).

Open, later: "Tone moves the line" (above).

A ruling in lines per inch is a size question: `cell` px = the print's DPI over the ruling, scaled to the recipe's size.
Documented with the DPI write option, no parameter.

## New effects

| Effect | What it does |
|---|---|
| ~~`modulation`~~ **Done** (2026-11-16) | Rows (or columns, or diagonals) of dashes whose length or pitch carries tone: one-dimensional error diffusion along each row. `sway` bends the rows, `weave` alternates their direction, `dash` sets a length. Its parameters mean nothing to the 2-D methods, so not a `dither` method. |
| `mosaic` | The picture rebuilt from caller-given tile pictures, each cell the tile nearest in mean colour (Oklab), optionally tinted. No tile art is bundled (design decision): tiles are a `picture` list or folder. |
| ~~`stipple`, `crosshatch`, `engraving`~~ **Done** (2026-11-16) | Rows in [effects-roadmap.md](effects-roadmap.md) ("Stylised looks"), moved from "On demand" to planned here. |

Photocopy is a look, not an effect: `grade` contrast, `dither` threshold, `grain`, `displace`, `dust_scratches`.

## Presets

| Addition | Where |
|---|---|
| ~~Gradient maps~~ **Done** (2026-11-17) | `data/presets/gradient_map.toml`: 30 curated (tints, natural, scientific scales, duotones). `space` = `oklab` mixes stops in Oklab (default `rgb`): measured, lightness steps along thermal's ramp vary 2.8x in rgb and 1.75x in Oklab, and blue-to-yellow dips in lightness in rgb but steps evenly in Oklab. |
| ~~Palettes~~ **Done** (2026-11-17) | See `dither`: modes. |
| ~~Tone curves~~ **Done** (2026-11-17) | `curves` presets: `low_contrast`, `contrast`, `strong_contrast`, `fade`, `matte`, `crush`, `lift_shadows`, `film`, `shoulder`. Open: a toe/shoulder with more than five points; per-channel film stocks. |

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

1. ~~`dither` methods and matrices; `serpentine`.~~ **Done** (2026-11-16); open: Guo-Liu and Mese matrices, Peano curve, the four-shade camera set.
2. ~~`halftone` dot shapes, screen geometry, RGB separation, print realism.~~ **Done** (2026-11-16); open: see the later items under `halftone`.
3. ~~`modulation`; attribute cells, pattern tiles, `phase`.~~ **Done** (2026-11-16, batch A); batch B (`stipple`, `crosshatch`, `engraving`) **Done** (2026-11-16).
4. ~~Presets: gradient maps, palettes as data, tone curves.~~ **Done** (2026-11-17); open: see the table above.
5. Data beside the picture, when scene's plates or vector's tracing is built.
6. `mosaic`, the detector.
