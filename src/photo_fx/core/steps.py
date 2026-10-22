"""The data model of a look: a `Step` (one effect, its parameters, optional mask and opacity) and a `Recipe` (a named,
ordered tuple of steps with an output size and an optional animation).

Promises:
- Everything is validated when it is made, wherever it comes from (a TOML file, `fx`, or your own code): an unknown
  effect, a misspelt or out-of-range parameter, a bad mask or opacity, an animated parameter without an animation, or
  an empty recipe fails at once with a `ValueError` saying what is valid. A step's parameters are checked by its
  effect's own parameter model; an animated parameter is checked at both ends of its movement and at 9 moments
  between, because an easing curve can overshoot its ends, so a bad animation fails here, not at some frame later.
- Steps and recipes are immutable values (frozen Pydantic models): `where` and `blend` return changed copies. A step
  keeps its parameters as they were written, so `describe` shows them that way.
- `Step.params_at` works out animated parameters for a moment of the clip; `Step.describe` gives a short readable form.

Dependency direction: imports `timing`, the effect registry and `params` only. `loader` (TOML to `Recipe`) and `runner`
(running a `Recipe` on a photo) both build on this module and do not know each other.
"""

from collections.abc import Mapping, Sequence
from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field, ValidationError, model_validator

from . import registry
from .params import describe_error
from .timing import Animated, Animation

# The moments of an animated parameter's movement at which a step checks it: both ends first (so a bad end value is
# the one reported), then 9 evenly spaced between, where an easing curve may overshoot its ends.
_CHECKED_MOMENTS = (0.0, 1.0, *(i / 10 for i in range(1, 10)))


class Step(BaseModel):
    """One effect with its parameters, optionally limited to the subject or the background and blended in partway.

    Steps are checked when they are made, wherever they come from (a recipe file, `fx`, or your own code): an unknown
    effect, a misspelt or out-of-range parameter or opacity fails at once with a message saying what is valid.
    """

    model_config = ConfigDict(extra="forbid", frozen=True)

    effect: str = Field(description="The name of a registered effect.")
    params: dict[str, Any] = Field(
        default_factory=dict, description="The effect's parameters as written; a value may be an `Animated`."
    )
    mask: Literal["subject", "background"] | None = Field(
        None, description='Apply only to the "subject" or only to the "background" of the photo; None: everywhere.'
    )
    opacity: float = Field(
        1.0, ge=0, le=1, strict=True, allow_inf_nan=False, description="How much of the step shows: 0 none, 1 fully."
    )

    def __init__(
        self,
        effect: str,
        params: Mapping[str, Any] | None = None,
        mask: str | None = None,
        opacity: float = 1.0,
        **unknown: Any,
    ) -> None:
        """Validate and make the step; ValueError saying what is wrong and what is valid (`unknown` keywords, which
        pydantic's own validation may pass, are refused by name)."""
        try:
            super().__init__(effect=effect, params=dict(params or {}), mask=mask, opacity=opacity, **unknown)
        except ValidationError as error:
            raise ValueError(describe_error(error, "", type(self).model_fields)) from None

    @model_validator(mode="after")
    def _valid_for_the_effect(self) -> "Step":
        """The parameters, checked by the effect itself, at every checked moment of any animation."""
        for name in registry.INPUTS:
            if name in self.params:
                raise ValueError(f"`{name}` is supplied by the recipe itself and cannot be set")
        try:
            registry.validate(self.effect, self.params_at(0.0))
        except KeyError as error:
            raise ValueError(error.args[0]) from None
        if self.animated:
            for moment in _CHECKED_MOMENTS:
                try:
                    registry.validate(self.effect, self.params_at(moment, moment))
                except ValueError as error:
                    raise ValueError(f"{error} (animated: at {moment:.0%} of its movement)") from None
        return self

    def where(self, region: str) -> "Step":
        """The same step applied only to the "subject" or only to the "background" of the photo."""
        return Step(self.effect, self.params, region, self.opacity)

    def blend(self, amount: float) -> "Step":
        """The same step mixed in partway: 0 = no effect, 1 = fully (the default)."""
        return Step(self.effect, self.params, self.mask, amount)

    @property
    def animated(self) -> bool:
        """True if any parameter changes over the clip."""
        return any(isinstance(value, Animated) for value in self.params.values())

    @property
    def wants_matte(self) -> bool:
        """True for an effect that takes the subject's silhouette: it declares the input `matte`."""
        return "matte" in registry.inputs(self.effect)

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

    def __str__(self) -> str:
        """The short readable form (see `describe`)."""
        return self.describe()


def _short(value: object) -> str:
    if isinstance(value, Animated):
        return f"{value.start:g}->{value.end:g}"
    text = repr(value)
    return text if len(text) <= 18 else text[:15] + "..."


class Recipe(BaseModel):
    """A named look: ordered steps, the size the photo is scaled to, and an optional animation.

    name: used for output file names. steps: at least one `Step`, run in order (a list is accepted and kept as a tuple).
    size: longest side of the working image in px, >= 1 (effect sizes such as cell or radius are relative to it), or
        None to keep the photo's own size. animation: an `Animation`; required if any step has an animated parameter.
    """

    model_config = ConfigDict(extra="forbid", frozen=True)

    name: str = Field(description="Used for output file names.")
    steps: tuple[Step, ...] = Field(description="The steps, run in order, each on the previous one's output.")
    size: int | None = Field(
        1024, ge=1, strict=True, description="Longest side of the working image in px; None keeps the photo's size."
    )
    animation: Animation | None = Field(None, description="The timeline; required if any parameter is animated.")

    def __init__(
        self,
        name: str,
        steps: Sequence[Step],
        size: int | None = 1024,
        animation: Animation | None = None,
        **unknown: Any,
    ) -> None:
        """Validate and make the recipe; ValueError saying what is wrong (unknown keywords are refused by name)."""
        try:
            super().__init__(name=name, steps=tuple(steps), size=size, animation=animation, **unknown)
        except ValidationError as error:
            raise ValueError(describe_error(error, "", type(self).model_fields)) from None

    @model_validator(mode="after")
    def _complete(self) -> "Recipe":
        if not self.steps:
            raise ValueError("a recipe needs at least one step")
        if self.animation is None and any(step.animated for step in self.steps):
            raise ValueError(
                "a parameter is animated but the recipe has no animation: add an [animation] table (seconds, fps)"
            )
        return self

    @property
    def needs_matte(self) -> bool:
        """True if any step is masked, i.e. rendering must find the subject first (the slow part)."""
        return any(step.mask or step.wants_matte for step in self.steps)
