"""The data model of a look: a `Step` (one effect, its parameters, and how it is laid over the picture below it: a
mask, an opacity and a blend mode), an `Each` (a look for each piece of a partition) and a `Recipe` (a named, ordered
tuple of steps with the regions they name, an output size and a `Framing`).

Promises:
- Everything is validated when it is made, wherever it comes from (a TOML file, `fx`, or your own code): an unknown
  effect, a misspelt or out-of-range parameter, a bad opacity or blend mode, or an empty recipe fails at once
  with a `RecipeError` (a `ValueError`; an unknown effect is an `UnknownEffectError`, also a `KeyError`) saying what is
  valid. A step's parameters are checked by its effect's own parameter model.
- The regions a step names (`mask`, `partition`, `each`) are checked when the recipe is made, which alone knows the
  regions it declares: a name that is no region, or is a region of the wrong kind, is a `RecipeError` naming the step
  and listing the valid names.
- Steps and recipes are immutable values (frozen Pydantic models). A step keeps its parameters as they were written,
  so `describe` shows them that way; with a `preset` they are the preset's parameters under the ones given, which is
  all the runner and the cache see of it.
- `Step.describe` gives a short readable form.

Dependency direction: imports the effect registry, `framing`, `params`, `regions`, `ops.composite` (the blend modes'
names) and `errors` only. The recipe-file reader
`loader` (TOML to `Recipe`, in the adapters layer) and `runner` (running a `Recipe` on a photo) both build on this
module and do not know each other.
"""

import re
from collections.abc import Mapping, Sequence
from typing import Any, Literal

import numpy as np
from pydantic import Field, field_validator, model_validator

from bullterrier_image.errors import RecipeError
from bullterrier_image.ops.composite import BlendMode

from . import registry
from .framing import Framing
from .params import ValidatedModel
from .regions import Kind, check, needs, needs_finder, needs_subject

# One reference a step makes to a region: the key it is written in, the name, and the kind of region that key takes.
Reference = tuple[str, str, Literal["partition", "selection"] | None]


class _Layer(ValidatedModel):
    """How a step's result is laid over the picture below it (its input, the backdrop): what `Step` and `Each` share.

    The runner combines the input and the step's result (the source) by `blend_mode`, then mixes that over the input by
    `opacity`, only where `mask` says.
    """

    mask: str | None = Field(
        None,
        description='Apply only where this selection says: the "subject" or the "background" of the photo, or the name '
        "of a selection the recipe declares (a `Path` or a `Pick`); None: everywhere.",
    )
    opacity: float = Field(
        ge=0,
        le=1,
        strict=True,
        allow_inf_nan=False,
        description="How much of the step shows: 0 none, 1 fully; when none is given, the effect's own default "
        "(`registry.opacity`, 1 for most effects, and for an `each` step).",
    )
    blend_mode: BlendMode = Field(
        "normal",
        description="How the result combines with the step's input, as the W3C blend modes: 'normal' shows the result, "
        "'multiply' darkens, 'screen' lightens, 'overlay' and 'soft_light' deepen contrast, 'difference' subtracts, "
        "'color' takes the result's hue and saturation and keeps the input's brightness.",
    )

    def references(self) -> list[Reference]:
        """The regions this step names, as `(key, name, kind)`; the recipe checks them against its regions."""
        return [] if self.mask is None else [("mask", self.mask, "selection")]


class Step(_Layer):
    """One effect with its parameters, laid over the picture below it (the step's input) as a layer is in an image
    editor: optionally limited to a selection, combined by a blend mode and mixed in partway.

    The runner first combines the input (the backdrop) and the effect's result (the source) by `blend_mode`, then mixes
    that over the input by `opacity`, only where `mask` says. Steps are checked when they are made, wherever they come
    from (a recipe file, `fx`, or your own code): an unknown effect, a misspelt or out-of-range parameter, opacity or
    blend mode fails at once with a message saying what is valid. A `mask` or `partition` names a region: the recipe it
    is put in checks that it exists.
    """

    effect: str = Field(description="The name of a registered effect.")
    params: dict[str, Any] = Field(default_factory=dict, description="The effect's parameters as written.")
    partition: str | None = Field(
        None,
        description="For an effect that draws along the borders of a partition (`seams`): the name of the region whose "
        "pieces it uses. A selection counts as a partition of two pieces, what it chooses and what it does not.",
    )

    def __init__(
        self,
        effect: str,
        params: Mapping[str, Any] | None = None,
        mask: str | None = None,
        opacity: float | None = None,
        blend_mode: str = "normal",
        preset: str | None = None,
        partition: str | None = None,
        **unknown: Any,
    ) -> None:
        """Validate and make the step; RecipeError saying what is wrong and what is valid (`unknown` keywords, which
        pydantic's own validation may pass, are refused by name).

        preset: the name of one of the effect's presets (see `registry.presets`). The step holds the preset's
        parameters with `params` laid over them, so a parameter you give wins; an effect without that preset is a
        RecipeError listing the ones it has. opacity: None takes the effect's default (`registry.opacity`, declared with
        `@effect(opacity=...)`, usually 1); a number replaces it. partition: the region an effect that takes the input
        `partition` draws (required for it, refused for any other effect)."""
        values = dict(params or {})
        if preset is not None:
            found = registry.presets(effect)  # an unknown effect: UnknownEffectError listing them
            if preset not in found:
                has = f"its presets: {', '.join(found)}" if found else "it has no presets"
                raise RecipeError(f"effect {effect!r} has no preset {preset!r}; {has}")
            values = {**found[preset].params, **values}
        if opacity is None:
            opacity = registry.opacity(effect)  # an unknown effect: UnknownEffectError listing them
        self._init_checked(
            "",
            effect=effect,
            params=values,
            mask=mask,
            opacity=opacity,
            blend_mode=blend_mode,
            partition=partition,
            **unknown,
        )

    @model_validator(mode="after")
    def _valid_for_the_effect(self) -> "Step":
        """The parameters, checked by the effect itself."""
        for name in registry.INPUTS:
            if name in self.params:
                raise RecipeError(f"`{name}` is supplied by the recipe itself and cannot be set")
        for name in registry.STEP_KEYS:
            if name in self.params:
                raise RecipeError(
                    f"`{name}` is a key of the step, not a parameter of {self.effect!r}: give it as {name}="
                )
        registry.get(self.effect).validate(self.params)  # an unknown effect: UnknownEffectError listing them
        takes = "partition" in registry.inputs(self.effect)
        if takes and self.partition is None:
            raise RecipeError(
                f"effect {self.effect!r} draws along the borders of a partition: give `partition`, a region"
            )
        if self.partition is not None and not takes:
            raise RecipeError(f"effect {self.effect!r} takes no partition: remove `partition`")
        return self

    @property
    def runs(self) -> tuple["Step", ...]:
        """The effect steps this step runs: itself."""
        return (self,)

    def references(self) -> list[Reference]:
        """The regions this step names, as `(key, name, kind)`: its mask, and the partition it draws."""
        drawn: list[Reference] = [] if self.partition is None else [("partition", self.partition, None)]
        return [*super().references(), *drawn]

    def describe(self, width: int = 56) -> str:
        """A short readable form, e.g. `fill(color='#000000') on background` or `glow() screen x0.8` (the blend mode,
        then the opacity), cut to `width` characters."""
        shown = ", ".join(f"{k}={_short(v)}" for k, v in self.params.items())
        return _described(f"{self.effect}({shown})", self, width)

    def __str__(self) -> str:
        """The short readable form (see `describe`)."""
        return self.describe()


class Each(_Layer):
    """A look for each piece of a partition: every piece shows one of `looks`, the looks taking turns or drawn at
    random, and the pieces blend into each other a little at their borders.

    Each look is rendered once over the whole picture and shown through its pieces, so the cost is the number of looks,
    not of pieces. The pieces' weights add up to 1 at every pixel; then `mask`, `opacity` and `blend_mode` lay the
    blended picture over the step's input like any step's result. A look is a `Step`, with its own `opacity` and
    `blend_mode` (and `mask`), or the word "none": the piece stays as it was.
    """

    each: str = Field(
        description="The name of the partition whose pieces get a look each (a grid, shards, cuts or svg region)."
    )
    looks: tuple[Step | Literal["none"], ...] = Field(
        min_length=1,
        description='The looks: each a step, or "none" for a piece left as it is. Piece k gets look k, then they '
        "start again (assign='cycle'), or each piece draws one at random (assign='random').",
    )
    assign: Literal["cycle", "random"] = Field(
        "cycle",
        description="How pieces get looks: 'cycle' gives the pieces, in reading order, the looks in turn; 'random' "
        "gives each piece a look drawn at random (the same `seed` always draws the same).",
    )
    seed: int = Field(
        0,
        ge=0,
        le=2**32 - 1,
        strict=True,
        description="Random seed of assign='random': the same seed always gives the same looks to the same pieces.",
    )
    feather: float = Field(
        0.001,
        ge=0,
        le=0.05,
        strict=True,
        allow_inf_nan=False,
        description="How softly a piece's look fades into its neighbours' across a border, as a share of the "
        "picture's long side (0 to 0.05): the standard deviation of a Gaussian. 0 gives hard borders; about 0.001 "
        "only smooths the stair-steps of the pixels; raising it blends the looks over a wider band.",
    )

    def __init__(
        self,
        each: str,
        looks: Sequence[Step | str],
        mask: str | None = None,
        opacity: float | None = None,
        blend_mode: str = "normal",
        assign: str = "cycle",
        seed: int = 0,
        feather: float = 0.001,
        **unknown: Any,
    ) -> None:
        """Validate and make the step; RecipeError saying what is wrong and what is valid. opacity: None shows the
        blended looks fully (1); a number mixes them over the input."""
        self._init_checked(
            "each step",
            each=each,
            looks=tuple(looks),
            mask=mask,
            opacity=1.0 if opacity is None else opacity,
            blend_mode=blend_mode,
            assign=assign,
            seed=seed,
            feather=feather,
            **unknown,
        )

    def assignment(self, pieces: int) -> list[int]:
        """The look (an index into `looks`) of each of `pieces` pieces, in reading order."""
        if self.assign == "cycle":
            return [piece % len(self.looks) for piece in range(pieces)]
        return np.random.default_rng(self.seed).integers(0, len(self.looks), pieces).tolist()

    @property
    def runs(self) -> tuple[Step, ...]:
        """The effect steps this step runs: its looks, but the "none" ones."""
        return tuple(look for look in self.looks if isinstance(look, Step))

    def references(self) -> list[Reference]:
        """The regions this step names, as `(key, name, kind)`: its mask, its partition, and its looks' regions."""
        return [
            *super().references(),
            ("each", self.each, "partition"),
            *(reference for look in self.runs for reference in look.references()),
        ]

    def describe(self, width: int = 56) -> str:
        """A short readable form, e.g. `each panes [grade(exposure=0.5), none] on subject x0.5`, cut to `width`."""
        looks = ", ".join(str(look) for look in self.looks)
        return _described(f"each {self.each} [{looks}]", self, width)

    def __str__(self) -> str:
        """The short readable form (see `describe`)."""
        return self.describe()


def _described(text: str, layer: _Layer, width: int) -> str:
    """`text` followed by what the layer adds (blend mode, mask, opacity), cut to `width` characters."""
    if layer.blend_mode != "normal":
        text += f" {layer.blend_mode}"
    if layer.mask:
        text += f" on {layer.mask}"
    if layer.opacity < 1.0:
        text += f" x{layer.opacity:g}"
    return text if len(text) <= width else text[: width - 1] + "…"


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
    """A named look: ordered steps, how the photo is framed before them and the size it is scaled to.

    name: used for output file names, so it must be a safe one: 1 to 64 letters, digits, `.`, `_` or `-`, starting with
        a letter or digit (no folders, no hidden files), and not a device name Windows reserves (CON, PRN, AUX, NUL,
        COM1-9, LPT1-9, in any case, with or without an extension); anything else is a RecipeError. `safe_name` makes
        a valid one from any text.
    steps: at least one `Step` or `Each`, run in order (a list is accepted and kept as a tuple).
    regions: the regions the steps can name besides the built-in `subject` and `background`: a name for each `Grid`,
        `Shards`, `Cuts`, `Path`, `Svg`, `Pick` or `Subject` (see `bullterrier_image.core.regions`). They are checked
        here, and made for each picture only when a step uses them.
    size: longest side of the working image in px, >= 1 (effect sizes such as cell or radius are relative to it), or
        None to keep the photo's own size. The photo is framed first (flip, turn, crop, borders) and then scaled.
    frame: a `Framing` (default: none): flips, turns, crops and pads the photo, the subject matte and the depth map
        together before the first step, and may cut the finished picture out (a circle, rounded corners, the subject)
        after the last, so the render has an alpha channel.
    """

    name: str = Field(
        pattern=_NAME_PATTERN,
        description="Used for output file names: 1 to 64 of A-Z a-z 0-9 . _ -, starting with a letter or digit.",
    )
    steps: tuple[Step | Each, ...] = Field(description="The steps, run in order, each on the previous one's output.")
    regions: dict[str, Kind] = Field(
        default_factory=dict,
        description="The regions the steps name, by name, besides the built-in 'subject' and 'background'.",
    )
    size: int | None = Field(
        1024, ge=1, strict=True, description="Longest side of the working image in px; None keeps the photo's size."
    )
    frame: Framing = Field(
        default_factory=Framing, description="How the photo is framed before the steps and cut out after them."
    )

    def __init__(
        self,
        name: str,
        steps: Sequence[Step | Each],
        size: int | None = 1024,
        frame: Framing | None = None,
        regions: Mapping[str, Kind] | None = None,
        **unknown: Any,
    ) -> None:
        """Validate and make the recipe; RecipeError saying what is wrong (unknown keywords are refused by name).
        `frame` is the recipe's `Framing`; without one the photo is only scaled. `regions` are the declared regions,
        by name; every region a step names must be among them or built in, and of the kind its key takes."""
        given = {} if frame is None else {"frame": frame}
        self._init_checked(
            "", name=name, steps=tuple(steps), size=size, regions=dict(regions or {}), **given, **unknown
        )

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
        check(self.regions)
        for number, step in enumerate(self.steps, start=1):
            for key, name, wanted in step.references():
                needs(f"step {number}", key, name, wanted, self.regions)
        return self

    @property
    def needs_matte(self) -> bool:
        """True if a step takes the matte, or a region a step uses is made from the subject (the subject or background
        itself, or a region laid over it), or the framing crops around the subject or keeps only it: rendering must
        find the subject first (the slow part). Regions that are declared but not used need nothing."""
        return self.frame.needs_matte or bool(self.taking("matte")) or needs_subject(self.regions, self._names_used())

    @property
    def needs_depth(self) -> bool:
        """True if a step's effect, or a look's, takes the depth map: rendering must be given it or a depth finder."""
        return bool(self.taking("depth"))

    def taking(self, name: str) -> list[tuple[int, str]]:
        """The steps whose effect, or the effect of one of whose looks, takes the injected input `name` (one of
        `registry.INPUTS`), as (step number counted from 1, effect), in order."""
        return [
            (number, run.effect)
            for number, step in enumerate(self.steps, start=1)
            for run in step.runs
            if name in registry.inputs(run.effect)
        ]

    @property
    def needs_finder(self) -> bool:
        """True if a region a step uses finds the subject piece by piece (a `Subject` with `per`): rendering must be
        given a subject finder, whatever else it is given, because it is asked about parts of the working picture, which
        no matte of the whole photo answers."""
        return needs_finder(self.regions, self._names_used())

    def _names_used(self) -> list[str]:
        """The names of the regions the steps refer to."""
        return [name for step in self.steps for _, name, _ in step.references()]
