"""Recipes: an ordered list of effects, kept in a small TOML file, and the functions that run one on a photo.

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
from collections.abc import Iterator
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np

from . import effects, imageio
from .matte import subject_matte

_MASKS = ("subject", "background")
_SIMPLE_EASES = {
    "linear": lambda t: t,
    "in": lambda t: t * t,
    "out": lambda t: 1.0 - (1.0 - t) ** 2,
    "in-out": lambda t: t * t * (3.0 - 2.0 * t),
}
_CURVES = {  # cubic-bezier (x1, y1, x2, y2), as in the motion-design skill's easing tables
    "gentle": (0.4, 0.0, 0.2, 1.0),  # smooth on-screen motion, no overshoot: the Premium signature
    "standard": (0.2, 0.0, 0.0, 1.0),
    "accelerate": (0.3, 0.0, 1.0, 1.0),  # exits
    "decelerate": (0.0, 0.0, 0.0, 1.0),  # entrances
    "emphasized": (0.05, 0.7, 0.1, 1.0),
    "sine": (0.37, 0.0, 0.63, 1.0),  # ease-in-out sine: even, flowing motion; the skill's choice for calm
}


def _cubic_bezier(x1: float, y1: float, x2: float, y2: float):
    """The CSS-style easing function for a cubic-bezier curve: time (0..1) -> progress (0..1)."""

    def coordinate(a: float, b: float, t: float) -> float:
        return 3 * a * (1 - t) ** 2 * t + 3 * b * (1 - t) * t**2 + t**3

    def ease(u: float) -> float:
        if u <= 0.0 or u >= 1.0:
            return min(max(u, 0.0), 1.0)  # exact end points, so a held first or last frame is exactly the end state
        low, high = 0.0, 1.0
        for _ in range(40):  # x(t) rises steadily, so bisection finds the t whose x is `u`
            mid = (low + high) / 2
            low, high = (mid, high) if coordinate(x1, x2, mid) < u else (low, mid)
        return coordinate(y1, y2, (low + high) / 2)

    return ease


def _easing(spec):
    """The easing function for a name or a four-number cubic-bezier."""
    if isinstance(spec, str):
        return _SIMPLE_EASES[spec] if spec in _SIMPLE_EASES else _cubic_bezier(*_CURVES[spec])
    return _cubic_bezier(*spec)


@dataclass(frozen=True)
class Animated:
    """A parameter that changes over the clip, written `{ from = 0, to = 1, ease = "gentle" }` in a recipe."""

    start: float
    end: float
    ease: str | tuple[float, float, float, float] = "linear"
    over: str = "motion"

    def at(self, position: float, clip: float | None = None) -> float:
        """The value at `position` (0..1) of the moving part of the clip, or at `clip` (0..1 of the whole clip,
        holds included) when `over` is "clip"."""
        t = (position if clip is None else clip) if self.over == "clip" else position
        return self.start + (self.end - self.start) * _easing(self.ease)(t)


@dataclass(frozen=True)
class Animation:
    seconds: float
    fps: int = 30
    hold_start: float = 0.0
    hold_end: float = 0.0
    loop: str = "once"

    def frames(self) -> list[tuple[float, float]]:
        """For every output frame, in order: (position in the moving part of the clip, position in the whole clip).

        Both run 0..1. The first stays at 0 during `hold_start` and at 1 during `hold_end`; the second never stops.
        Pingpong looping plays the frames forward and then backward, without repeating the turning frames.
        """
        count = max(2, round(self.seconds * self.fps))
        moving = max(self.seconds - self.hold_start - self.hold_end, 1e-6)
        motion = [min(max((i / self.fps - self.hold_start) / moving, 0.0), 1.0) for i in range(count)]
        if self.loop == "pingpong":
            motion = motion + motion[-2:0:-1]
        return [(m, i / (len(motion) - 1)) for i, m in enumerate(motion)]

    def positions(self) -> list[float]:
        """The position in the moving part of the clip of every output frame (see `frames`)."""
        return [motion for motion, _ in self.frames()]


@dataclass(frozen=True)
class Step:
    effect: str
    params: dict = field(default_factory=dict)
    mask: str | None = None
    opacity: float = 1.0

    @property
    def animated(self) -> bool:
        return any(isinstance(value, Animated) for value in self.params.values())

    def params_at(self, position: float, clip: float | None = None) -> dict:
        """The parameters with every animated value worked out for a moment of the clip (see `Animated.at`)."""
        return {k: v.at(position, clip) if isinstance(v, Animated) else v for k, v in self.params.items()}


@dataclass(frozen=True)
class Recipe:
    name: str
    steps: tuple[Step, ...]
    size: int | None = 1024
    animation: Animation | None = None

    @property
    def needs_matte(self) -> bool:
        """True if any step is masked, i.e. rendering must find the person first (the slow part)."""
        return any(step.mask or _wants_matte(step.effect) for step in self.steps)


def _wants_matte(effect_name: str) -> bool:
    """True for effects that take the person's silhouette: they declare a parameter called `matte`."""
    return "matte" in inspect.signature(effects.get(effect_name)).parameters


def _animated_value(value, where: str, key: str):
    """Turn `{ from, to, ease, over }` into an Animated; any other value is returned unchanged."""
    if not isinstance(value, dict):
        return value
    ease = value.get("ease", "linear")
    if isinstance(ease, list):
        ease = tuple(ease)
    numbers = all(isinstance(value.get(k), int | float) and not isinstance(value.get(k), bool) for k in ("from", "to"))
    named = isinstance(ease, str) and (ease in _SIMPLE_EASES or ease in _CURVES)
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
        names = (*_SIMPLE_EASES, *_CURVES)
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
    """Read and validate a recipe file. Errors name the file, the step and the offending key."""
    path = Path(path)
    with open(path, "rb") as f:
        raw = tomllib.load(f)
    animation = _load_animation(raw["animation"], path.name) if "animation" in raw else None
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
        if "matte" in entry:
            raise ValueError(f"{where}: `matte` is supplied by the recipe itself and cannot be set")
        params = {key: _animated_value(value, where, key) for key, value in entry.items()}
        step = Step(name, params, mask, float(opacity))
        if step.animated and animation is None:
            raise ValueError(f"{where}: a parameter is animated but the recipe has no [animation] table")
        try:
            inspect.signature(effects.get(name)).bind(None, **step.params_at(0.0))  # unknown or missing parameters
        except (KeyError, TypeError) as error:
            raise ValueError(f"{where}: {error}") from None
        steps.append(step)
    if not steps:
        raise ValueError(f"{path.name}: a recipe needs at least one [[step]]")
    return Recipe(name=raw.get("name", path.stem), steps=tuple(steps), size=raw.get("size", 1024), animation=animation)


def render(rgb: np.ndarray, recipe: Recipe, matte: np.ndarray | None = None, at: float = 0.0) -> np.ndarray:
    """Run `recipe` on the photo `rgb` and return the result as a new image.

    The photo is first scaled to `recipe.size`. Steps run in order, each on the previous step's output. A masked
    step is blended in through the person matte, so only the subject or only the background changes.
    Effects that declare a `matte` parameter (see `evaporate`) get the person's silhouette automatically.
    `matte` (alpha for `rgb`, any size) can be passed in to avoid finding the person again when rendering several
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
        raise ValueError(f"recipe {recipe.name!r} has no [animation] table")
    rgb, matte = _prepare(rgb, recipe, matte)
    first_moving = next(i for i, step in enumerate(recipe.steps) if step.animated)
    still = _run(rgb, recipe.steps[:first_moving], matte, 0.0, 0.0)
    for motion, clip in recipe.animation.frames():
        yield _finish(_run(still, recipe.steps[first_moving:], matte, motion, clip))


def render_all(rgb: np.ndarray, recipes: list[Recipe]) -> list[np.ndarray]:
    """Run several recipes on one photo; the person is found at most once, however many recipes need them."""
    matte = subject_matte(rgb) if any(r.needs_matte for r in recipes) else None
    return [render(rgb, recipe, matte) for recipe in recipes]


def _prepare(rgb: np.ndarray, recipe: Recipe, matte: np.ndarray | None) -> tuple[np.ndarray, np.ndarray | None]:
    """Scale the photo and matte to the recipe's size, and find the person if a step needs them."""
    if matte is not None and matte.shape[:2] != rgb.shape[:2]:
        raise ValueError(f"matte is {matte.shape[:2]} but the photo is {rgb.shape[:2]}")
    if recipe.size:
        rgb = imageio.resize(rgb, recipe.size)
        matte = None if matte is None else imageio.resize(matte, recipe.size)
    if recipe.needs_matte and matte is None:
        matte = subject_matte(rgb)
    return rgb, matte


def _run(out: np.ndarray, steps: tuple[Step, ...], matte: np.ndarray | None, motion: float, clip: float) -> np.ndarray:
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
    return out


def _finish(out: np.ndarray) -> np.ndarray:
    return np.clip(out, 0.0, 1.0).astype(np.float32)
