"""Running a `Recipe` on a photo: `render` (one still), `render_all` (several recipes on one photo) and `trace` (every
intermediate picture, as `Stage`s).

Promises: the input photo is never modified; the same photo, recipe and matte always give the same pixels; `trace`'s
last stage is exactly what `render` returns, because both run the same code; an effect that changes the image shape is
an error; `render` keeps alive only the image the next step needs, so a render's memory peak is its busiest step's
own (that step's input, scratch and output) plus the scaled matte, whatever the number of steps, while `trace` keeps
every stage. The runner is given everything it uses: the photo, and the subject either as a
`matte` or as a `find_subject` function, which it calls at most once per render (once per photo in `render_all`) and
only if the recipe needs the subject. It never loads a file or looks for the subject on its own;
`bullterrier_image.recipe` binds the default finder for users. The subject is found on the photo as given, before the
recipe's `Framing` is applied, so a crop can follow the subject and the matte moves with the photo; the framed photo is
then scaled to the recipe's size.

Dependency direction: imports `steps`, the effect registry and `ops` (to scale the photo); never the recipe-file
reader `loader`, so recipes built in Python run without it.
"""

from collections import deque
from collections.abc import Callable, Iterator, Sequence
from dataclasses import dataclass

import numpy as np

from bullterrier_image.errors import SubjectNotFoundError
from bullterrier_image.ops.resample import resize

from . import registry
from .steps import Recipe, Step

# Finds the subject of a photo: float32 (h, w, 3) 0..1 -> its alpha, float32 (h, w) 0..1 (white = subject).
SubjectFinder = Callable[[np.ndarray], np.ndarray]


def render(
    rgb: np.ndarray,
    recipe: Recipe,
    matte: np.ndarray | None = None,
    *,
    find_subject: SubjectFinder | None = None,
) -> np.ndarray:
    """Run `recipe` on the photo `rgb` and return the result as a new image.

    The photo is first framed (`recipe.frame`: flip, turn, crop, borders) and scaled to `recipe.size`; the subject
    matte is framed and scaled with it. Steps run in order, each on the previous step's output. A masked
    step is blended in through the subject matte, so only the subject or only the background changes.
    Effects that declare a `matte` parameter (see `evaporate`) get the subject's silhouette automatically. After the
    last step a recipe whose framing is `transparent` is cut out: the result then has four channels, RGB and straight
    alpha (h, w, 4), instead of three.

    rgb: the photo, float32 (h, w, 3) in 0..1. recipe: what to run. matte: the subject's alpha, float32 with the photo's
    height and width, 0..1 (white = subject), or None. find_subject: called with the photo as given (before it is
    framed or scaled) when the recipe needs the subject and no `matte` was given. Raises SubjectNotFoundError when the
    recipe needs the subject and neither was given, the finder returns None, the matte is not the photo's size, or the
    framing crops around a subject that is not there.
    """
    image, matte = _prepare(rgb, recipe, matte, find_subject)
    stages = _stages(image, recipe.steps, matte)
    del image  # the stages own the scaled photo now, and let it go once the first step has run
    result = _last(stages)
    return recipe.frame.cut(result, matte) if recipe.frame.has_alpha else result


@dataclass(frozen=True)
class Stage:
    """One picture from inside a render: what the photo looked like at some point on its way through the recipe."""

    label: str
    image: np.ndarray


def trace(
    rgb: np.ndarray,
    recipe: Recipe,
    matte: np.ndarray | None = None,
    *,
    find_subject: SubjectFinder | None = None,
) -> list[Stage]:
    """Every stage of a render, so you can see what each step did and where something went wrong.

    The stages are the photo as the recipe receives it (framed and scaled), the subject matte if the recipe uses one
    (white = subject), the image after each step, in order, and, when the recipe's framing cuts the picture out, that
    cut (an RGBA picture, four channels). The last stage is exactly what `render` returns, because both run the
    same code. Every stage stays in memory: one picture per step, plus the input and the matte.

    rgb, recipe, matte, find_subject: as for `render`.
    """
    rgb, matte = _prepare(rgb, recipe, matte, find_subject)
    stages = [Stage("input", rgb)]
    if recipe.needs_matte:
        subject = np.repeat(matte[..., None], 3, axis=2)  # type: ignore[index]  # `_prepare` gave the matte this recipe needs
        stages.append(Stage("subject matte (white = subject)", subject))
    for number, (step, image) in enumerate(_stages(rgb, recipe.steps, matte), start=1):
        stages.append(Stage(f"{number}. {step.describe()}", image))
    if recipe.frame.has_alpha:
        label = f"{len(recipe.steps) + 1}. transparent({recipe.frame.transparent!r})"
        stages.append(Stage(label, recipe.frame.cut(stages[-1].image, matte)))
    return stages


def render_all(
    rgb: np.ndarray, recipes: list[Recipe], *, find_subject: SubjectFinder | None = None
) -> list[np.ndarray]:
    """Run several recipes on one photo; the subject is found at most once, however many recipes need it.

    rgb: the photo (see `render`). recipes: the recipes to run; the results come back in the same order, one still each.
    find_subject: called once with the photo as given when any recipe needs the subject (SubjectNotFoundError if it is
    needed and missing).
    """
    masked = next((recipe for recipe in recipes if recipe.needs_matte), None)
    matte = None if masked is None else _find_subject(rgb, masked, find_subject)
    return [render(rgb, recipe, matte) for recipe in recipes]


def _prepare(
    rgb: np.ndarray, recipe: Recipe, matte: np.ndarray | None, find_subject: SubjectFinder | None
) -> tuple[np.ndarray, np.ndarray | None]:
    """Find the subject if the recipe needs it, then frame the photo and the matte together and scale both to the
    recipe's size. The one place the photo is framed (a depth map, later, moves with it here too)."""
    if matte is None and recipe.needs_matte:
        matte = _find_subject(rgb, recipe, find_subject)
    if matte is not None and matte.shape[:2] != rgb.shape[:2]:
        raise SubjectNotFoundError(f"matte is {matte.shape[:2]} but the photo is {rgb.shape[:2]}")
    rgb, matte = recipe.frame.apply(rgb, matte)
    if recipe.size:
        rgb = resize(rgb, recipe.size)
        matte = None if matte is None else resize(matte, recipe.size)
    return rgb, matte


def _find_subject(rgb: np.ndarray, recipe: Recipe, find_subject: SubjectFinder | None) -> np.ndarray:
    if find_subject is None:
        raise SubjectNotFoundError(
            f"recipe {recipe.name!r} has masked steps, so it needs the subject: pass `matte` (the subject's alpha) "
            "or `find_subject` (a function photo -> alpha, e.g. bullterrier_image.matte.subject_matte)"
        )
    return subject_from(find_subject, rgb)


def subject_from(find_subject: SubjectFinder, rgb: np.ndarray) -> np.ndarray:
    """Ask `find_subject` for the subject of `rgb` and return its matte. The one place a finder is called and its answer
    checked (the runner and the pipeline both use it): SubjectNotFoundError if it returned None, since a finder must
    answer or raise, and carrying on would drop the subject without a word."""
    matte = find_subject(rgb)
    if matte is None:
        raise SubjectNotFoundError(
            "`find_subject` returned None: it must return the subject's alpha (float32, the photo's height and width, "
            "0..1) or raise when it finds none"
        )
    return matte


def _last(stages: Iterator[tuple[Step, np.ndarray]]) -> np.ndarray:
    """The image after the final stage (there must be one). Only the newest image is kept while the stages run: each
    step's input is freed once the next step has its output, unless something else still holds it."""
    return deque(stages, maxlen=1)[0][1]


def _stages(out: np.ndarray, steps: Sequence[Step], matte: np.ndarray | None) -> Iterator[tuple[Step, np.ndarray]]:
    """Run the steps in order, yielding each step with the image after it. The one place a step is applied."""
    for step in steps:
        extra = {"matte": matte} if step.wants_matte else {}
        result = registry.apply(step.effect, out, **step.params, **extra)
        if result.shape != out.shape:
            raise RuntimeError(f"effect {step.effect!r} changed the image shape {out.shape} -> {result.shape}")
        if step.mask or step.opacity < 1.0:
            result = _blend(out, result, step, matte)
        out = result
        yield step, out


def _blend(before: np.ndarray, after: np.ndarray, step: Step, matte: np.ndarray | None) -> np.ndarray:
    """`after` blended over `before` by the step's opacity and mask. A function of its own so that its weights, a third
    of a picture, are freed before the next step runs rather than kept by the generator."""
    alpha: np.floating | np.ndarray = np.float32(step.opacity)
    if step.mask:
        # `_prepare` gave the matte every masked step needs
        alpha = alpha * (matte if step.mask == "subject" else 1.0 - matte)[..., None]  # type: ignore[index,operator]
    return before + (after - before) * alpha
