"""Writing effects for bullterrier-image in your own package: everything an effect author needs, in one place.

    import numpy as np
    from bullterrier_image.plugin import Color, EffectParams, effect, param

    class TintParams(EffectParams):
        \"\"\"Settings of the tint.\"\"\"

        colour: Color = param("#ff8800", doc="The colour mixed in.")
        amount: float = param(0.3, unit="fraction", min=0, max=1, doc="How much of it, 0..1. Raising it tints more.")

    @effect(category="colour", params=TintParams)
    def tint(rgb: np.ndarray, params: TintParams) -> np.ndarray:
        \"\"\"Mixes a flat colour into the picture.\"\"\"
        colour = np.asarray(params.colour, np.float32)
        return (rgb * (1 - params.amount) + colour * params.amount).astype(np.float32)

A parameter that is a second picture (a texture, a logo) is declared `image: Picture = param("#808080", doc="...")`:
it holds a `Picture` (read-only RGBA float32 `pixels`, straight alpha, at its own size) and its default is a colour,
which gives a flat 1x1 picture. A recipe file writes a path, `fx` takes a path, an array, a `Picture` or a colour.

Declare the module as an entry point of the group `bullterrier_image.effects` in your package's `pyproject.toml`, and
install the package (`pip install`, or `uv sync` for an editable checkout):

    [project.entry-points."bullterrier_image.effects"]
    my_effects = "my_package.effects"

bullterrier-image then finds the effect like its own, by name, in TOML recipes, `fx.tint(...)`, `bt-image effects` and
every other command: the registry imports every module of that group the first time it is asked about an effect. A
package that fails to import stops that query with a `BullterrierImageError` naming its entry point;
`BT_IMAGE_PLUGINS=builtin` loads only bullterrier-image's own effects.

The contract: an effect is a pure function of a float32 RGB image `(h, w, 3)` in 0..1 and its parameter model, returning
a new image of the same shape, deterministic (randomness from a `seed` field), never modifying its input. Every
parameter is a `param` field with a description; a number also has a unit (px, fraction, degrees...; an unknown one is
refused naming the valid ones) and an inclusive range holding its default. `category` is one of `CATEGORIES`. Mistakes
in a definition raise when the module is imported. An effect that needs the subject's silhouette declares
`*, matte: np.ndarray | None = None` (float32 (h, w), 0..1, 1 = subject); one that needs how near each pixel is declares
`*, depth: np.ndarray | None = None` (float32 (h, w), 0..1, 1 = nearest); one that draws along the borders of a
partition declares
`*, partition: np.ndarray | None = None` (int32 piece numbers, one per pixel, 0..n-1 in piece order: reading order,
`depth_bands` nearest first; a step names the region with its `partition` key). An effect meant to be mixed in
partway declares the opacity its steps have by default, `@effect(category=..., params=..., opacity=0.7)` (0..1, default
1); a step's own `opacity` replaces it. No
parameter may be named like an input (`matte`, `depth`, `partition`) or a key of the step (`effect`, `mask`, `opacity`,
`blend_mode`, `preset`, `partition`, `each`, `looks`). Names must be unique across every installed package.
"""

from .core.params import Color, EffectParams, Picture, param
from .core.registry import CATEGORIES, effect

__all__ = ["CATEGORIES", "Color", "EffectParams", "Picture", "effect", "param"]
