"""Recipes: an ordered list of effects, kept in a small TOML file, and the function that runs one on a photo.

    name = "green-terminal"
    size = 1024                    # longest side in px; effect sizes (cell, radius...) are relative to this

    [[step]]
    effect = "ascii"               # any effect from portrait_fx.effects; the other keys are its parameters
    cell = 9
    fg = "#00ff9c"

    [[step]]
    effect = "fill"
    color = "#000000"
    mask = "background"            # "subject" or "background": apply only there (needs a person in the photo)
    opacity = 0.3                  # optional, 0..1: blend the effect in partway (default 1 = fully)

Removing the background is not a feature of its own: it is a `fill` step masked to the background.
"""

import inspect
import tomllib
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np

from . import effects, imageio
from .matte import subject_matte

_MASKS = ("subject", "background")


@dataclass(frozen=True)
class Step:
    effect: str
    params: dict = field(default_factory=dict)
    mask: str | None = None
    opacity: float = 1.0


@dataclass(frozen=True)
class Recipe:
    name: str
    steps: tuple[Step, ...]
    size: int | None = 1024

    @property
    def needs_matte(self) -> bool:
        """True if any step is masked, i.e. rendering must find the person first (the slow part)."""
        return any(step.mask for step in self.steps)


def load_recipe(path: str | Path) -> Recipe:
    """Read and validate a recipe file. Errors name the file, the step and the offending key."""
    path = Path(path)
    with open(path, "rb") as f:
        raw = tomllib.load(f)
    steps = []
    for i, entry in enumerate(raw.get("step", []), start=1):
        where = f"{path.name}, step {i}"
        entry = dict(entry)
        name = entry.pop("effect", None)
        mask = entry.pop("mask", None)
        opacity = entry.pop("opacity", 1.0)
        if name is None:
            raise ValueError(f"{where}: missing `effect`")
        if mask is not None and mask not in _MASKS:
            raise ValueError(f"{where}: mask must be one of {_MASKS}, got {mask!r}")
        if not isinstance(opacity, int | float) or not 0.0 <= opacity <= 1.0:
            raise ValueError(f"{where}: opacity must be a number from 0 to 1, got {opacity!r}")
        try:
            inspect.signature(effects.get(name)).bind(None, **entry)  # catches unknown or missing parameters now
        except (KeyError, TypeError) as error:
            raise ValueError(f"{where}: {error}") from None
        steps.append(Step(name, entry, mask, float(opacity)))
    if not steps:
        raise ValueError(f"{path.name}: a recipe needs at least one [[step]]")
    return Recipe(name=raw.get("name", path.stem), steps=tuple(steps), size=raw.get("size", 1024))


def render(rgb: np.ndarray, recipe: Recipe, matte: np.ndarray | None = None) -> np.ndarray:
    """Run `recipe` on the photo `rgb` and return the result as a new image.

    The photo is first scaled to `recipe.size`. Steps run in order, each on the previous step's output. A masked
    step is blended in through the person matte, so only the subject or only the background changes.
    `matte` (alpha for `rgb`, any size) can be passed in to avoid finding the person again when rendering several
    recipes from one photo; if a masked step needs it and it is missing, it is computed here.
    """
    if matte is not None and matte.shape[:2] != rgb.shape[:2]:
        raise ValueError(f"matte is {matte.shape[:2]} but the photo is {rgb.shape[:2]}")
    if recipe.size:
        rgb = imageio.resize(rgb, recipe.size)
        matte = None if matte is None else imageio.resize(matte, recipe.size)
    if recipe.needs_matte and matte is None:
        matte = subject_matte(rgb)

    out = rgb
    for step in recipe.steps:
        result = effects.apply(step.effect, out, **step.params)
        if result.shape != out.shape:
            raise RuntimeError(f"effect {step.effect!r} changed the image shape {out.shape} -> {result.shape}")
        if step.mask or step.opacity < 1.0:
            alpha = np.float32(step.opacity)
            if step.mask:
                alpha = alpha * (matte if step.mask == "subject" else 1.0 - matte)[..., None]
            result = out + (result - out) * alpha
        out = result
    return np.clip(out, 0.0, 1.0).astype(np.float32)


def render_all(rgb: np.ndarray, recipes: list[Recipe]) -> list[np.ndarray]:
    """Run several recipes on one photo; the person is found at most once, however many recipes need them."""
    matte = subject_matte(rgb) if any(r.needs_matte for r in recipes) else None
    return [render(rgb, recipe, matte) for recipe in recipes]
