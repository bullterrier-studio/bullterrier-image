# portrait-fx

Cut a person out of a photo and style the result with composable effects: clean black background, ASCII art,
halftone, glitch, colour schemes. Built for profile pictures; written as a small library so other apps can reuse it.

```bash
uv sync                                                    # once
uv run portrait-fx cutout  inputs/me.jpg                   # just you, transparent background -> out/cutout.png
uv run portrait-fx render  inputs/me.jpg recipes           # every recipe in the folder -> out/me__<recipe>.png
uv run portrait-fx sheet   inputs/me.jpg recipes           # all of them side by side -> out/sheet.png
uv run portrait-fx animate inputs/me.jpg recipes/36-anim-right-to-right.toml   # an animated recipe -> out/me__<name>.mp4 (add --max-side 1080 for a smaller copy)
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

## Animation

Add an `[animation]` table to a recipe and any numeric parameter can change over the clip. `portrait-fx animate` renders
it to .mp4 (needs ffmpeg) or .gif; `render` shows a still of an animated recipe at its start.

```toml
[animation]
seconds = 6
fps = 30
hold_start = 1.2          # frames held before the motion starts, and after it ends
hold_end = 1.2

[[step]]
effect = "disperse"
progress = { from = 0.0, to = 0.95, ease = "sine" }       # ease: a name or [x1, y1, x2, y2] (cubic-bezier)
clock = { from = 0.0, to = 6.0, over = "clip" }            # over = "clip": keeps running through the holds
```

`disperse` takes its `progress` and gives every character its own fixed path, so frames fit together. `sweep` (-1 = the
front starts at the right edge, +1 = at the left; `top` and `edge` can add "higher first" and "outline first") sets
the order in which pieces let go; `reach` is how much of the figure ever goes. The wind blows at `angle` degrees above
the horizontal, to the right for a positive `wind` (also the farthest a piece travels, as a share of the width). `hover`
makes the outline float slowly along the wind first, and each piece lets go when the front reaches it. In
`36-anim-right-to-right.toml` the right part goes first and everything is blown right and up at 30 degrees, through
space the right side has already cleared.

The animation recipes follow the [motion-design skill](https://github.com/lottiefiles/motion-design-skill) (installed
in `~/.claude/skills/motion-design`): a declared personality and easing, an anticipation beat, and three layers:
primary (the sweep), secondary (pieces pull back before they go and glow as they leave) and ambient (a slow
shimmer, running through the holds, against the wind). Its UI-scale numbers are adapted, not applied literally: a
particle swarm is one "element" crossing the picture, so the 1/3 rules are checked as "no piece travels more than a
third of the width" and "at most a third of the pieces are in flight at once" (both tested), and the 500 ms
stagger budget is stretched to the length of the clip. The first frame is a still to use where motion is unwanted.

Check an animation's pacing by measuring it, not just by looking at stills: decode the clip and plot how much changes
between frames. A slow-tailed ease (like a sine curve) plus a long final hold leaves the last third frozen; an
ease that is gentle at the ends but steady in the middle, such as `[0.25, 0.1, 0.75, 0.9]`, keeps it moving.
Videos use a keyframe every second; 1600 px square needs H.264 level 5.0, so `--max-side 1080` makes a copy that
plays almost everywhere.

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
    evaporate.py   evaporate (a still: pieces of the top/outline of the picture drift off and fade)
    disperse.py    disperse (the same idea over time: for animations)
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
- `evaporate` moves whole characters, so give it the same `cell` as the `ascii` step before it. It needs a cut-out on
  black (put the background `fill` before it) and only uses the room already in the frame: pieces that rise or drift
  past the edge are simply gone. Pieces detach from the top down (`start`/`end`) and/or from the figure's outline
  (`edge` = depth of the zone as a fraction of the width). `outward` makes them fly away from the edge they came
  from (left side sheds left, right side right), `wind` leans everything one way, `vanish` drops some early so the
  plume is sparse. The outline comes from the person matte, which the recipe passes in by itself. Keep `edge` modest
  (0.05-0.08): at 0.12 and above so much of the head breaks up that the face stops reading.
- Pure ASCII in the photo's colours: leave brightness to `ascii`'s `gamma` (lower = denser = brighter) and do not use
  `grade` exposure first, which washes the hue out of the characters. A little `saturation` keeps skin warm.
