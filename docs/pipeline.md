# Pipeline and command line

## Command line

`bt-image --help` lists commands (`render`, `sheet`, `sweep`, `cutout`, `run`, `steps`, `init`, `effects`);
`bt-image COMMAND --help` shows options and an example. `bt-image --version` prints version; `python -m bullterrier_image` =
same program when script not on `PATH`. Results to stdout, progress and errors to stderr. Mistake -> one line
`error: ...`, exit 1 (usage mistakes 2, Ctrl-C 130). `-q` hides progress. Traceback: `--debug` or `BT_IMAGE_DEBUG=1`.

`bt-image effects` lists every effect and its main parameters; `--advanced` shows all, `--category NAME` one category,
`--list-categories` the categories with their number of effects, `NAME` one effect in full.

`sweep` shows what one number does: recipe at five values (`--count`, 2 to 24), side by side, labelled. Target = number's
address: `effect.parameter`, `effect#2.parameter` for effect used twice. Range starting below zero goes after `--`:
`... -o out/sweep.png -- -2..2`.

```
uv run bt-image sweep examples/photos/bullterrier.jpeg glitch rgb_split.amount 0..20
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
  + colour       subject       recipe's      sensible?     several        index.html
  profile        (cached)      steps                       sizes
```

Subject and depth found once per photo, only if a recipe needs them, cached by photo hash and finder identity. Image reading
and writing details (EXIF, ICC, 16-bit): [Images in and out](#images-in-and-out).

- **Incremental.** Result fingerprinted from photo, recipe, export settings, tool's own code. Unchanged work skipped; missing
  output file made again.
- **Resilient.** One photo or recipe failing is recorded, rest carry on. Exit code non-zero if anything failed (`--strict`:
  also on warnings, for CI).
- **Safe to preview.** `--dry-run` shows what would run, writes nothing. `--only 'dog*.jpg'` limits photos. `--force` redoes all.

Project file `bt-image.toml`:

```toml
photos  = ["photos/*.jpg", "photos/*.png"]   # globs, relative to this file
recipes = ["recipes"]                        # recipe files, or folders of them
output  = "output"

[export]
formats = ["jpg"]          # jpg, png or webp
sizes   = [2048, 1080]     # longest side in px; sizes larger than the render are never enlarged
quality = 92
steps   = false            # true: also save the picture after every step of every recipe

[checks]
subject_share = [0.02, 0.97]   # a cut-out outside this share of the photo is flagged
```

**Automatic checks** run on every export, appear in report. Never block export; `--strict` makes them fail the exit code.

| Check | Flags |
| --- | --- |
| Subject coverage | Cut-out under 2% or over 97% of photo (subject missed, or no background to separate). |
| Blank or broken result | All-black or all-white picture, flat colour with no detail, invalid pixel values. |

## Images in and out

`bullterrier_image.imageio`:

- **Reading.** Anything Pillow opens (JPEG, PNG, WebP, TIFF, GIF, BMP...), any mode: 8-bit RGB, greyscale, 1-bit, palette,
  CMYK; 16-bit greyscale at full precision. Transparency composited over white. Animation (GIF, APNG) gives first frame.
- **Colour.** Embedded ICC profile (Display P3, CMYK press, grey gamma) converted to sRGB. No usable profile, and 16-bit
  greyscale: taken as sRGB (the web's rule for untagged images).
- **Errors.** Missing, non-image, truncated or oversized file -> `ImageReadError` naming it. Pillow decompression-bomb limit
  kept: over about 179 megapixels (twice `PIL.Image.MAX_IMAGE_PIXELS`) refused; raise it yourself for big files you trust.
- **Writing.** Stills embed a small sRGB profile, nothing else: no EXIF, no GPS, no timestamp, so same picture -> same file.
  JPEG/WebP quality = project's `quality` export setting (92 unless set; also `bt-image steps`), or 95 for `imageio.save`,
  `bt-image render`, `sheet`, `cutout`. JPEGs keep full-resolution colour.

## Seeing what each step does

Look not right? Cause rarely last step. Print picture after every step:

```bash
uv run bt-image steps examples/photos/bullterrier.jpeg examples/recipes/ember-glitch.toml
```

<p align="center">
  <img src="../examples/output/steps/ember-glitch/_sheet.jpg" alt="Every step of the ember-glitch recipe">
</p>

Sheet (`out/steps/<photo>__<recipe>/`, one picture per step) shows photo, subject cut-out (white = subject), result of each
step with exact parameters. Python: `trace(photo, recipe)`. Whole batch: `bt-image run --steps`. Last picture is exactly what
`render` returns: same code.
