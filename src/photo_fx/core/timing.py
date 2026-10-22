"""How a value changes over a clip: easing curves, `Animated` parameters, `ramp`, and the `Animation` timeline.

Promises:
- `ramp(start, end, ease, over)` (written `{ from, to, ease, over }` in a recipe) is a number that moves from `start`
  to `end` as the clip plays; `Animated.at` gives its value at any moment.
- `Animation` turns seconds, fps, holds and looping into the position (0..1) of every output frame.
- Easing is by name (see `EASE_NAMES`) or by your own cubic-bezier of four numbers; both end exactly on 0 and 1.

Dependency direction: this module imports nothing from photo_fx. `steps` builds on it, `loader` and `runner` on `steps`.
"""

from collections.abc import Callable
from dataclasses import dataclass

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


def _cubic_bezier(x1: float, y1: float, x2: float, y2: float) -> Callable[[float], float]:
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


def _easing(spec: str | tuple[float, ...]) -> Callable[[float], float]:
    """The easing function for a name or a four-number cubic-bezier."""
    if isinstance(spec, str):
        return _SIMPLE_EASES[spec] if spec in _SIMPLE_EASES else _cubic_bezier(*_CURVES[spec])
    return _cubic_bezier(*spec)


EASE_NAMES = (*_SIMPLE_EASES, *_CURVES)  # every ease that can be written by name


@dataclass(frozen=True)
class Animated:
    """A parameter that changes over the clip, written `{ from = 0, to = 1, ease = "gentle" }` in a recipe.

    start, end: the parameter's value at the start and at the end of the movement (in the parameter's own unit).
    ease: a name from `EASE_NAMES`, or a cubic-bezier as four numbers (x1, y1, x2, y2) with x1 and x2 in 0..1.
    over: "motion" runs only while the animation itself runs (after `hold_start`, before `hold_end`); "clip" runs across
        the whole clip, holds included, e.g. a clock for ambient motion.
    """

    start: float
    end: float
    ease: str | tuple[float, ...] = "linear"  # a name, or the four numbers x1, y1, x2, y2
    over: str = "motion"

    def at(self, position: float, clip: float | None = None) -> float:
        """The value at `position` (0..1) of the moving part of the clip, or at `clip` (0..1 of the whole clip,
        holds included) when `over` is "clip"."""
        t = (position if clip is None else clip) if self.over == "clip" else position
        return self.start + (self.end - self.start) * _easing(self.ease)(t)


def ramp(
    start: float, end: float, ease: str | tuple[float, ...] | list[float] = "linear", over: str = "motion"
) -> Animated:
    """A parameter that changes over an animation: `fx.disperse(progress=ramp(0, 0.9, "gentle"))`.

    start, end: the value at the start and at the end of the movement. ease: a name from `EASE_NAMES`, or a cubic-bezier
    as a list or tuple of four numbers. over: "motion" (default) or "clip" (see `Animated`).
    """
    return Animated(float(start), float(end), tuple(ease) if isinstance(ease, list) else ease, over)


@dataclass(frozen=True)
class Animation:
    """The timeline of an animated recipe.

    seconds: length of the clip, > 0, holds included. fps: frames per second, >= 1.
    hold_start, hold_end: seconds the first / last frame is held, >= 0; together they must leave time to move.
    loop: "once" plays forward; "pingpong" plays forward then backward, for a seamless loop.
    """

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
