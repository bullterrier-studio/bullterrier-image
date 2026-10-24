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
`*, matte: np.ndarray | None = None`. Names must be unique across every installed package.
"""

from .core.params import Color, EffectParams, param
from .core.registry import CATEGORIES, effect

__all__ = ["CATEGORIES", "Color", "EffectParams", "effect", "param"]
