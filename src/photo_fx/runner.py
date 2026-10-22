"""Running a `Recipe` on a photo: `render` (one still), `render_frames` (an animation), `render_all` (several recipes
on one photo) and `trace` (every intermediate picture, as `Stage`s).

Promises: the input photo is never modified; the same photo, recipe and matte always give the same pixels; `trace`'s
last stage is exactly what `render` returns, because both run the same code; an effect that changes the image shape is
an error. The subject is found at most once per photo (`render_all`) and only if some step needs it.

Dependency direction: imports `steps` (and `timing` through it) and never `loader`, so recipes built in Python run
without the TOML reader.
"""

from collections import deque
from collections.abc import Iterator
from dataclasses import dataclass

import numpy as np

from . import effects, imageio
from .matte import subject_matte
from .steps import Recipe, Step


def render(rgb: np.ndarray, recipe: Recipe, matte: np.ndarray | None = None, at: float = 0.0) -> np.ndarray:
    """Run `recipe` on the photo `rgb` and return the result as a new image.

    The photo is first scaled to `recipe.size`. Steps run in order, each on the previous step's output. A masked
    step is blended in through the subject matte, so only the subject or only the background changes.
    Effects that declare a `matte` parameter (see `evaporate`) get the subject's silhouette automatically.
    `matte` (alpha for `rgb`, any size) can be passed in to avoid finding the subject again when rendering several
    recipes from one photo; if a masked step needs it and it is missing, it is computed here.

    For an animated recipe this is one still: the frame at position `at` (0..1) of the clip.

    rgb: the photo, float32 (h, w, 3) in 0..1. recipe: what to run. matte: the subject's alpha, float32 with the photo's
    height and width, 0..1 (white = subject), or None. at: position in the clip, 0..1 (ignored by a still recipe).
    """
    rgb, matte = _prepare(rgb, recipe, matte)
    return _finish(_run(rgb, recipe.steps, matte, at, at))


def render_frames(rgb: np.ndarray, recipe: Recipe, matte: np.ndarray | None = None) -> Iterator[np.ndarray]:
    """Yield every frame of an animated recipe, in order (see `Animation.positions`).

    The steps before the first animated one cannot change between frames, so they run once for the whole clip.

    rgb, matte: as for `render`. recipe: must have an animation (ValueError otherwise).
    """
    if recipe.animation is None:
        raise ValueError(f"recipe {recipe.name!r} has no animation: add an [animation] table (seconds, fps)")
    rgb, matte = _prepare(rgb, recipe, matte)
    first_moving = next(i for i, step in enumerate(recipe.steps) if step.animated)
    still = _run(rgb, recipe.steps[:first_moving], matte, 0.0, 0.0)
    for motion, clip in recipe.animation.frames():
        yield _finish(_run(still, recipe.steps[first_moving:], matte, motion, clip))


@dataclass(frozen=True)
class Stage:
    """One picture from inside a render: what the photo looked like at some point on its way through the recipe."""

    label: str
    image: np.ndarray


def trace(rgb: np.ndarray, recipe: Recipe, matte: np.ndarray | None = None, at: float = 0.0) -> list[Stage]:
    """Every stage of a render, so you can see what each step did and where something went wrong.

    The stages are the photo as the recipe receives it, the subject matte if the recipe uses one (white = subject),
    and the image after each step, in order. The last stage is exactly what `render` returns, because both run the
    same code. For an animated recipe the stages are for the frame at position `at` (0..1) of the clip.

    rgb, recipe, matte, at: as for `render`.
    """
    rgb, matte = _prepare(rgb, recipe, matte)
    stages = [Stage("input", rgb)]
    if recipe.needs_matte:
        stages.append(Stage("subject matte (white = subject)", np.repeat(matte[..., None], 3, axis=2)))
    for number, (step, image) in enumerate(_stages(rgb, recipe.steps, matte, at, at), start=1):
        stages.append(Stage(f"{number}. {step.describe()}", _finish(image)))
    return stages


def render_all(rgb: np.ndarray, recipes: list[Recipe]) -> list[np.ndarray]:
    """Run several recipes on one photo; the subject is found at most once, however many recipes need it.

    rgb: the photo (see `render`). recipes: the recipes to run; the results come back in the same order, one still each.
    """
    matte = subject_matte(rgb) if any(r.needs_matte for r in recipes) else None
    return [render(rgb, recipe, matte) for recipe in recipes]


def _prepare(rgb: np.ndarray, recipe: Recipe, matte: np.ndarray | None) -> tuple[np.ndarray, np.ndarray | None]:
    """Scale the photo and matte to the recipe's size, and find the subject if a step needs it."""
    if matte is not None and matte.shape[:2] != rgb.shape[:2]:
        raise ValueError(f"matte is {matte.shape[:2]} but the photo is {rgb.shape[:2]}")
    if recipe.size:
        rgb = imageio.resize(rgb, recipe.size)
        matte = None if matte is None else imageio.resize(matte, recipe.size)
    if recipe.needs_matte and matte is None:
        matte = subject_matte(rgb)
    return rgb, matte


def _run(out: np.ndarray, steps: tuple[Step, ...], matte: np.ndarray | None, motion: float, clip: float) -> np.ndarray:
    last = deque(_stages(out, steps, matte, motion, clip), maxlen=1)  # only the final stage is kept in memory
    return last[0][1] if last else out


def _stages(out, steps, matte, motion, clip) -> Iterator[tuple[Step, np.ndarray]]:
    """Run the steps in order, yielding each step with the image after it. The one place a step is applied."""
    for step in steps:
        extra = {"matte": matte} if step.wants_matte else {}
        result = effects.apply(step.effect, out, **step.params_at(motion, clip), **extra)
        if result.shape != out.shape:
            raise RuntimeError(f"effect {step.effect!r} changed the image shape {out.shape} -> {result.shape}")
        if step.mask or step.opacity < 1.0:
            alpha = np.float32(step.opacity)
            if step.mask:
                alpha = alpha * (matte if step.mask == "subject" else 1.0 - matte)[..., None]
            result = out + (result - out) * alpha
        out = result
        yield step, out


def _finish(out: np.ndarray) -> np.ndarray:
    return np.clip(out, 0.0, 1.0).astype(np.float32)
