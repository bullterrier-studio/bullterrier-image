"""Recipes: an ordered list of effects, kept in a small TOML file, and the functions that run one on a photo.

    name = "green-terminal"
    size = 1024                    # longest side in px; effect sizes (cell, radius...) are relative to this

    [[step]]
    effect = "ascii"               # any effect from photo_fx.effects; the other keys are its parameters
    cell = 9
    fg = "#00ff9c"

    [[step]]
    effect = "fill"
    color = "#000000"
    mask = "background"            # "subject" or "background": apply only there (needs a clear subject in the photo)
    opacity = 0.3                  # optional, 0..1: blend the effect in partway (default 1 = fully)

Removing the background is not a feature of its own: it is a `fill` step masked to the background.

A recipe becomes an animation with an [animation] table, and any numeric parameter can then change over the clip:

    [animation]
    seconds = 5
    fps = 30
    hold_start = 0.5               # optional: seconds the first frame is held
    hold_end = 0.5                 # optional: seconds the last frame is held
    loop = "pingpong"              # optional: "once" (default), or play forward then backward for a seamless loop

    [[step]]
    effect = "disperse"
    progress = { from = 0.0, to = 1.0, ease = "gentle" }

`ease` is a name (linear, the default; in, out, in-out; or the cubic-bezier presets gentle, standard, accelerate,
decelerate, emphasized, sine) or your own cubic-bezier as four numbers, `ease = [0.4, 0.0, 0.2, 1.0]`. Spatial motion
should not stay linear. A value normally moves only while the animation itself runs (after `hold_start`, before
`hold_end`); add `over = "clip"` to make it run across the whole clip including the holds, e.g. a clock
for ambient motion.
"""

import inspect
import tomllib
from collections import deque
from collections.abc import Iterator
from dataclasses import dataclass, field, replace
from pathlib import Path

import numpy as np

from . import effects, imageio
from .matte import subject_matte
from .timing import EASE_NAMES, Animated, Animation, ramp  # noqa: F401

_MASKS = ("subject", "background")


@dataclass(frozen=True)
class Step:
    """One effect with its parameters, optionally limited to the subject or the background and blended in partway.

    Steps are checked when they are made, wherever they come from (a recipe file, `fx`, or your own code): an unknown
    effect, a misspelt parameter or an out-of-range opacity fails at once with a message saying what is valid.
    """

    effect: str
    params: dict = field(default_factory=dict)
    mask: str | None = None
    opacity: float = 1.0

    def __post_init__(self) -> None:
        if self.mask is not None and self.mask not in _MASKS:
            raise ValueError(f"mask must be one of {_MASKS}, got {self.mask!r}")
        if (
            not isinstance(self.opacity, int | float)
            or isinstance(self.opacity, bool)
            or not 0.0 <= self.opacity <= 1.0
        ):
            raise ValueError(f"opacity must be a number from 0 to 1, got {self.opacity!r}")
        if "matte" in self.params:
            raise ValueError("`matte` is supplied by the recipe itself and cannot be set")
        try:
            inspect.signature(effects.get(self.effect)).bind(None, **self.params_at(0.0))
        except (KeyError, TypeError) as error:
            raise ValueError(str(error.args[0]) if isinstance(error, KeyError) else str(error)) from None

    def where(self, region: str) -> "Step":
        """The same step applied only to the "subject" or only to the "background" of the photo."""
        return replace(self, mask=region)

    def blend(self, amount: float) -> "Step":
        """The same step mixed in partway: 0 = no effect, 1 = fully (the default)."""
        return replace(self, opacity=amount)

    @property
    def animated(self) -> bool:
        return any(isinstance(value, Animated) for value in self.params.values())

    def params_at(self, position: float, clip: float | None = None) -> dict:
        """The parameters with every animated value worked out for a moment of the clip (see `Animated.at`)."""
        return {k: v.at(position, clip) if isinstance(v, Animated) else v for k, v in self.params.items()}

    def describe(self, width: int = 56) -> str:
        """A short readable form, e.g. `fill(color='#000000') on background`, cut to `width` characters."""
        shown = ", ".join(f"{k}={_short(v)}" for k, v in self.params.items())
        text = f"{self.effect}({shown})"
        if self.mask:
            text += f" on {self.mask}"
        if self.opacity < 1.0:
            text += f" x{self.opacity:g}"
        return text if len(text) <= width else text[: width - 1] + "…"

    __str__ = describe


def _short(value) -> str:
    if isinstance(value, Animated):
        return f"{value.start:g}->{value.end:g}"
    text = repr(value)
    return text if len(text) <= 18 else text[:15] + "..."


@dataclass(frozen=True)
class Recipe:
    name: str
    steps: tuple[Step, ...]
    size: int | None = 1024
    animation: Animation | None = None

    def __post_init__(self) -> None:
        object.__setattr__(self, "steps", tuple(self.steps))  # a list is fine when building one in code
        if not self.steps:
            raise ValueError("a recipe needs at least one step")
        if self.animation is None and any(step.animated for step in self.steps):
            raise ValueError(
                "a parameter is animated but the recipe has no animation: add an [animation] table (seconds, fps)"
            )

    @property
    def needs_matte(self) -> bool:
        """True if any step is masked, i.e. rendering must find the subject first (the slow part)."""
        return any(step.mask or _wants_matte(step.effect) for step in self.steps)


def _wants_matte(effect_name: str) -> bool:
    """True for effects that take the subject's silhouette: they declare a parameter called `matte`."""
    return "matte" in inspect.signature(effects.get(effect_name)).parameters


def _animated_value(value, where: str, key: str):
    """Turn `{ from, to, ease, over }` into an Animated; any other value is returned unchanged."""
    if not isinstance(value, dict):
        return value
    ease = value.get("ease", "linear")
    if isinstance(ease, list):
        ease = tuple(ease)
    numbers = all(isinstance(value.get(k), int | float) and not isinstance(value.get(k), bool) for k in ("from", "to"))
    named = isinstance(ease, str) and ease in EASE_NAMES
    curve = (
        isinstance(ease, tuple)
        and len(ease) == 4
        and all(isinstance(n, int | float) and not isinstance(n, bool) for n in ease)
        and 0 <= ease[0] <= 1
        and 0 <= ease[2] <= 1
    )
    if (
        set(value) - {"from", "to", "ease", "over"}
        or not numbers
        or not (named or curve)
        or value.get("over", "motion") not in ("motion", "clip")
    ):
        names = EASE_NAMES
        raise ValueError(
            f"{where}: `{key}` must be {{ from = number, to = number, ease = one of {names} or [x1, y1, x2, y2] "
            f'with x1, x2 in 0..1, over = "motion" or "clip" }}'
        )
    return Animated(float(value["from"]), float(value["to"]), ease, value.get("over", "motion"))


def _load_animation(table: dict, filename: str) -> Animation:
    unknown = set(table) - {"seconds", "fps", "hold_start", "hold_end", "loop"}
    if unknown or "seconds" not in table:
        raise ValueError(
            f"{filename}: [animation] needs `seconds`; allowed keys: seconds, fps, hold_start, hold_end, loop"
        )
    animation = Animation(**table)
    if animation.seconds <= 0 or animation.fps < 1 or animation.hold_start < 0 or animation.hold_end < 0:
        raise ValueError(f"{filename}: [animation] needs positive seconds and fps, and holds of 0 or more")
    if animation.hold_start + animation.hold_end >= animation.seconds:
        raise ValueError(f"{filename}: [animation] the holds leave no time for the animation itself")
    if animation.loop not in ("once", "pingpong"):
        raise ValueError(f"{filename}: [animation] loop must be 'once' or 'pingpong'")
    return animation


def load_recipe(path: str | Path) -> Recipe:
    """Read a recipe file. Errors name the file, the step and the offending key."""
    path = Path(path)
    with open(path, "rb") as f:
        raw = tomllib.load(f)
    animation = _load_animation(raw["animation"], path.name) if "animation" in raw else None
    steps = []
    for i, entry in enumerate(raw.get("step", []), start=1):
        where = f"{path.name}, step {i}"
        entry = dict(entry)
        name = entry.pop("effect", None)
        if name is None:
            raise ValueError(f"{where}: missing `effect`")
        mask, opacity = entry.pop("mask", None), entry.pop("opacity", 1.0)
        params = {key: _animated_value(value, where, key) for key, value in entry.items()}
        try:
            steps.append(Step(name, params, mask, opacity if isinstance(opacity, int | float) else opacity))
        except ValueError as error:
            raise ValueError(f"{where}: {error}") from None
    try:
        return Recipe(raw.get("name", path.stem), tuple(steps), raw.get("size", 1024), animation)
    except ValueError as error:
        raise ValueError(f"{path.name}: {error}") from None


def render(rgb: np.ndarray, recipe: Recipe, matte: np.ndarray | None = None, at: float = 0.0) -> np.ndarray:
    """Run `recipe` on the photo `rgb` and return the result as a new image.

    The photo is first scaled to `recipe.size`. Steps run in order, each on the previous step's output. A masked
    step is blended in through the subject matte, so only the subject or only the background changes.
    Effects that declare a `matte` parameter (see `evaporate`) get the subject's silhouette automatically.
    `matte` (alpha for `rgb`, any size) can be passed in to avoid finding the subject again when rendering several
    recipes from one photo; if a masked step needs it and it is missing, it is computed here.

    For an animated recipe this is one still: the frame at position `at` (0..1) of the clip.
    """
    rgb, matte = _prepare(rgb, recipe, matte)
    return _finish(_run(rgb, recipe.steps, matte, at, at))


def render_frames(rgb: np.ndarray, recipe: Recipe, matte: np.ndarray | None = None) -> Iterator[np.ndarray]:
    """Yield every frame of an animated recipe, in order (see `Animation.positions`).

    The steps before the first animated one cannot change between frames, so they run once for the whole clip.
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
    """
    rgb, matte = _prepare(rgb, recipe, matte)
    stages = [Stage("input", rgb)]
    if recipe.needs_matte:
        stages.append(Stage("subject matte (white = subject)", np.repeat(matte[..., None], 3, axis=2)))
    for number, (step, image) in enumerate(_stages(rgb, recipe.steps, matte, at, at), start=1):
        stages.append(Stage(f"{number}. {step.describe()}", _finish(image)))
    return stages


def render_all(rgb: np.ndarray, recipes: list[Recipe]) -> list[np.ndarray]:
    """Run several recipes on one photo; the subject is found at most once, however many recipes need it."""
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
        extra = {"matte": matte} if _wants_matte(step.effect) else {}
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
