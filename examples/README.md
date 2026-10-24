# examples

Everything here uses one photo, `photos/bullterrier.jpeg`.

| Folder or file | What it is |
| --- | --- |
| `bt-image.toml` | the demo project: every photo in `photos/` through every recipe in `recipes/` into `output/` (the layout `bt-image init` makes) |
| `recipes/` | finished looks, each a recipe that combines three or more effects |
| `output/bullterrier/` | the rendered pictures the main README shows (re-made by the command below) |
| `assets/` | pictures that recipes use as a second picture (a texture, a logo), made by the library itself, so they carry no licence; the commands are below |
| `effects/` | one short recipe per effect, the source of its catalogue picture in `docs/gallery/` |
| `experiments/` | older recipes, mostly variants of glitch, ASCII and evaporate; not part of the demo |
| `python_api.py` | the same pipeline driven from Python |

```bash
uv run bt-image run examples/bt-image.toml           # render every recipe (finished work is skipped next time)
open examples/output/index.html                      # the results and any quality warnings (xdg-open on Linux)
uv run bt-image sweep examples/photos/bullterrier.jpeg examples/recipes/glitch.toml glitch.split 0..20
uv run python examples/python_api.py
```

## Assets

Recipes name a second picture by a path relative to the recipe file (`texture = "../assets/grain-texture.png"`).
The two assets here were made with the library from the demo photo; run these from the repository root to make them again:

```bash
# bullterrier-cutout.png: the dog on a transparent background (the alpha channel is kept)
uv run bt-image cutout examples/photos/bullterrier.jpeg -o examples/assets/bullterrier-cutout.png --feather 1

# grain-texture.png: paper fibres around mid grey (assets/grain-texture.toml), made from a flat grey picture
uv run python -c "import numpy as np; from bullterrier_image import imageio; imageio.save(np.full((512, 512, 3), 0.5, np.float32), 'out/grey.png')"
uv run bt-image render out/grey.png examples/assets/grain-texture.toml -o out
mv out/grey__grain-texture.png examples/assets/grain-texture.png
```
