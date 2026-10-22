<h1 align="center">photo-fx</h1>

<p align="center">
  <b>A photo-effects pipeline you can script.</b><br>
  Describe a look as a short recipe, run it on one photo or a thousand, and get ASCII art, glitch, halftone,<br>
  neon grades and evaporating-particle animations, with caching, quality checks and an HTML report.
</p>

<p align="center">
  <img alt="Python 3.13+" src="https://img.shields.io/badge/python-3.13%2B-3776ab?logo=python&logoColor=white">
  <img alt="uv" src="https://img.shields.io/badge/built%20with-uv-de5fe9">
  <img alt="Tests" src="https://img.shields.io/badge/tests-pytest-2ea44f">
  <img alt="Code style" src="https://img.shields.io/badge/lint-ruff-261230">
  <img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue">
  <img alt="Platform" src="https://img.shields.io/badge/subject%20cut--out-macOS-lightgrey?logo=apple">
</p>

<p align="center">
  <img src="docs/images/banner.jpg" alt="One photo of a bull terrier turned into five styles: neon-pulse, glitch, ascii-colour, halftone-print and ember-glitch">
</p>

It can cut the subject (a person, a pet, an object) out of the photo first, so effects can target the subject or the
background separately.

<table>
  <tr>
    <td width="33%" valign="top"><b>Recipes are data</b><br>A look is a list of effects in TOML or Python. Changing a number is a one-line edit, and the same recipe runs on any photo.</td>
    <td width="33%" valign="top"><b>A real pipeline</b><br>Many photos x many recipes, cached on disk. One bad photo never stops the batch, and everything lands in a manifest and a browsable report.</td>
    <td width="33%" valign="top"><b>Developer friendly</b><br>Small pure-function effects, each with its category, example and gallery picture. Typos fail when a recipe is loaded, not halfway through a batch, and editors autocomplete every parameter.</td>
  </tr>
  <tr>
    <td width="33%" valign="top"><b>Transparent</b><br>See the picture after every step with <code>photo-fx steps</code>, so a look that is not right can be debugged, not guessed at.</td>
    <td width="33%" valign="top"><b>Checked automatically</b><br>A missed cut-out, a blank result or an animation that stops moving is flagged without anyone having to look.</td>
    <td width="33%" valign="top"><b>Animates too</b><br>Time-based effects render to video and a filmstrip PNG, with real easing curves and per-piece motion that stays continuous.</td>
  </tr>
</table>

## Gallery

Every picture here is made from one photo by the recipe linked next to it. Reproduce them with `photo-fx run examples/photo-fx.toml`.

<table>
  <tr>
    <td align="center" width="20%"><img src="examples/output/bullterrier/neon-pulse.jpg" alt="neon-pulse"></td>
    <td align="center" width="20%"><img src="examples/output/bullterrier/glitch.jpg" alt="glitch"></td>
    <td align="center" width="20%"><img src="examples/output/bullterrier/ascii-colour.jpg" alt="ascii-colour"></td>
    <td align="center" width="20%"><img src="examples/output/bullterrier/halftone-print.jpg" alt="halftone-print"></td>
    <td align="center" width="20%"><img src="examples/output/bullterrier/ember-glitch.jpg" alt="ember-glitch"></td>
  </tr>
  <tr>
    <td valign="top"><a href="examples/recipes/01-neon-pulse.toml"><b>neon-pulse</b></a><br><sub>Magenta-to-cyan grade, chromatic fringe, glow spilling onto black.<br><code>clarity</code> <code>gradient_map</code> <code>fill</code> <code>glitch</code> <code>glow</code></sub></td>
    <td valign="top"><a href="examples/recipes/02-glitch.toml"><b>glitch</b></a><br><sub>The subject on black, digitally damaged, with film grain.<br><code>fill</code> <code>glitch</code> <code>grain</code></sub></td>
    <td valign="top"><a href="examples/recipes/03-ascii-colour.toml"><b>ascii-colour</b></a><br><sub>Redrawn as text, every character in the photo's own colour.<br><code>grade</code> <code>clarity</code> <code>ascii</code> <code>fill</code></sub></td>
    <td valign="top"><a href="examples/recipes/04-halftone-print.toml"><b>halftone-print</b></a><br><sub>Newsprint dot screen in warm white ink.<br><code>clarity</code> <code>halftone</code> <code>fill</code></sub></td>
    <td valign="top"><a href="examples/recipes/05-ember-glitch.toml"><b>ember-glitch</b></a><br><sub>Fire palette, with some torn strips dissolving into text.<br><code>gradient_map</code> <code>fill</code> <code>glitch</code> <code>glow</code> <code>grain</code></sub></td>
  </tr>
</table>

The sixth demo recipe is an [animation](#animation).

## Contents

[Gallery](#gallery) | [Install](#install) | [Quick start](#quick-start) | [The pipeline](#the-pipeline) | [Recipes](#recipes) |
[Python API](#python-api) | [Seeing what each step does](#seeing-what-each-step-does) | [Animation](#animation) |
[Effects](#effects) | [Automatic checks](#automatic-checks) | [Project layout](#project-layout) | [Development](#development)

## Install

Requires Python 3.13+ and [uv](https://docs.astral.sh/uv/). `ffmpeg` is needed only for video output.

```bash
git clone https://github.com/jcakstins/photo-fx.git
cd photo-fx
uv sync
uv run photo-fx effects        # lists every effect and its parameters
```

**Platform.** Finding the subject uses Apple Vision's subject segmentation, so cut-outs, `mask = "background"` and
anything that needs a silhouette work on **macOS**. Every other effect runs anywhere. Subject finding is isolated in
[`matte.py`](src/photo_fx/matte.py), so another backend can be added without touching the rest.

## Quick start

The repository ships a demo: one photo and six looks (five stills and an animation).

```bash
uv run photo-fx run examples/photo-fx.toml
open examples/output/index.html
```

That processes `examples/photos/bullterrier.jpeg` with every recipe in `examples/recipes/` (about 45 s the first time). Run it
again and finished work is skipped; edit one recipe and only that one is redone. Add photos to `examples/photos/` and
they join the batch.

To start your own project:

```bash
uv run photo-fx init my-project            # photo-fx.toml, a photos/ folder, two starter recipes
cp ~/Pictures/me.jpg my-project/photos/
uv run photo-fx run my-project/photo-fx.toml
```

## The pipeline

```
 photos/*.jpg              recipes/*.toml  (or Recipe objects in Python)
      │                           │
      ▼                           ▼
 ┌─────────┐   ┌─────────┐   ┌─────────┐   ┌─────────┐   ┌─────────┐   ┌──────────┐
 │ ingest  │ → │ analyse │ → │  style  │ → │  check  │ → │ export  │ → │  report  │
 └─────────┘   └─────────┘   └─────────┘   └─────────┘   └─────────┘   └──────────┘
  orientation    find the      run the       cut-out       jpg/png/webp   manifest.json
  + colour       subject       recipe's      sensible?     mp4, several   index.html
  profile        (cached)      steps         motion?       sizes
```

| Stage | What it does |
| --- | --- |
| **ingest** | Reads the photo, applying EXIF rotation and colour profile, into one internal format (float RGB). |
| **analyse** | Finds the subject once per photo, only if a recipe needs it, and caches the result on disk by photo hash. |
| **style** | Runs each recipe's steps in order. Animated recipes render every frame. |
| **check** | Flags a cut-out that covers an implausible share of the photo, blank or blown-out results, and animations whose motion stops early. |
| **export** | Writes every format and size you asked for (animations as `.mp4`, plus a filmstrip PNG). |
| **report** | `manifest.json` (machine-readable) and `index.html` (browse everything, warnings included). |

Good batch behaviour is built in, and tested:

- **Incremental.** Each result is fingerprinted from the photo, the recipe, the export settings and the tool's own code. Unchanged work is skipped, and a missing output file is made again.
- **Resilient.** One photo or recipe failing is recorded and the rest carry on. The exit code is non-zero if anything failed (`--strict` also fails on warnings, for CI).
- **Safe to preview.** `--dry-run` shows what would run and writes nothing. `--only 'dog*.jpg'` limits a run to some photos. `--force` redoes everything.

The project file is `photo-fx.toml`:

```toml
photos  = ["photos/*.jpg", "photos/*.png"]   # globs, relative to this file
recipes = ["recipes"]                        # recipe files, or folders of them
output  = "output"

[export]
formats = ["jpg"]          # jpg, png or webp for stills; animations are always .mp4
sizes   = [2048, 1080]     # longest side in px; sizes larger than the render are never enlarged
quality = 92
steps   = false            # true: also save the picture after every step of every recipe

[checks]
subject_share = [0.02, 0.97]   # a cut-out outside this share of the photo is flagged
frozen_tail   = 0.3            # an animation frozen for more than this share of its length is flagged
```

## Recipes

A recipe is an ordered list of effects. Each `[[step]]` names an effect, and the other keys are that effect's parameters.

```toml
name = "ember-glitch"
size = 1024                       # longest side in px; effect sizes are relative to this

[[step]]
effect = "gradient_map"           # a fire palette
stops = ["#0a0000", "#e0261b", "#ffb347", "#fff3d6"]

[[step]]
effect = "fill"
color = "#000000"
mask = "background"               # "subject" or "background": apply only there

[[step]]
effect = "glitch"
split = 10
bands = 10
ascii_share = 0.5                 # some torn strips turn into text
seed = 4                          # same seed, same result, every time

[[step]]
effect = "glow"
opacity = 0.6                     # mix this step in at 60%
```

Removing a background is just a `fill` step masked to the background. Every step can take `mask` and `opacity`. Unknown
effects, misspelt parameters and bad values fail when the recipe loads, naming the file and the step.

## Python API

The same recipes can be written in Python with `fx`. Every effect is a function with its real signature and docstring, so
editors complete names and parameters (`fx.pyi` is generated by `photo-fx regenerate`).

```python
from photo_fx import Recipe, fx, imageio, render

photo = imageio.load("dog.jpg")

neon_glitch = Recipe(
    "neon-glitch",
    size=1024,
    steps=[
        fx.gradient_map(stops=["#12002b", "#ff2bd6", "#00f0ff"]),
        fx.fill("#000000").where("background"),  # only where the photo's background is
        fx.glitch(split=12, bands=8, seed=4),
        fx.glow(radius=26, amount=0.65).blend(0.8),  # mixed in at 80%
    ],
)

imageio.save(render(photo, neon_glitch), "dog-neon-glitch.jpg")
```

`fx.glitch(spilt=8)` raises at once and lists the valid parameters, the same as a typo in a TOML file does. Code-built
recipes can be handed straight to the pipeline: `run(Project(photos=(...), recipes=(neon_glitch,), output=...))`. See
[`examples/python_api.py`](examples/python_api.py) for a complete, runnable example.

## Seeing what each step does

When a look is not right, the cause is rarely the last step. Print the picture after every step:

```bash
uv run photo-fx steps examples/photos/bullterrier.jpeg examples/recipes/05-ember-glitch.toml
```

<p align="center">
  <img src="examples/output/steps/ember-glitch/_sheet.jpg" alt="Every step of the ember-glitch recipe">
</p>

The sheet shows the photo, the subject cut-out (white = subject), and the result of each step, with the exact parameters. In
Python it is `trace(photo, recipe)`, and `photo-fx run --steps` does it for a whole batch (linked from the report).
The last picture is exactly what `render` returns, because both run the same code.

## Animation

Add an `[animation]` table and any numeric parameter can change over the clip. `photo-fx run` (or `photo-fx animate`) renders
it to `.mp4` and also saves a filmstrip PNG, so the animation can be shown where a video cannot go.

<p align="center">
  <img src="examples/output/bullterrier/evaporate.frames.png" alt="Evaporation, six frames over five seconds">
</p>
<p align="center"><sub><code>06-evaporate.toml</code>: the dog, drawn as text, evaporates. Frames at 0, 1, 2, 3, 4 and 5 seconds.</sub></p>

```toml
[animation]
seconds = 5
fps = 30
hold_start = 0.5            # still frames before the motion starts
hold_end = 0.3

[[step]]
effect = "disperse"
progress = { from = 0.0, to = 0.88, ease = [0.25, 0.1, 0.75, 0.9] }   # a name or four cubic-bezier numbers
clock = { from = 0.0, to = 5.0, over = "clip" }                      # keeps running through the holds
wind = 0.3
angle = 30                  # degrees above the horizontal
```

The `disperse` effect cuts the picture into character-sized pieces, each with its own fixed path, so frames fit together.
The outline floats first, a front sweeps across, and each piece lets go when it arrives and is carried off on the wind. It
follows the motion-design principles of anticipation, easing and three layers of movement, and a test checks that no piece
travels more than a third of the picture. Easing can be `linear`, `in`, `out`, `in-out`, a cubic-bezier preset (`gentle`,
`standard`, `accelerate`, `decelerate`, `emphasized`, `sine`) or your own four numbers.

Generated videos are not committed to this repository: only the filmstrip is.

## Effects

Colour grades and gradient maps, light and texture, halftone, ASCII art, glitch, background removal and evaporating
particles, grouped by category. **[The effect catalogue](docs/effects.md)** shows every effect with a picture of it, a
one-line description, its parameters and defaults, and the example recipe that made the picture.
`uv run photo-fx effects` prints the full documentation of every parameter.

Adding an effect adds three files and edits none:

- `src/photo_fx/effects/<module>.py`: a pure function `def my_effect(rgb, ...) -> rgb` with the decorator
  `@effect(category="...")` (from `photo_fx.core.registry`), every parameter with a default. Its docstring becomes its
  documentation. Any module in [`src/photo_fx/effects/`](src/photo_fx/effects) is found automatically. Effects never
  import each other: what two of them share goes to `src/photo_fx/ops/`.
- `tests/effects/test_<module>.py`: tests of the property that defines it.
- `examples/effects/<effect>.toml`: a short example recipe (size 480), which also makes its picture in the catalogue.

The shared tests then check automatically that it keeps the image's shape and range, is deterministic, does not modify
its input, has a category, a docstring, defaults and an example recipe, and uses the shared parameter names (`amount`,
not `strength`).

## Automatic checks

So that a person does not have to look at every result, these run during every export and appear in the report. They
never block an export, and `--strict` turns them into a failing exit code:

| Check | Flags |
| --- | --- |
| Subject coverage | A cut-out covering under 2% or over 97% of the photo (the subject was missed, or there is no background to separate). |
| Blank or broken result | An all-black or all-white picture, a flat colour with no detail, or invalid pixel values. |
| Frozen tail | An animation whose motion stops early, so the last part of the clip is frozen (the file is fine, the motion died). |

## Project layout

```
src/photo_fx/                  layers, top to bottom; each imports only from the layers below it
  cli.py         app       the `photo-fx` command (no logic of its own)
  pipeline.py              many photos x many recipes: cache, checks, export, report
  gallery.py               what is generated from the effects: the catalogue page, its pictures, fx.pyi
  sheet.py                 contact sheets and step sheets
  checks.py                automatic quality checks
  fx.py                    the effects as validated step builders for Python (+ generated fx.pyi)
  recipe.py                the public namespace: the core's recipe names, with the subject finder bound to matte
  imageio.py     adapters  file formats, EXIF and colour profiles, video; defines the one in-memory image type
  matte.py                 subject_matte(rgb) -> alpha. Which model finds the subject is hidden here
  effects/       effects   the effect library: pure functions (rgb, **params) -> rgb, found automatically
  core/          core      registry.py (effects register here), types.py, vocabulary.py, and the recipe engine:
                           timing.py (easing, `ramp`, `Animation`), steps.py (`Step`, `Recipe`), loader.py (TOML),
                           runner.py (render, render_frames, render_all, trace; given the photo and the subject)
  ops/           ops       shared numeric primitives named by concept: colorspace, color, resample, textart, glyphs,
                           cells, distance, warp, voronoi... (core may import ops; ops imports nothing above it)
examples/        the demo project: one photo, six recipes, the committed results, a Python example
  effects/       one short example recipe per effect
docs/            the effect catalogue (effects.md, generated) and its pictures, the roadmap and build order
recipes/         a larger set of experiments
tests/
```

Each module owns one decision and callers never see it: `imageio` is the only place that knows about file formats,
`matte` the only place that knows about Vision, and effects know nothing about files, mattes or recipes. The layers are
import-linter contracts in `pyproject.toml`, checked by the test suite (`uv run lint-imports` runs them alone).

## Development

```bash
uv sync
uv run poe setup       # once per clone: installs the git hooks and the commit-message template
uv run poe check       # before every push or merge: format check, lint (ruff), types (mypy), the test suite
uv run poe fix         # applies the automatic lint and format fixes
uv run photo-fx render examples/photos/bullterrier.jpeg examples/effects/my_effect.toml   # look at it, in out/
uv run photo-fx regenerate   # at integration: fx.pyi, docs/effects.md, missing pictures
```

Commits follow [Conventional Commits](https://www.conventionalcommits.org/) (`type(scope): imperative subject`, header at
most 72 characters, body explaining why); a `commit-msg` hook checks it. The details are in [CLAUDE.md](CLAUDE.md#git-conventions).

Generated files (`src/photo_fx/fx.pyi`, `docs/effects.md`, `docs/gallery/`) are not updated on feature branches, so
branches that add effects never conflict over them. After merging, `photo-fx regenerate` brings them up to date in one
commit. Pictures that exist are kept (`photo-fx gallery --force` renders them again).

Lint rules (ruff) and the type check (mypy) are configured in `pyproject.toml`; `uv run mypy` runs the type check alone,
and the pre-push hook runs it with the tests.

Tests that need Apple Vision are skipped elsewhere, and tests that need `ffmpeg` are skipped when it is missing.

## License

[MIT](LICENSE) © 2026 Janis Cakstins
