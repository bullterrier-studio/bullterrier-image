"""The data model of a look: a `Step` (one effect, its parameters, optional mask and opacity) and a `Recipe` (a named,
ordered tuple of steps with an output size and an optional animation).

Promises:
- Everything is validated when it is made, wherever it comes from (a TOML file, `fx`, or your own code): an unknown
  effect, a misspelt parameter, a bad mask or opacity, an animated parameter without an animation, or an empty recipe
  fails at once with a message saying what is valid.
- Steps and recipes are immutable values: `where` and `blend` return changed copies.
- `Step.params_at` works out animated parameters for a moment of the clip; `Step.describe` gives a short readable form.

Dependency direction: imports `timing` (and the effect registry) only. `loader` (TOML to `Recipe`) and `runner`
(running a `Recipe` on a photo) both build on this module and do not know each other.
"""

import inspect
from dataclasses import dataclass, field, replace

from . import registry
from .timing import Animated, Animation

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
            inspect.signature(registry.get(self.effect)).bind(None, **self.params_at(0.0))
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
        """True if any parameter changes over the clip."""
        return any(isinstance(value, Animated) for value in self.params.values())

    @property
    def wants_matte(self) -> bool:
        """True for an effect that takes the subject's silhouette: it declares a parameter called `matte`."""
        return "matte" in inspect.signature(registry.get(self.effect)).parameters

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
    """A named look: ordered steps, the size the photo is scaled to, and an optional animation.

    name: used for output file names. steps: at least one `Step`, run in order (a list is accepted and kept as a tuple).
    size: longest side of the working image in px (effect sizes such as cell or radius are relative to it), or None to
        keep the photo's own size. animation: an `Animation`; required if any step has an animated parameter.
    """

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
        return any(step.mask or step.wants_matte for step in self.steps)
