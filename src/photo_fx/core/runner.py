"""Running a `Recipe` on a photo: `render` (one still), `render_frames` (an animation), `render_all` (several recipes
on one photo) and `trace` (every intermediate picture, as `Stage`s).

Promises: the input photo is never modified; the same photo, recipe and matte always give the same pixels; `trace`'s
last stage is exactly what `render` returns, because both run the same code; an effect that changes the image shape is
an error. The runner is given everything it uses: the photo, and the subject either as a `matte` or as a
`find_subject` function, which it calls at most once per render (once per photo in `render_all`) and only if some step
needs the subject. It never loads a file or looks for the subject on its own; `photo_fx.recipe` binds the default
finder for users.

Dependency direction: imports `steps`, the effect registry and `ops` (to scale the photo); never `loader`, so recipes
built in Python run without the TOML reader.
"""

from collections import deque
from collections.abc import Callable, Iterator, Sequence
from dataclasses import dataclass

import numpy as np

from photo_fx.ops.resample import resize

from . import registry
from .steps import Recipe, Step

# Finds the subject of a photo: float32 (h, w, 3) 0..1 -> its alpha, float32 (h, w) 0..1 (white = subject).
SubjectFinder = Callable[[np.ndarray], np.ndarray]


def render(
    rgb: np.ndarray,
    recipe: Recipe,
    matte: np.ndarray | None = None,
    at: float = 0.0,
    *,
    find_subject: SubjectFinder | None = None,
) -> np.ndarray:
    """Run `recipe` on the photo `rgb` and return the result as a new image.

    The photo is first scaled to `recipe.size`. Steps run in order, each on the previous step's output. A masked
    step is blended in through the subject matte, so only the subject or only the background changes.
    Effects that declare a `matte` parameter (see `evaporate`) get the subject's silhouette automatically.

    For an animated recipe this is one still: the frame at position `at` (0..1) of the clip.

    rgb: the photo, float32 (h, w, 3) in 0..1. recipe: what to run. matte: the subject's alpha, float32 with the photo's
    height and width, 0..1 (white = subject), or None. at: position in the clip, 0..1 (ignored by a still recipe).
    find_subject: called with the photo (already scaled to `recipe.size`) when a step needs the subject and no `matte`
    was given. Raises ValueError when a step needs the subject and neither was given, or the finder returns None.
    """
    rgb, matte = _prepare(rgb, recipe, matte, find_subject)
    return _finish(_run(rgb, recipe.steps, matte, at, at))


def render_frames(
    rgb: np.ndarray, recipe: Recipe, matte: np.ndarray | None = None, *, find_subject: SubjectFinder | None = None
) -> Iterator[np.ndarray]:
    """Yield every frame of an animated recipe, in order (see `Animation.positions`).

    The steps before the first animated one cannot change between frames, so they run once for the whole clip, and the
    subject is found once for the whole clip. A recipe with an animation but no animated parameter gives the same still
    picture in every frame.

    rgb, matte, find_subject: as for `render`. recipe: must have an animation (ValueError otherwise).
    """
    if recipe.animation is None:
        raise ValueError(f"recipe {recipe.name!r} has no animation: add an [animation] table (seconds, fps)")
    rgb, matte = _prepare(rgb, recipe, matte, find_subject)
    # With no animated step every step is still: the clip is the still picture, held for every frame.
    first_moving = next((i for i, step in enumerate(recipe.steps) if step.animated), len(recipe.steps))
    still = _run(rgb, recipe.steps[:first_moving], matte, 0.0, 0.0)
    for motion, clip in recipe.animation.frames():
        yield _finish(_run(still, recipe.steps[first_moving:], matte, motion, clip))


@dataclass(frozen=True)
class Stage:
    """One picture from inside a render: what the photo looked like at some point on its way through the recipe."""

    label: str
    image: np.ndarray


def trace(
    rgb: np.ndarray,
    recipe: Recipe,
    matte: np.ndarray | None = None,
    at: float = 0.0,
    *,
    find_subject: SubjectFinder | None = None,
) -> list[Stage]:
    """Every stage of a render, so you can see what each step did and where something went wrong.

    The stages are the photo as the recipe receives it, the subject matte if the recipe uses one (white = subject),
    and the image after each step, in order. The last stage is exactly what `render` returns, because both run the
    same code. For an animated recipe the stages are for the frame at position `at` (0..1) of the clip.

    rgb, recipe, matte, at, find_subject: as for `render`.
    """
    rgb, matte = _prepare(rgb, recipe, matte, find_subject)
    stages = [Stage("input", rgb)]
    if recipe.needs_matte and matte is not None:  # `_prepare` gives a matte whenever the recipe needs one
        stages.append(Stage("subject matte (white = subject)", np.repeat(matte[..., None], 3, axis=2)))
    for number, (step, image) in enumerate(_stages(rgb, recipe.steps, matte, at, at), start=1):
        stages.append(Stage(f"{number}. {step.describe()}", _finish(image)))
    return stages


def render_all(
    rgb: np.ndarray, recipes: list[Recipe], *, find_subject: SubjectFinder | None = None
) -> list[np.ndarray]:
    """Run several recipes on one photo; the subject is found at most once, however many recipes need it.

    rgb: the photo (see `render`). recipes: the recipes to run; the results come back in the same order, one still each.
    find_subject: called once with the photo as given when any recipe needs the subject (ValueError if it is needed
    and missing).
    """
    masked = next((recipe for recipe in recipes if recipe.needs_matte), None)
    matte = None if masked is None else _find_subject(rgb, masked, find_subject)
    return [render(rgb, recipe, matte) for recipe in recipes]


def _prepare(
    rgb: np.ndarray, recipe: Recipe, matte: np.ndarray | None, find_subject: SubjectFinder | None
) -> tuple[np.ndarray, np.ndarray | None]:
    """Scale the photo and matte to the recipe's size, and find the subject if a step needs it."""
    if matte is not None and matte.shape[:2] != rgb.shape[:2]:
        raise ValueError(f"matte is {matte.shape[:2]} but the photo is {rgb.shape[:2]}")
    if recipe.size:
        rgb = resize(rgb, recipe.size)
        matte = None if matte is None else resize(matte, recipe.size)
    if recipe.needs_matte and matte is None:
        matte = _find_subject(rgb, recipe, find_subject)
    return rgb, matte


def _find_subject(rgb: np.ndarray, recipe: Recipe, find_subject: SubjectFinder | None) -> np.ndarray:
    if find_subject is None:
        raise ValueError(
            f"recipe {recipe.name!r} has masked steps, so it needs the subject: pass `matte` (the subject's alpha) "
            "or `find_subject` (a function photo -> alpha, e.g. photo_fx.matte.subject_matte)"
        )
    return subject_from(find_subject, rgb)


def subject_from(find_subject: SubjectFinder, rgb: np.ndarray) -> np.ndarray:
    """Ask `find_subject` for the subject of `rgb` and return its matte. The one place a finder is called and its answer
    checked (the runner and the pipeline both use it): ValueError if it returned None, since a finder must answer or
    raise, and carrying on would drop the subject without a word."""
    matte = find_subject(rgb)
    if matte is None:
        raise ValueError(
            "`find_subject` returned None: it must return the subject's alpha (float32, the photo's height and width, "
            "0..1) or raise when it finds none"
        )
    return matte


def _run(out: np.ndarray, steps: Sequence[Step], matte: np.ndarray | None, motion: float, clip: float) -> np.ndarray:
    last = deque(_stages(out, steps, matte, motion, clip), maxlen=1)  # only the final stage is kept in memory
    return last[0][1] if last else out


def _stages(
    out: np.ndarray, steps: Sequence[Step], matte: np.ndarray | None, motion: float, clip: float
) -> Iterator[tuple[Step, np.ndarray]]:
    """Run the steps in order, yielding each step with the image after it. The one place a step is applied."""
    for step in steps:
        extra = {"matte": matte} if step.wants_matte else {}
        result = registry.apply(step.effect, out, **step.params_at(motion, clip), **extra)
        if result.shape != out.shape:
            raise RuntimeError(f"effect {step.effect!r} changed the image shape {out.shape} -> {result.shape}")
        if step.mask or step.opacity < 1.0:
            alpha: np.floating | np.ndarray = np.float32(step.opacity)
            if step.mask:
                if matte is None:  # not reachable through the public functions: `_prepare` finds it for masked steps
                    raise ValueError(
                        f"step {step.effect!r} is masked to the {step.mask}, but there is no subject matte"
                    )
                alpha = alpha * (matte if step.mask == "subject" else 1.0 - matte)[..., None]
            result = out + (result - out) * alpha
        out = result
        yield step, out


def _finish(out: np.ndarray) -> np.ndarray:
    return np.clip(out, 0.0, 1.0).astype(np.float32)
