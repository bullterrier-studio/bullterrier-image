# examples

Everything here uses one photo, `photos/bullterrier.jpeg`.

| Folder or file | What it is |
| --- | --- |
| `bt-image.toml` | the demo project: every photo in `photos/` through every recipe in `recipes/` into `output/` (the layout `bt-image init` makes) |
| `recipes/` | finished looks, each a recipe that combines three or more effects |
| `output/bullterrier/` | the rendered pictures the main README shows (re-made by the command below) |
| `effects/` | one short recipe per effect, the source of its catalogue picture in `docs/gallery/` |
| `experiments/` | older recipes, mostly variants of glitch, ASCII and evaporate; not part of the demo |
| `python_api.py` | the same pipeline driven from Python |

```bash
uv run bt-image run examples/bt-image.toml           # render every recipe (finished work is skipped next time)
open examples/output/index.html                      # the results and any quality warnings (xdg-open on Linux)
uv run bt-image sweep examples/photos/bullterrier.jpeg examples/recipes/glitch.toml glitch.split 0..20
uv run python examples/python_api.py
```
