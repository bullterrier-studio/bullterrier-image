"""The data model of a look: a `Step` (one effect, its parameters, optional mask and opacity) and a `Recipe` (a named,
ordered tuple of steps with an output size and an optional animation).

Promises:
- Everything is validated when it is made, wherever it comes from (a TOML file, `fx`, or your own code): an unknown
  effect, a misspelt or out-of-range parameter, a bad mask or opacity, an animated parameter without an animation, or
  an empty recipe fails at once with a `RecipeError` (a `ValueError`; an unknown effect is an `UnknownEffectError`,
  also a `KeyError`) saying what is valid. A step's parameters are checked by its
  effect's own parameter model. An animated parameter is checked by a step at both ends of its movement and at 9
  moments between, and by its recipe at every frame that will be rendered, because an easing curve can overshoot its
  ends: a bad animation fails when the recipe is made, not at some frame later. An animated whole-number parameter
  moves in whole steps (rounded to the nearest whole number) and is checked that way.
- Steps and recipes are immutable values (frozen Pydantic models): `where` and `blend` return changed copies. A step
  keeps its parameters as they were written, so `describe` shows them that way.
- `Step.params_at` works out animated parameters for a moment of the clip; `Step.describe` gives a short readable form.

Dependency direction: imports `timing`, the effect registry, `params` and `errors` only. `loader` (TOML to `Recipe`)
and `runner` (running a `Recipe` on a photo) both build on this module and do not know each other.
"""

import re
from collections.abc import Mapping, Sequence
from typing import Any, Literal

from pydantic import Field, field_validator, model_validator

from . import registry
from .errors import RecipeError
from .params import ValidatedModel
from .timing import Animated, Animation

# The moments of an animated parameter's movement at which a step checks it: both ends first (so a bad end value is
# the one reported), then 9 evenly spaced between, where an easing curve may overshoot its ends.
_CHECKED_MOMENTS = (0.0, 1.0, *(i / 10 for i in range(1, 10)))


class Step(ValidatedModel):
    """One effect with its parameters, optionally limited to the subject or the background and blended in partway.

    Steps are checked when they are made, wherever they come from (a recipe file, `fx`, or your own code): an unknown
    effect, a misspelt or out-of-range parameter or opacity fails at once with a message saying what is valid.
    """

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
        """Validate and make the step; RecipeError saying what is wrong and what is valid (`unknown` keywords, which
        pydantic's own validation may pass, are refused by name)."""
        self._init_checked("", effect=effect, params=dict(params or {}), mask=mask, opacity=opacity, **unknown)

    @model_validator(mode="after")
    def _valid_for_the_effect(self) -> "Step":
        """The parameters, checked by the effect itself, at every checked moment of any animation."""
        for name in registry.INPUTS:
            if name in self.params:
                raise RecipeError(f"`{name}` is supplied by the recipe itself and cannot be set")
        registry.inputs(self.effect)  # an unknown effect: UnknownEffectError listing the known ones
        registry.validate(self.effect, self.params_at(0.0))
        if self.animated:
            for moment in _CHECKED_MOMENTS:
                try:
                    registry.validate(self.effect, self.params_at(moment, moment))
                except RecipeError as error:
                    raise RecipeError(f"{error} (animated: at {moment:.0%} of its movement)") from None
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
        """The parameters with every animated value worked out for a moment of the clip (see `Animated.at`). An
        animated whole-number parameter is rounded to the nearest whole number (halves to the even one, as Python's
        `round`): it moves in whole steps."""
        whole = registry.parameters(self.effect).whole_number_fields()
        values = {}
        for name, value in self.params.items():
            if isinstance(value, Animated):
                value = value.at(position, clip)
                value = round(value) if name in whole else value
            values[name] = value
        return values

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


# A recipe name becomes a file name and recipes are shared as data, so it holds nothing that could leave the output
# folder or hide a file (CWE-22), on any operating system.
_NAME_CHARACTERS = "A-Za-z0-9._-"
_NAME_LENGTH = 64
_NAME_PATTERN = rf"^[A-Za-z0-9][{_NAME_CHARACTERS}]{{0,{_NAME_LENGTH - 1}}}$"
# Windows keeps these names for devices, in any case and whatever the extension: such a file cannot be made there.
_WINDOWS_DEVICES = frozenset(
    {"CON", "PRN", "AUX", "NUL", *(f"COM{digit}" for digit in range(1, 10)), *(f"LPT{digit}" for digit in range(1, 10))}
)


def _a_device(name: str) -> bool:
    return name.split(".", 1)[0].upper() in _WINDOWS_DEVICES


def safe_name(text: str) -> str:
    """`text` (e.g. a file name without its extension) made into a valid recipe name, or "" when none is left.

    Every run of characters a name may not hold becomes one `-`, repeated `-` collapse into one, anything before the
    first letter or digit is dropped, the result is cut to 64 characters and a trailing `-` or `.` is trimmed: "my
    look" gives "my-look". "" when nothing remains or the result is a name Windows reserves for a device ("con")."""
    name = re.sub(r"-{2,}", "-", re.sub(rf"[^{_NAME_CHARACTERS}]+", "-", text))
    name = re.sub(r"^[^A-Za-z0-9]+", "", name)[:_NAME_LENGTH].rstrip("-.")
    return "" if not name or _a_device(name) else name


class Recipe(ValidatedModel):
    """A named look: ordered steps, the size the photo is scaled to, and an optional animation.

    name: used for output file names, so it must be a safe one: 1 to 64 letters, digits, `.`, `_` or `-`, starting with
        a letter or digit (no folders, no hidden files), and not a device name Windows reserves (CON, PRN, AUX, NUL,
        COM1-9, LPT1-9, in any case, with or without an extension); anything else is a RecipeError. `safe_name` makes
        a valid one from any text.
    steps: at least one `Step`, run in order (a list is accepted and kept as a tuple).
    size: longest side of the working image in px, >= 1 (effect sizes such as cell or radius are relative to it), or
        None to keep the photo's own size. animation: an `Animation`; required if any step has an animated parameter.
    """

    name: str = Field(
        pattern=_NAME_PATTERN,
        description="Used for output file names: 1 to 64 of A-Z a-z 0-9 . _ -, starting with a letter or digit.",
    )
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
        """Validate and make the recipe; RecipeError saying what is wrong (unknown keywords are refused by name)."""
        self._init_checked("", name=name, steps=tuple(steps), size=size, animation=animation, **unknown)

    @field_validator("name")
    @classmethod
    def _not_a_device(cls, name: str) -> str:
        """After the pattern: a name Windows keeps for a device is refused too, on every system."""
        if _a_device(name):
            raise RecipeError(
                f"{name!r} is reserved: Windows keeps CON, PRN, AUX, NUL, COM1-9 and LPT1-9 (with any extension) for "
                "devices; choose another name"
            )
        return name

    @model_validator(mode="after")
    def _complete(self) -> "Recipe":
        if not self.steps:
            raise RecipeError("a recipe needs at least one step")
        if self.animation is None and any(step.animated for step in self.steps):
            raise RecipeError(
                "a parameter is animated but the recipe has no animation: add an [animation] table (seconds, fps)"
            )
        if self.animation is not None:
            self._check_every_frame(self.animation)
        return self

    def _check_every_frame(self, animation: Animation) -> None:
        """Each animated step's values at every frame that will be rendered: an easing curve can leave the range
        between any few moments a step checks on its own, so the recipe, which knows its frames, checks them all."""
        frames = animation.frames()
        for number, step in enumerate(self.steps, start=1):
            if not step.animated:
                continue
            for index, (motion, clip) in enumerate(frames):
                try:
                    registry.validate(step.effect, step.params_at(motion, clip))
                except RecipeError as error:
                    where = f"frame {index} at {index / animation.fps:.2f} s"
                    raise RecipeError(f"step {number}: {error} (animated: {where})") from None

    @property
    def needs_matte(self) -> bool:
        """True if any step is masked, i.e. rendering must find the subject first (the slow part)."""
        return any(step.mask or step.wants_matte for step in self.steps)
