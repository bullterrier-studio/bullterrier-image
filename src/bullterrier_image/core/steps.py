"""The data model of a look: a `Step` (one effect, its parameters, and how it is laid over the picture below it: a
mask, an opacity and a blend mode), an `Each` (a look for each piece of a partition) and a `Recipe` (an ordered
tuple of steps with the regions they name, an output size and a `Framing`).

Promises:
- Everything is validated when it is made, wherever it comes from (a TOML file, `fx`, or your own code): an unknown
  effect, a misspelt or out-of-range parameter, a bad opacity or blend mode, or an empty recipe fails at once
  with a `RecipeError` (a `ValueError`; an unknown effect is an `UnknownEffectError`, also a `KeyError`) saying what is
  valid. A step's parameters are checked by its effect's own parameter model.
- The regions a step names (`mask`, `by`, `partition`, `each`) are checked when the recipe is made, which alone knows
  the regions it declares: a name that is no region, or is a region of the wrong kind, is a `RecipeError` naming the
  step and listing the valid names.
- Steps and recipes are immutable values (frozen Pydantic models). A step keeps its parameters as they were written,
  so `describe` shows them that way; with a `preset` they are the preset's parameters under the ones given, which is
  all the runner and the cache see of it.
- `Step.describe` gives a short readable form.
- A number an effect declares drivable may be a `Drive` (see `params`): it is validated at every level when the step is
  made. Every number of a recipe has an address (`Recipe.parameters`, `Recipe.parameter`), and `Recipe.with_values`
  returns a copy with some set; a step's optional `name` addresses its numbers.

Dependency direction: imports the effect registry, `framing`, `params`, `regions`, `ops.composite` (the blend modes'
names) and `errors` only. The recipe-file reader
`loader` (TOML to `Recipe`, in the adapters layer) and `runner` (running a `Recipe` on a photo) both build on this
module and do not know each other.
"""

import difflib
import re
from collections.abc import Mapping, Sequence
from dataclasses import dataclass, replace
from importlib import import_module
from typing import Any, Literal

import numpy as np
from pydantic import Field, field_validator, model_validator
from pydantic.fields import FieldInfo

from bullterrier_image.errors import RecipeError
from bullterrier_image.ops.composite import BlendMode

from . import registry
from .framing import Framing
from .framing import needs_subject as _framing_needs_subject
from .params import (
    Drive,
    EffectParams,
    ValidatedModel,
    arrays_as_values,
    bounds,
    cyclic_of,
    drivable_fields,
    unit_of,
    whole_number_fields,
)
from .regions import Kind, check, maps_used, needs
from .regions import derived_maps as _derived_from
from .regions import needs_finder as _finds_pieces

_NAME = re.compile(r"[A-Za-z][A-Za-z0-9_]*")
_ADDRESS_WORDS = ("region", "frame", "each")  # what an address starts with besides a step: a step cannot be called so


@dataclass(frozen=True)
class Parameter:
    """One number of a recipe, as `Recipe.parameters` lists it: its `value` now (None for an optional number the recipe
    leaves unset, such as the framing's `aspect`), its `unit` (`px`, `fraction`, `count`...) and range, what it does
    (`doc`), whether it must be a whole number (`whole`: a count, a seed) and whether it wraps (`cyclic`: an angle
    whose range is one turn and that takes any value, wrapped into it). The unit and range are those of the number's
    own declaration, so a control that maps a signal onto it can scale to them and stay inside, or wrap if cyclic."""

    value: float | None
    unit: str
    minimum: float | None
    maximum: float | None
    doc: str
    whole: bool = False
    cyclic: bool = False


# One reference a step makes to a region: the key it is written in, the name, and the kind of region that key takes.
Reference = tuple[str, str, Literal["partition", "selection"] | None]


class _Layer(ValidatedModel):
    """How a step's result is laid over the picture below it (its input, the backdrop): what `Step` and `Each` share.

    The runner combines the input and the step's result (the source) by `blend_mode`, then mixes that over the input by
    `opacity`, only where `mask` says.
    """

    mask: str | None = Field(
        None,
        description='Apply only where this selection says: the "subject", "background", "depth" or "light" of the '
        "photo, or the name of a selection the recipe declares (path, pick, subject, band, combine, a gradient or "
        'given); None: everywhere. Where the subject map is empty "subject" selects nothing and "background" '
        "everything.",
    )
    opacity: float = Field(
        ge=0,
        le=1,
        strict=True,
        allow_inf_nan=False,
        json_schema_extra={"unit": "fraction"},
        description="How much of the step shows: 0 none, 1 fully; when none is given, the effect's own default "
        "(`registry.opacity`, 1 for most effects, and for an `each` step).",
    )
    blend_mode: BlendMode = Field(
        "normal",
        description="How the result combines with the step's input, as the W3C blend modes: 'normal' shows the result, "
        "'multiply' darkens, 'screen' lightens, 'overlay' and 'soft_light' deepen contrast, 'difference' subtracts, "
        "'color' takes the result's hue and saturation and keeps the input's brightness.",
    )

    name: str | None = Field(
        None,
        description="A name for the step, so its numbers have addresses that survive reordering: letters, digits and "
        "underscores, starting with a letter; no two steps of a recipe share one, and it is not `region`, `frame` or "
        "`each` nor the name of an effect the recipe uses. None: the step is reached by its effect (and its number "
        "among the steps of that effect).",
    )

    @field_validator("name")
    @classmethod
    def _a_word(cls, name: str | None) -> str | None:
        """A name is a word that cannot be taken for the rest of an address."""
        if name is not None and not _NAME.fullmatch(name):
            raise ValueError(f"must be letters, digits and underscores, starting with a letter, got {name!r}")
        if name in _ADDRESS_WORDS:
            raise ValueError(f"{name!r} is reserved: an address starts with `region`, `frame` or `each`")
        return name


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
        *,
        params: Mapping[str, Any] | None = None,
        mask: str | None = None,
        opacity: float | None = None,
        blend_mode: str = "normal",
        preset: str | None = None,
        partition: str | None = None,
        name: str | None = None,
        **unknown: Any,
    ) -> None:
        """Validate and make the step; RecipeError saying what is wrong and what is valid (`unknown` keywords, which
        pydantic's own validation may pass, are refused by name).

        preset: the name of one of the effect's presets (see `registry.presets`). The step holds the preset's
        parameters with `params` laid over them, so a parameter you give wins; an effect without that preset is a
        RecipeError listing the ones it has. opacity: None takes the effect's default (`registry.opacity`, declared with
        `@effect(opacity=...)`, usually 1); a number replaces it. partition: the region an effect that takes the input
        `partition` draws (required for it, refused for any other effect)."""
        values = {name: _as_drive(effect, name, value) for name, value in (params or {}).items()}
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
            name=name,
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
        _check_drives(self.effect, self.params)  # an unknown effect: UnknownEffectError listing them
        takes = "partition" in registry.inputs(self.effect)
        if takes and self.partition is None:
            raise RecipeError(
                f"effect {self.effect!r} draws along the borders of a partition: give `partition`, a region"
            )
        if self.partition is not None and not takes:
            raise RecipeError(f"effect {self.effect!r} takes no partition: remove `partition`")
        return self

    def apply(
        self,
        photo: np.ndarray,
        *,
        maps: Mapping[str, np.ndarray] | None = None,
        finders: Mapping[str, Any] | None = None,
        size: int | None = None,
    ) -> np.ndarray:
        """This step alone on `photo`, without writing a recipe: exactly `bullterrier_image.render(photo,
        Recipe([step]), maps=maps, finders=finders, size=size)`, which documents every argument (`finders` holds a
        `Finder` by map name), including how the subject and the depth are found when the step needs them."""
        # A deliberate upward import (import-linter cannot see a call-time import): the public `render` binds the
        # default subject and depth finders, which live above the core. Importing it here, when called, works in a
        # fresh process too (an unpickled step in a pool worker); the structural fix is runner-owned default finders.
        render = import_module("bullterrier_image.recipe").render
        return render(photo, self, maps=maps, finders=finders, size=size)

    def replace(self, **changes: Any) -> "Step":
        """A copy with `changes`: the effect's parameters (laid over this step's) and the step keys (`mask`, `opacity`,
        `blend_mode`, `partition`, `name`; each replacing this step's), as `fx.<effect>(...)` takes them, checked as a
        new step is: a misspelt or out-of-range value is a RecipeError. A picture or lookup table parameter takes a
        `Picture`, a `Lut`, a colour or an array, as `fx` does; a path is read only by `fx`, which makes a step for
        it. Neither `effect` nor `preset` can be replaced: another effect is another step (`fx.<effect>(...)`), and a
        step does not know which of its values its preset gave, so `fx.<effect>(preset=...)` builds the step with
        another preset."""
        if "effect" in changes:
            raise RecipeError(
                f"`effect`: a copy of a step keeps its effect {self.effect!r} (got {changes['effect']!r}); build a "
                f"step of another effect with fx.{changes['effect']}(...)"
            )
        if "preset" in changes:
            raise RecipeError(
                f"`preset`: a copy of a step cannot take another preset, as it does not know which of its values the "
                f"old one gave; build the step again with fx.{self.effect}(preset={changes['preset']!r}, ...)"
            )
        keys = {key: changes.pop(key) for key in registry.STEP_KEYS if key in changes}  # the rest are the parameters
        held = {"mask": self.mask, "opacity": self.opacity, "blend_mode": self.blend_mode, "partition": self.partition}
        params = {**self.params, **arrays_as_values(registry.parameters(self.effect), changes)}
        return Step(self.effect, params=params, **{**held, "name": self.name, **keys})

    def describe(self, width: int = 56) -> str:
        """A short readable form, e.g. `gradient(colors=['#000000']) on background` or `bloom() screen x0.8` (the
        blend mode, then the opacity), cut to `width` characters."""
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
    `blend_mode` (and `mask`), or None: the piece stays as it was.
    """

    each: str = Field(
        description="The name of the partition whose pieces get a look each (grid, shards, cuts, svg or bands)."
    )
    looks: tuple[Step | None, ...] = Field(
        min_length=1,
        description="The looks: each a step, or None for a piece left as it is. Piece k gets look k, then they "
        "start again (assign='cycle'), or each piece draws one at random (assign='random').",
    )
    assign: Literal["cycle", "random"] = Field(
        "cycle",
        description="How pieces get looks: 'cycle' gives the pieces, piece 0 first, the looks in turn; 'random' "
        "gives each piece a look drawn at random (the same `seed` always draws the same).",
    )
    seed: int = Field(
        0,
        ge=0,
        le=2**32 - 1,
        strict=True,
        json_schema_extra={"unit": "count"},
        description="Random seed of assign='random': the same seed always gives the same looks to the same pieces.",
    )
    feather: float = Field(
        0.5,
        ge=0,
        le=24,
        strict=True,
        allow_inf_nan=False,
        json_schema_extra={"unit": "px"},
        description="How softly a piece's look fades into its neighbours' across a border, in px (0 to 24): the "
        "standard deviation of a Gaussian. 0 gives hard borders; about 0.5 only smooths the stair-steps of the "
        "pixels; raising it blends the looks over a wider band.",
    )

    def __init__(
        self,
        each: str,
        *,
        looks: Sequence[Step | None],
        mask: str | None = None,
        opacity: float | None = None,
        blend_mode: str = "normal",
        assign: str = "cycle",
        seed: int = 0,
        feather: float = 0.5,
        name: str | None = None,
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
            name=name,
            **unknown,
        )

    @field_validator("looks")
    @classmethod
    def _looks_take_no_name(cls, looks: tuple[Step | None, ...]) -> tuple[Step | None, ...]:
        """A look is reached by its place, `each.looks[0]`, so a name on it would address nothing."""
        for number, look in enumerate(looks):
            if isinstance(look, Step) and look.name is not None:
                raise ValueError(f"a look takes no `name`: its numbers are addressed by place, `each.looks[{number}]`")
        return looks

    def describe(self, width: int = 56) -> str:
        """A short readable form, e.g. `each panes [grade(exposure=0.5), none] on subject x0.5`, cut to `width`."""
        looks = ", ".join("none" if look is None else str(look) for look in self.looks)
        return _described(f"each {self.each} [{looks}]", self, width)

    def __str__(self) -> str:
        """The short readable form (see `describe`)."""
        return self.describe()


# What the runner (and the recipe's own checks) ask of steps and recipes, kept out of the public classes.


def runs(step: Step | Each) -> tuple[Step, ...]:
    """The effect steps `step` runs: a `Step` itself, an `Each` its looks but the None ones."""
    return (step,) if isinstance(step, Step) else tuple(look for look in step.looks if isinstance(look, Step))


def drives(step: Step) -> dict[str, Drive]:
    """The parameters of `step` a field varies across the picture, by name; empty for a step of plain values."""
    return {name: value for name, value in step.params.items() if isinstance(value, Drive)}


def fixed(step: Step) -> dict[str, Any]:
    """The parameters of `step` that are plain values, as written (the rest are `drives`)."""
    return {name: value for name, value in step.params.items() if not isinstance(value, Drive)}


def named_fields(step: Step) -> dict[str, str]:
    """The fields the effect of `step` works from, by the name of the parameter that names each, as written or by
    default (`{"field": "light"}`); empty for an effect with no field parameter."""
    return registry.get(step.effect).fields_named(step.params)


def references(step: Step | Each) -> list[Reference]:
    """The regions `step` names, as `(key, name, kind)`, which the recipe checks against its regions: its mask; for a
    `Step` the field that drives it (`by`, a selection), the fields its effect works from (`<effect>.<parameter>`,
    selections) and the partition it draws; for an `Each` its partition and its looks' regions."""
    masked: list[Reference] = [] if step.mask is None else [("mask", step.mask, "selection")]
    if isinstance(step, Each):
        looks = [reference for look in runs(step) for reference in references(look)]
        return [*masked, ("each", step.each, "partition"), *looks]
    driving: list[Reference] = [("by", by, "selection") for by in dict.fromkeys(d.by for d in drives(step).values())]
    working: list[Reference] = [
        (f"{step.effect}.{parameter}", field, "selection") for parameter, field in named_fields(step).items()
    ]
    drawn: list[Reference] = [] if step.partition is None else [("partition", step.partition, None)]
    return [*masked, *driving, *working, *drawn]


def assignment(each: Each, pieces: int) -> list[int]:
    """The look (an index into `each.looks`) of each of `pieces` pieces, piece 0 first."""
    if each.assign == "cycle":
        return [piece % len(each.looks) for piece in range(pieces)]
    return np.random.default_rng(each.seed).integers(0, len(each.looks), pieces).tolist()


def maps_needed(recipe: "Recipe") -> frozenset[str]:
    """The names of the maps of the photo that rendering `recipe` must be given or find: `subject` when the framing
    crops around it or keeps only it, or a region a step uses (a mask, a field, a drive's `by`) is made from it (the
    built-in `subject` or `background`, a `Subject` without `per`, or a region laid over one); `depth` likewise (the
    built-in `depth` or a region made from it, however indirectly); and the name of every `Given` region a step uses.
    Regions that are declared but not used need nothing; a `Subject` with `per` needs a finder, not a map (see
    `needs_finder`)."""
    framed = {"subject"} if _framing_needs_subject(recipe.frame) else set()
    return frozenset(framed | maps_used(recipe.regions, _names_used(recipe)))


def derived_maps(recipe: "Recipe") -> frozenset[str]:
    """The names of the maps a render of `recipe` makes from the working picture itself (`regions.DERIVED`: `light`,
    the picture's brightness) because a region a step uses is made from them, however indirectly. Nobody supplies
    these: they are not in `maps_needed`."""
    return frozenset(_derived_from(recipe.regions, _names_used(recipe)))


def needs_finder(recipe: "Recipe") -> bool:
    """True if a region a step of `recipe` uses finds the subject piece by piece (a `Subject` with `per`): rendering
    must be given a subject finder, whatever else it is given, because it is asked about parts of the working picture,
    which no map of the whole photo answers."""
    return _finds_pieces(recipe.regions, _names_used(recipe))


def _names_used(recipe: "Recipe") -> list[str]:
    """The names of the regions the steps of `recipe` refer to."""
    return [name for step in recipe.steps for _, name, _ in references(step)]


def _as_drive(effect: str, name: str, value: Any) -> Any:
    """`value`, or the `Drive` it writes when it is a table with a `by` (what a recipe file gives); RecipeError naming
    the effect and the parameter when the table is not a drive."""
    if not (isinstance(value, dict) and "by" in value):
        return value
    try:
        return Drive(**value)
    except RecipeError as error:
        raise RecipeError(f"effect {effect!r}: `{name}`: {error}") from None


def _check_drives(effect: str, params: Mapping[str, Any]) -> None:
    """RecipeError unless `params` (a step's values as written, some perhaps `Drive`s) are valid for `effect`: the
    plain ones as its model says, each driven one a drivable parameter whose every level value (start and end, and the
    steps between) is valid there, and all drives of the step varying one field in one number of levels."""
    model = registry.get(effect)
    fixed = {name: value for name, value in params.items() if not isinstance(value, Drive)}
    drives = {name: value for name, value in params.items() if isinstance(value, Drive)}
    model.validate(fixed)
    fields = model.params_model
    for name in drives:
        if name not in fields.model_fields:
            model.validate({name: 0})  # an unknown parameter: the model's own error, listing the valid ones
        if name not in drivable_fields(fields):
            raise RecipeError(_not_drivable(effect, name, fields))
    for what, kinds in (
        ("field", {d.by for d in drives.values()}),
        ("number of levels", {d.levels for d in drives.values()}),
    ):
        if len(kinds) > 1:
            shown = " and ".join(repr(kind) for kind in sorted(kinds, key=str))
            raise RecipeError(f"effect {effect!r}: the drives of one step must vary by one {what}, got {shown}")
    levels = next((d.levels for d in drives.values()), 0)
    for level in (*([0, levels - 1] if levels else []), *range(1, levels - 1)):  # the ends as written first
        model.validate({**fixed, **{name: drive.value(level) for name, drive in drives.items()}})


def _not_drivable(effect: str, name: str, fields: type[EffectParams]) -> str:
    """Why the parameter `name` of `effect`, whose model is `fields`, cannot be driven, and which can."""
    why = (
        "it is a whole number"
        if name in whole_number_fields(fields)
        else "it is not a number"
        if _kind(fields, name) != "number"
        else "its effect does not declare it drivable"
    )
    names = ", ".join(sorted(drivable_fields(fields))) or "none"
    return f"effect {effect!r}: `{name}` cannot be driven by a field ({why}); its drivable parameters: {names}"


def _kind(fields: type[EffectParams], name: str) -> str:
    """ "number" for a decimal number field, else the field's own type word."""
    return next(doc.type for doc in fields.fields_doc() if doc.name == name).split(" (")[0]


def _described(text: str, layer: _Layer, width: int) -> str:
    """`text` (after the layer's name, if it has one) followed by what the layer adds (blend mode, mask, opacity), cut
    to `width` characters."""
    if layer.name:
        text = f"{layer.name}: {text}"
    if layer.blend_mode != "normal":
        text += f" {layer.blend_mode}"
    if layer.mask:
        text += f" on {layer.mask}"
    if layer.opacity < 1.0:
        text += f" x{layer.opacity:g}"
    return text if len(text) <= width else text[: width - 1] + "…"


def _short(value: object) -> str:
    text = value.describe() if isinstance(value, Drive) else repr(value)
    return text if len(text) <= 18 else text[:15] + "..."


# The file name a recipe's name gives holds nothing that could leave the output folder or hide a file (CWE-22), on any
# operating system.
_NAME_CHARACTERS = "A-Za-z0-9._-"
_NAME_LENGTH = 64
# Windows keeps these names for devices, in any case and whatever the extension: such a file cannot be made there.
_WINDOWS_DEVICES = frozenset(
    {"CON", "PRN", "AUX", "NUL", *(f"COM{digit}" for digit in range(1, 10)), *(f"LPT{digit}" for digit in range(1, 10))}
)


def safe_name(text: str) -> str:
    """`text` (a recipe's name, a file name without its extension) made into a safe file name, or "" when none is left.

    Every run of characters other than letters, digits, `.`, `_` and `-` becomes one `-`, repeated `-` collapse into
    one, anything before the first letter or digit is dropped, the result is cut to 64 characters and a trailing `-` or
    `.` is trimmed: "my look" gives "my-look". "" when nothing remains or the result is a name Windows reserves for a
    device ("con")."""
    name = re.sub(r"-{2,}", "-", re.sub(rf"[^{_NAME_CHARACTERS}]+", "-", text))
    name = re.sub(r"^[^A-Za-z0-9]+", "", name)[:_NAME_LENGTH].rstrip("-.")
    return "" if not name or name.split(".", 1)[0].upper() in _WINDOWS_DEVICES else name


def file_name(recipe: "Recipe") -> str:
    """The name the files of `recipe` are given: its `name` made safe (`safe_name`). RecipeError naming the fix when
    the recipe has no name or its name leaves no safe file name."""
    if recipe.name is None:
        raise RecipeError('a recipe needs a name to name its files: give one, Recipe(steps, name="my-look")')
    stem = safe_name(recipe.name)
    if not stem:
        raise RecipeError(
            f"recipe {recipe.name!r}: its name leaves no file name (letters, digits, . _ - and not a device name "
            'Windows keeps, such as "con"): give another, name="my-look"'
        )
    return stem


class Recipe(ValidatedModel):
    """A look: ordered steps, how the photo is framed before them and the size it is scaled to, and optionally a name
    and a description.

    steps: at least one `Step` or `Each`, run in order (a list is accepted and kept as a tuple). The only argument
        given by position; the rest are keywords.
    name: free text naming the look, or None. Whoever writes files for it (the command line, a project) makes a safe
        file name of it.
    description: what the look looks like, in a sentence, or None.
    regions: the regions the steps can name besides the built-in `subject`, `background`, `depth` and `light`: a name
        for each `Grid`, `Shards`, `Cuts`, `Path`, `Svg`, `Pick`, `Subject`, `Band`, `Bands`, `Combine`,
        `LinearGradient`, `RadialGradient` or `Given` (see `bullterrier_image.core.regions`). They are checked here, and
        made for each picture only when a step uses them.
    size: longest side of the working image in px, >= 1; None (the default) keeps the photo's own size. The photo
        is framed first (flip, turn, crop, borders) and then scaled. It only sets how big the result is: the recipe's
        pixel values (a radius, a cell) are always written for a picture whose longest side is 480, so the same look
        results at any size; `render(..., size=)` renders at another size.
    frame: a `Framing` (default: none): flips, turns, crops and pads the photo and its maps (the subject, the depth,
        any map handed in) together before the first step, and may cut the finished picture out (a circle, rounded
        corners, the subject) after the last, so the render has an alpha channel.
    """

    steps: tuple[Step | Each, ...] = Field(description="The steps, run in order, each on the previous one's output.")
    name: str | None = Field(None, description="What the look is called, free text; None: unnamed.")
    description: str | None = Field(None, description="What the look looks like, in a sentence; None: undescribed.")
    regions: dict[str, Kind] = Field(
        default_factory=dict,
        description="The regions the steps name, by name, besides the built-in 'subject', 'background', 'depth' and "
        "'light'.",
    )
    size: int | None = Field(
        None,
        ge=1,
        strict=True,
        description="The longest side in px the recipe renders at; None: the photo's own. Its px values are always "
        "written for 480.",
    )
    frame: Framing = Field(
        default_factory=Framing, description="How the photo is framed before the steps and cut out after them."
    )

    def __init__(
        self,
        steps: Sequence[Step | Each],
        *,
        name: str | None = None,
        description: str | None = None,
        size: int | None = None,
        frame: Framing | None = None,
        regions: Mapping[str, Kind] | None = None,
        **unknown: Any,
    ) -> None:
        """Validate and make the recipe; RecipeError saying what is wrong (unknown keywords are refused by name).
        `frame` is the recipe's `Framing`; without one the photo is only scaled. `regions` are the declared regions,
        by name; every region a step names must be among them or built in, and of the kind its key takes."""
        given = {} if frame is None else {"frame": frame}
        self._init_checked(
            "",
            steps=tuple(steps),
            name=name,
            description=description,
            size=size,
            regions=dict(regions or {}),
            **given,
            **unknown,
        )

    def replace(self, **changes: Any) -> "Recipe":
        """A copy with `changes`: any of the fields `Recipe` takes (`steps`, `name`, `description`, `size`, `frame`,
        `regions`), each replacing this recipe's. Checked as a new recipe is: RecipeError for a bad or unknown field.
        To set numbers inside steps, regions or the frame by address, see `with_values`."""
        held = {field: getattr(self, field) for field in type(self).model_fields}
        return Recipe(**{**held, **changes})

    @model_validator(mode="after")
    def _complete(self) -> "Recipe":
        if not self.steps:
            raise RecipeError("a recipe needs at least one step")
        check(self.regions)
        for number, step in enumerate(self.steps, start=1):
            for key, name, wanted in references(step):
                needs(f"step {number}", key, name, wanted, self.regions)
        self._check_names()
        return self

    def _check_names(self) -> None:
        """RecipeError unless the step names are unique and none is the name of an effect the recipe uses (it would be
        taken for that effect in an address)."""
        used = {step.effect for step in self.steps if isinstance(step, Step)}
        first: dict[str, int] = {}
        for number, step in enumerate(self.steps, start=1):
            if step.name is None:
                continue
            if step.name in first:
                raise RecipeError(f"`name` {step.name!r} is the name of steps {first[step.name]} and {number}")
            if step.name in used:
                raise RecipeError(
                    f"step {number}: `name` {step.name!r} is also an effect this recipe uses; choose another"
                )
            first[step.name] = number

    def parameters(self) -> dict[str, Parameter]:
        """Every number of the recipe that can be set, by address, in the order of the recipe: the numbers of each step
        (`pulse.gain`; for a driven one `pulse.gain.start`, `.end` and `.levels`), its `opacity`, an `Each`'s `feather`
        and its looks' numbers (`each.looks[0].gain`), then the numbers of each region (`region.near.low`), then of the
        frame (`frame.angle`). A step is addressed by its `name`, else by its effect (`gain`) when the recipe has one
        such step, else by the effect and its number among them (`gain#2`; an `Each` is `each`, `each#2`); the other
        forms an address may take are listed in `with_values`. Each is described by a `Parameter`: its value now (at its
        default where the recipe does not write it), unit, range and what it does."""
        found: dict[str, Parameter] = {}
        for head, step in zip(self._heads(), self.steps, strict=True):
            found |= {f"{head}.{rest}": number for rest, number in _step_numbers(step).items()}
        for name, kind in self.regions.items():
            found |= {f"region.{name}.{field}": number for field, number in _numbers(kind).items()}
        return found | {f"frame.{field}": number for field, number in _numbers(self.frame).items()}

    def parameter(self, address: str) -> Parameter:
        """The number at `address` (see `parameters`; any form an address may take); RecipeError naming the address
        when there is none, with what is valid."""
        target, rest = self._reach(address, {})
        return self._numbers_of(target)[rest]

    def with_values(self, values: Mapping[str, float]) -> "Recipe":
        """A copy of the recipe with each number at an address of `values` set to its value; the recipe itself is left
        as it is, and every step, region and frame that is not touched is shared with the copy. An address is a key of
        `parameters`, or reaches the same number as one: a step by its effect (`gain.radius`) when the recipe has one,
        or by effect and number (`gain#1.radius`, counted from 1 among the steps of that effect). A whole-number value
        may be written as a float (`3.0`). Every changed step, region and frame is checked afresh, so a value out of
        its range is refused naming the address, the number and the range (a cyclic angle takes any real);
        so is an address the recipe does not have
        (listing the valid ones and the nearest), a driven number set by itself (set its `.start`, `.end` or `.levels`)
        and a `.start` of a number that is not driven."""
        changes: dict[tuple, dict[str, Any]] = {}
        where: dict[tuple, dict[str, str]] = {}  # the address that set each number
        cache: dict[tuple, dict[str, Parameter]] = {}
        for address, value in values.items():
            target, rest = self._reach(address, cache)
            if isinstance(value, np.generic):
                value = value.item()  # a number from numpy is the Python number it holds
            if self._numbers_of(target, cache)[rest].whole and isinstance(value, float) and value.is_integer():
                value = int(value)
            if rest in changes.get(target, {}):
                raise RecipeError(f"{where[target][rest]!r} and {address!r} reach the same number: give it one value")
            changes.setdefault(target, {})[rest] = value
            where.setdefault(target, {})[rest] = address
        steps, regions, frame = list(self.steps), dict(self.regions), self.frame
        for target, put in changes.items():
            try:
                match target:
                    case ("step", index):
                        steps[index] = _set_step(steps[index], put)
                    case ("region", name):
                        regions[name] = _set_numbers(regions[name], put)
                    case _:
                        frame = _set_numbers(frame, put)
            except RecipeError as error:
                blamed = [where[target][rest] for rest in put if _refuses(self, target, rest, put[rest])]
                raise RecipeError(
                    f"{', '.join(repr(address) for address in blamed or where[target].values())}: {error}"
                ) from None
        return self.replace(steps=steps, frame=frame, regions=regions)

    def _heads(self) -> list[str]:
        """What each step's address starts with, in order: its name, else its effect (`each` for an `Each`) when it is
        the recipe's only step of that kind, else the effect and its number among those (`gain#2`)."""
        kinds = [_called(step) for step in self.steps]
        seen: dict[str, int] = {}
        heads = []
        for step, kind in zip(self.steps, kinds, strict=True):
            seen[kind] = seen.get(kind, 0) + 1
            heads.append(step.name or (kind if kinds.count(kind) == 1 else f"{kind}#{seen[kind]}"))
        return heads

    def _reach(self, address: str, cache: dict[tuple, dict[str, Parameter]]) -> tuple[tuple, str]:
        """What `address` points at, as (the step, region or frame, the rest of the address): `(("step", index), rest)`,
        `(("region", name), rest)` or `(("frame",), rest)`; RecipeError naming the address when it points at nothing."""
        parts = address.split(".")
        if len(parts) < 2:
            raise RecipeError(
                f"{address!r}: not an address; write a step's number as `step.number`, a region's as "
                "`region.name.number` and the frame's as `frame.number` (see `Recipe.parameters`)"
            )
        head = parts[0]
        if head == "frame":
            target: tuple = ("frame",)
            rest, label = ".".join(parts[1:]), "'frame'"
        elif head == "region":
            if len(parts) < 3:
                raise RecipeError(f"{address!r}: a region's number is addressed `region.{parts[1]}.number`")
            name = ".".join(parts[1:-1])
            if name not in self.regions:
                raise RecipeError(
                    f"{address!r}: no region {name!r}; the recipe's regions: {', '.join(self.regions) or 'none'}"
                )
            target, rest, label = ("region", name), parts[-1], f"region {name!r}"
        else:
            target, rest, label = ("step", self._step_index(head, address)), ".".join(parts[1:]), repr(head)
        numbers = self._numbers_of(target, cache)
        if rest in numbers:
            return target, rest
        base, _, part = rest.rpartition(".")
        if f"{rest}.start" in numbers:
            raise RecipeError(
                f"{address!r}: {rest!r} is driven by a field: set {rest}.start, {rest}.end or {rest}.levels"
            )
        if base in numbers and part in ("start", "end", "levels"):
            raise RecipeError(f"{address!r}: {base!r} is not driven by a field, so it has no {part!r}")
        near = difflib.get_close_matches(address, list(self.parameters()), n=3)
        hint = f"; did you mean {', '.join(near)}?" if near else ""
        raise RecipeError(f"{address!r}: {label} has no number {rest!r}; its numbers: {', '.join(numbers)}{hint}")

    def _step_index(self, head: str, address: str) -> int:
        """The index of the step an address starts with `head` for: a step's name, its effect (`each` for an `Each`)
        when the recipe has one such step, or its effect and number (`gain#2`, from 1); RecipeError otherwise."""
        heads = self._heads()
        if head in heads:
            return heads.index(head)
        kind, _, number = head.partition("#")
        same = [index for index, step in enumerate(self.steps) if _called(step) == kind]
        if number and same:
            if number.isdigit() and 1 <= int(number) <= len(same):
                return same[int(number) - 1]
            raise RecipeError(f"{address!r}: no step {head!r}; the recipe has {len(same)} {kind!r} steps")
        if len(same) == 1 and not number:
            return same[0]  # the recipe's only step of that effect, though it has a name
        if len(same) > 1:
            numbered = ", ".join(f"{kind}#{n}.{address.partition('.')[2]}" for n in range(1, len(same) + 1))
            raise RecipeError(f"{address!r}: the recipe has {len(same)} {kind!r} steps; write one of {numbered}")
        listed = [*heads, *(f"region.{name}" for name in self.regions), "frame"]
        raise RecipeError(f"{address!r}: no step, region or frame called {head!r}; it has: {', '.join(listed)}")

    def _numbers_of(
        self, target: tuple, cache: dict[tuple, dict[str, Parameter]] | None = None
    ) -> dict[str, Parameter]:
        """The numbers (by the rest of an address) of the step, region or frame `target` (see `_reach`)."""
        if cache is not None and target in cache:
            return cache[target]
        match target:
            case ("step", index):
                found = _step_numbers(self.steps[index])
            case ("region", name):
                found = _numbers(self.regions[name])
            case _:
                found = _numbers(self.frame)
        if cache is not None:
            cache[target] = found
        return found


def _called(step: Step | Each) -> str:
    """What a step is called in an address when it has no name: its effect, or `each`."""
    return step.effect if isinstance(step, Step) else "each"


def _number(value: float, info: FieldInfo, *, whole: bool = False) -> Parameter:
    """The `Parameter` of the number of value `value` that the field `info` declares."""
    low, high = bounds(info)
    return Parameter(value, unit_of(info) or "", low, high, info.description or "", whole, cyclic_of(info))


def _numbers(model: EffectParams) -> dict[str, Parameter]:
    """The numbers of the parameter model `model` (an effect's, a region kind's or the framing), by field, at the values
    it holds: every field that has a range, so not a colour, a choice, a text or a list."""
    return {
        name: _number(getattr(model, name), info, whole=info.annotation is int)
        for name, info in type(model).model_fields.items()
        if bounds(info)[0] is not None
    }


def _step_numbers(step: Step | Each) -> dict[str, Parameter]:
    """The numbers of a step by the rest of an address: an effect's (a driven one as `.start`, `.end` and `.levels`),
    then `opacity`; an `Each`'s `feather`, `opacity` and its looks' as `looks[0].gain`."""
    if isinstance(step, Each):
        found = {
            "feather": _number(step.feather, Each.model_fields["feather"]),
            "opacity": _number(step.opacity, Each.model_fields["opacity"]),
            "seed": _number(step.seed, Each.model_fields["seed"], whole=True),
        }
        for index, look in enumerate(step.looks):
            if isinstance(look, Step):
                found |= {f"looks[{index}].{rest}": number for rest, number in _step_numbers(look).items()}
        return found
    found = {}
    for name, number in _numbers(registry.get(step.effect).validate(fixed(step))).items():
        if name in drives(step):
            drive = drives(step)[name]
            found[f"{name}.start"] = replace(number, value=drive.start)
            found[f"{name}.end"] = replace(number, value=drive.end)
            found[f"{name}.levels"] = _number(drive.levels, Drive.model_fields["levels"], whole=True)
        else:
            found[name] = number
    return found | {"opacity": _number(step.opacity, Step.model_fields["opacity"])}


def _set_step(step: Step | Each, put: Mapping[str, Any]) -> Step | Each:
    """`step` with the numbers `put` (by the rest of an address, see `_step_numbers`) set: a new step, checked."""
    return _set_each(step, put) if isinstance(step, Each) else _set_effect_step(step, put)


def _set_each(each: Each, put: Mapping[str, Any]) -> Each:
    """`each` with its `feather`, `opacity`, `seed` and the numbers of its looks (`looks[0].gain`) set from `put`."""
    own = {rest: value for rest, value in put.items() if not rest.startswith("looks[")}
    by_look: dict[int, dict[str, Any]] = {}
    for rest, value in put.items():
        if rest.startswith("looks["):
            number, _, inner = rest[len("looks[") :].partition("].")
            by_look.setdefault(int(number), {})[inner] = value
    looks = [
        _set_effect_step(look, by_look[index]) if isinstance(look, Step) and index in by_look else look
        for index, look in enumerate(each.looks)
    ]
    return Each(
        each.each,
        looks=looks,
        mask=each.mask,
        opacity=own.get("opacity", each.opacity),
        blend_mode=each.blend_mode,
        assign=each.assign,
        seed=own.get("seed", each.seed),
        feather=own.get("feather", each.feather),
        name=each.name,
    )


def _set_effect_step(step: Step, put: Mapping[str, Any]) -> Step:
    """`step` with its numbers, `opacity` and the ends of its drives set from `put`."""
    params, opacity = dict(step.params), step.opacity
    ends: dict[str, dict[str, Any]] = {}
    for rest, value in put.items():
        name, _, part = rest.partition(".")
        if rest == "opacity":
            opacity = value
        elif part:
            ends.setdefault(name, {})[part] = value
        else:
            params[rest] = value
    for name, parts in ends.items():
        params[name] = Drive(**{**params[name].model_dump(), **parts})
    return step.replace(**params, opacity=opacity)


def _set_numbers[M: EffectParams](model: M, put: Mapping[str, Any]) -> M:
    """`model` (a region kind or the framing) with the numbers `put` set; a new model, checked afresh."""
    return type(model)(**{**model.model_dump(), **put})


def _refuses(recipe: Recipe, target: tuple, rest: str, value: Any) -> bool:
    """Whether setting the one number `rest` of `target` (see `Recipe._reach`) to `value`, alone, is refused: how an
    error that came from several numbers set together is blamed on the ones that cause it."""
    try:
        match target:
            case ("step", index):
                _set_step(recipe.steps[index], {rest: value})
            case ("region", name):
                _set_numbers(recipe.regions[name], {rest: value})
            case _:
                _set_numbers(recipe.frame, {rest: value})
    except RecipeError:
        return True
    return False
