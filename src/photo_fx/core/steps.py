"""The data model of a look: a `Step` (one effect, its parameters, optional mask and opacity) and a `Recipe` (a named,
ordered tuple of steps with an output size).

Promises:
- Everything is validated when it is made, wherever it comes from (a TOML file, `fx`, or your own code): an unknown
  effect, a misspelt or out-of-range parameter, a bad mask or opacity, or an empty recipe fails at once with a
  `RecipeError` (a `ValueError`; an unknown effect is an `UnknownEffectError`, also a `KeyError`) saying what is valid.
  A step's parameters are checked by its effect's own parameter model.
- Steps and recipes are immutable values (frozen Pydantic models): `where` and `blend` return changed copies. A step
  keeps its parameters as they were written, so `describe` shows them that way; with a `preset` they are the preset's
  parameters under the ones given, which is all the runner and the cache see of it.
- `Step.describe` gives a short readable form.

Dependency direction: imports the effect registry, `params` and `errors` only. `loader` (TOML to `Recipe`)
and `runner` (running a `Recipe` on a photo) both build on this module and do not know each other.
"""

import re
from collections.abc import Mapping, Sequence
from typing import Any, Literal

from pydantic import Field, field_validator, model_validator

from photo_fx.errors import RecipeError

from . import registry
from .params import ValidatedModel


class Step(ValidatedModel):
    """One effect with its parameters, optionally limited to the subject or the background and blended in partway.

    Steps are checked when they are made, wherever they come from (a recipe file, `fx`, or your own code): an unknown
    effect, a misspelt or out-of-range parameter or opacity fails at once with a message saying what is valid.
    """

    effect: str = Field(description="The name of a registered effect.")
    params: dict[str, Any] = Field(default_factory=dict, description="The effect's parameters as written.")
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
        preset: str | None = None,
        **unknown: Any,
    ) -> None:
        """Validate and make the step; RecipeError saying what is wrong and what is valid (`unknown` keywords, which
        pydantic's own validation may pass, are refused by name).

        preset: the name of one of the effect's presets (see `registry.presets`). The step holds the preset's
        parameters with `params` laid over them, so a parameter you give wins; an effect without that preset is a
        RecipeError listing the ones it has."""
        values = dict(params or {})
        if preset is not None:
            found = registry.presets(effect)  # an unknown effect: UnknownEffectError listing them
            if preset not in found:
                has = f"its presets: {', '.join(found)}" if found else "it has no presets"
                raise RecipeError(f"effect {effect!r} has no preset {preset!r}; {has}")
            values = {**found[preset].params, **values}
        self._init_checked("", effect=effect, params=values, mask=mask, opacity=opacity, **unknown)

    @model_validator(mode="after")
    def _valid_for_the_effect(self) -> "Step":
        """The parameters, checked by the effect itself."""
        for name in registry.INPUTS:
            if name in self.params:
                raise RecipeError(f"`{name}` is supplied by the recipe itself and cannot be set")
        registry.get(self.effect).validate(self.params)  # an unknown effect: UnknownEffectError listing them
        return self

    def where(self, region: str) -> "Step":
        """The same step applied only to the "subject" or only to the "background" of the photo."""
        return Step(self.effect, self.params, region, self.opacity)

    def blend(self, amount: float) -> "Step":
        """The same step mixed in partway: 0 = no effect, 1 = fully (the default)."""
        return Step(self.effect, self.params, self.mask, amount)

    @property
    def wants_matte(self) -> bool:
        """True for an effect that takes the subject's silhouette: it declares the input `matte`."""
        return "matte" in registry.inputs(self.effect)

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
    """A named look: ordered steps and the size the photo is scaled to.

    name: used for output file names, so it must be a safe one: 1 to 64 letters, digits, `.`, `_` or `-`, starting with
        a letter or digit (no folders, no hidden files), and not a device name Windows reserves (CON, PRN, AUX, NUL,
        COM1-9, LPT1-9, in any case, with or without an extension); anything else is a RecipeError. `safe_name` makes
        a valid one from any text.
    steps: at least one `Step`, run in order (a list is accepted and kept as a tuple).
    size: longest side of the working image in px, >= 1 (effect sizes such as cell or radius are relative to it), or
        None to keep the photo's own size.
    """

    name: str = Field(
        pattern=_NAME_PATTERN,
        description="Used for output file names: 1 to 64 of A-Z a-z 0-9 . _ -, starting with a letter or digit.",
    )
    steps: tuple[Step, ...] = Field(description="The steps, run in order, each on the previous one's output.")
    size: int | None = Field(
        1024, ge=1, strict=True, description="Longest side of the working image in px; None keeps the photo's size."
    )

    def __init__(self, name: str, steps: Sequence[Step], size: int | None = 1024, **unknown: Any) -> None:
        """Validate and make the recipe; RecipeError saying what is wrong (unknown keywords are refused by name)."""
        self._init_checked("", name=name, steps=tuple(steps), size=size, **unknown)

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
        return self

    @property
    def needs_matte(self) -> bool:
        """True if any step is masked, i.e. rendering must find the subject first (the slow part)."""
        return any(step.mask or step.wants_matte for step in self.steps)
