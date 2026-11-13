# examples

Everything here uses one photo, `photos/bullterrier.jpeg`.

| Folder or file | What it is |
| --- | --- |
| `bt-image.toml` | the demo project: every photo in `photos/` through every recipe in `recipes/` into `output/` (the layout `bt-image init` makes) |
| `recipes/` | finished looks, each a recipe that combines three or more effects |
| `regions/` | recipes that use regions (recipe format 2; `windows` and `panes`, format 3): `glass` (shattered glass), `torn` (torn scraps), `one-cell` (one cell of an uneven grid), `grid-first` (the subject found cell by cell), `diagonal` (one straight cut), `tear` (a torn cut with a solid band), `bezier-cut` (a curved cut), `bezier-shape` (a closed path as a mask), `svg` (shapes of `assets/mosaic.svg` as pieces), `windows` (a synthesised facade: each window a connected piece, no photo) and `panes` (panes found by colour; run on `assets/facade.png`); the subject is found where a recipe uses it, and they are not part of the demo project |
| `fields/` | recipes that cut or drive by a field (recipe format 3), none needing a model: `tilt-shift` (a blur whose radius a gradient drives), `shadow-grain` (grain driven by a band of the picture's own light) and `tonal-grade` (a look per band of light); [the pictures](../docs/gallery/fields/README.md) |
| `output/bullterrier/` | the rendered pictures the main README shows (re-made by the command below) |
| `transparent/` | recipes whose picture is cut out (a circle, rounded corners, the subject): their render has an alpha channel, so it is a PNG, and a project that exports `jpg` refuses them; the picture is made with the command below, not by the demo project |
| `assets/` | pictures that recipes use as a second picture (a texture, a logo) or, for `regions/panes.toml`, as the photo (`facade.png`), made by the library itself, so they carry no licence (the commands are below), `mosaic.svg`, a drawing of closed shapes made for the examples, and `warm-film.cube`, a colour lookup table made by the command below; all free of licence |
| `effects/` | one short recipe per effect, the source of its catalogue picture in `docs/gallery/` |
| `experiments/` | older recipes, mostly variants of glitch, ASCII and disperse; not part of the demo |
| `python_api.py` | the same pipeline driven from Python |

```bash
uv run bt-image run examples/bt-image.toml           # render every recipe (finished work is skipped next time)
open examples/output/index.html                      # the results and any quality warnings (xdg-open on Linux)
uv run bt-image sweep examples/photos/bullterrier.jpeg glitch rgb_split.amount 0..20
uv run python examples/python_api.py
```

## Transparent looks

A recipe with `transparent` in its `[frame]` table renders to a picture with an alpha channel. `transparent/avatar.toml` crops
a square around the subject and cuts a circle; run this from the repository root to make `output/bullterrier/avatar.png`
again:

```bash
uv run bt-image render examples/photos/bullterrier.jpeg examples/transparent/avatar.toml -o out
mv out/bullterrier__avatar.png examples/output/bullterrier/avatar.png
```

## Assets

Recipes name a second picture by a path relative to the recipe file (`image = "../assets/grain-texture.png"`).
The assets here were made with the library, so none carries a licence; run these from the repository root to make them again:

```bash
# bullterrier-cutout.png: the dog on a transparent background (the alpha channel is kept); cut with Apple Vision, the
# macOS default
uv run bt-image cutout examples/photos/bullterrier.jpeg -o examples/assets/bullterrier-cutout.png --feather 1 --segmenter vision

# grain-texture.png: paper fibres around mid grey (assets/grain-texture.toml), made from a flat grey picture
uv run python -c "import numpy as np; from bullterrier_image import imageio; imageio.save(np.full((512, 512, 3), 0.5, np.float32), 'out/grey.png')"
uv run bt-image render out/grey.png examples/assets/grain-texture.toml -o out
mv out/grey__grain-texture.png examples/assets/grain-texture.png

# night-sky.jpg: aurora and stars (assets/night-sky.toml), made from a flat grey picture of the photo's shape
uv run python -c "import numpy as np; from bullterrier_image import imageio; imageio.save(np.full((768, 614, 3), 0.5, np.float32), 'out/sky.png')"
uv run bt-image render out/sky.png examples/assets/night-sky.toml -o out --format jpg
mv out/sky__night-sky.jpg examples/assets/night-sky.jpg

# facade.png: a brick wall and eight blue-grey panes, brighter and darker than the brick, with a little noise, for
# regions/panes.toml
uv run python -c "
import numpy as np
from bullterrier_image import imageio
brick = np.array([0x8A, 0x3B, 0x2E]) / 255
panes = np.array([[0x2A, 0x35, 0x44], [0xA9, 0xBC, 0xCF], [0x3C, 0x4E, 0x63], [0x7F, 0x93, 0xA8], [0xC5, 0xD3, 0xE0], [0x1F, 0x2A, 0x38], [0x5A, 0x6B, 0x7D], [0x45, 0x57, 0x6B]]) / 255
picture = np.tile(brick, (360, 480, 1))
for number, colour in enumerate(panes):
    top, left = 40 + 160 * (number // 4), 30 + 112 * (number % 4)
    picture[top:top + 120, left:left + 82] = colour
noise = np.random.default_rng(0).uniform(-0.015, 0.015, picture.shape)
imageio.save(np.clip(picture + noise, 0, 1).astype(np.float32), 'examples/assets/facade.png')
"
```

```bash
# warm-film.cube: a 17-point lookup table, half an S-curve with red up and blue down and lifted
uv run python -c "
from itertools import product
n = 17
curve = lambda v: 0.5 * v + 0.5 * v * v * (3 - 2 * v)  # half an S-curve: a little more contrast
rows = []
for b, g, r in product(range(n), repeat=3):  # .cube order: red changes fastest
    r, g, b = (curve(c / (n - 1)) for c in (r, g, b))
    rows.append(f'{min(1, r * 1.08 + 0.02):.6f} {g:.6f} {b * 0.88 + 0.04:.6f}')  # warm: red up, blue down and lifted
open('examples/assets/warm-film.cube', 'w', encoding='utf-8').write(f'TITLE \"warm film\"\nLUT_3D_SIZE {n}\n' + '\n'.join(rows) + '\n')
"
```
