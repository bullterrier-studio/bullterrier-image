# portrait-fx

Cut a person out of a photo and style the result with composable effects: clean black background, ASCII art,
halftone, glitch, colour schemes. Built for profile pictures; written as a small library so other apps can reuse it.

```bash
uv sync                                                    # once
uv run portrait-fx cutout  inputs/me.jpg                   # just you, transparent background -> out/cutout.png
uv run portrait-fx render  inputs/me.jpg recipes           # every recipe in the folder -> out/me__<recipe>.png
uv run portrait-fx sheet   inputs/me.jpg recipes           # all of them side by side -> out/sheet.png
uv run portrait-fx effects                                 # what effects exist and their parameters
uv run pytest                                              # tests
```

The person is found with Apple Vision, so cutouts and anything using `mask` need **macOS**. Effects without a mask
work anywhere.

## Recipes

A recipe is a small TOML file: an ordered list of effects. Copy one from `recipes/`, change numbers, run `sheet`.

```toml
name = "ascii-green"
size = 1024                  # longest side in px; effect sizes are relative to this

[[step]]
effect = "ascii"             # any effect from `portrait-fx effects`; other keys are its parameters
cell = 5
fg = "#00ff9c"

[[step]]
effect = "fill"
color = "#000000"
mask = "background"          # "subject" | "background": apply only there
```

Removing the background is not a special feature, just a `fill` step masked to the background. Typos in effect
or parameter names are reported at load time, with the file and step number.

## Layout

```
src/portrait_fx/
  imageio.py     file formats, EXIF/colour profiles, resizing; defines the one in-memory image type
  matte.py       subject_matte(rgb) -> alpha. Which model finds the person is hidden here
  effects/       the effect library: pure functions (rgb, **params) -> rgb
    color.py       fill, grade, clarity, gradient_map, hue_shift
    ascii.py       ascii
    texture.py     halftone, glow, grain
    glitch.py      glitch
  recipe.py      TOML recipes; render(), render_all()
  sheet.py       comparison sheets
  cli.py         the `portrait-fx` command (no logic of its own)
recipes/         example looks (data, not code)
tests/
inputs/ out/     your photos and the renders; git-ignored
```

Each module owns one decision, and callers never see it: `imageio` is the only place that knows about file
formats and colour profiles, `matte` is the only place that knows about Vision, and effects know nothing about files,
mattes or recipes.

## Using it from another app

```python
from portrait_fx import imageio, load_recipe, render_all

photo = imageio.load("me.jpg")
looks = [load_recipe(p) for p in ("recipes/01-clean-black.toml", "recipes/06-glitch.toml")]
for look, image in zip(looks, render_all(photo, looks)):  # finds the person once for all recipes
    imageio.save(image, f"me-{look.name}.png")
```

Add it to another uv project with `uv add --editable ../portrait-fx`.

## Adding an effect

Write a function in any module under `effects/` and decorate it with `@effect`; import the module in
`effects/__init__.py`. It takes the image and keyword parameters (with defaults) and returns an image of the same
shape. Its docstring is what `portrait-fx effects` shows, so describe every parameter there. The shared tests
automatically check that it keeps the shape, dtype and 0..1 range, is deterministic and does not modify its input.

## Tips from tuning

- `ascii` and `halftone` turn *tone* into marks, so a face in a narrow band of brightness turns to mush. Put
  `grade` and `clarity` before them (see `recipes/02-ascii-green.toml`) and use small cells (5-6 px at 1024).
- A faint coloured fringe along the hair after replacing the background means the matte is too generous: raise
  `shrink` in `subject_matte` (default 1.5 px).
- `glitch` with `ascii_share` turns some of the sliding strips into text in the photo's colours. `seed` decides where
  the strips land, so try several seeds (`portrait-fx sheet` makes that quick) until none cut across the eyes or mouth.
  Put the colour scheme (`gradient_map`, `hue_shift`) *before* the glitch and the glow *after* it.
