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

Dependency direction: imports the effect registry, `native` (the core's rules, which a recipe is held to when it is
made), `framing`, `params`, `regions`, `errors` only. The recipe-file
reader `loader` (TOML to `Recipe`, in the adapters layer) and `bridge` (running a `Recipe` on a photo in the Rust core)
both build on this module and do not know each other.
"""

import re
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from importlib import import_module
from typing import Any, Literal, cast

import numpy as np
from pydantic import Field, field_validator, model_validator

from bullterrier_image.errors import RecipeError

from . import native, registry
from .framing import Framing
from .params import (
    Drive,
    EffectParams,
    ValidatedModel,
    arrays_as_values,
    bounds,
    cyclic_of,
    field_name_fields,
    picture_fields,
    unit_of,
)
from .regions import Kind
from .values import BlendMode, Lut, Picture

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
        /,
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
            if not isinstance(preset, str) or preset not in found:  # a recipe file may write any value
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
        """The step, checked by the core as a recipe of one step (the names of regions are the recipe's to check)."""
        _check_alone(self)
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
        # fresh process too (an unpickled step in a pool worker); the structural fix is default finders found through
        # entry points.
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
        /,
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


# What the recipe's own checks and the bridge ask of steps and recipes, kept out of the public classes.


def runs(step: Step | Each) -> tuple[Step, ...]:
    """The effect steps `step` runs: a `Step` itself, an `Each` its looks but the None ones."""
    return (step,) if isinstance(step, Step) else tuple(look for look in step.looks if isinstance(look, Step))


def drives(step: Step) -> dict[str, Drive]:
    """The parameters of `step` a field varies across the picture, by name; empty for a step of plain values."""
    return {name: value for name, value in step.params.items() if isinstance(value, Drive)}


Hold = Callable[
    [object], object
]  # `registry.holding`'s: keeps an object, returns the reference or name to put in its place


def recipe_dict(recipe: "Recipe", hold: Hold) -> dict[str, Any]:
    """`recipe` as the dict the core reads (the form of `_core.read_recipe`): a key is there when it is set. A step's
    preset is already in its parameters. A parameter that cannot be written as data, a `Picture` or a `Lut`, is
    `hold`'s (`registry.holding`): the dict has the reference `hold` returns."""
    document: dict[str, Any] = {"steps": [_entry(step, hold) for step in recipe.steps]}
    for key in ("name", "description", "size"):
        if (value := getattr(recipe, key)) is not None:
            document[key] = value
    if recipe.frame != Framing():
        document["frame"] = recipe.frame.model_dump()
    if recipe.regions:
        document["regions"] = {name: _region(region) for name, region in recipe.regions.items()}
    return document


def _entry(step: "Step | Each | None", hold: Hold) -> dict[str, Any] | None:
    if step is None:  # a piece of an `each` step left as it was
        return None
    if isinstance(step, Each):
        entry: dict[str, Any] = {
            "each": step.each,
            "looks": [_entry(look, hold) for look in step.looks],
            "assign": step.assign,
            "seed": step.seed,
            "feather": step.feather,
        }
    else:
        entry = {"effect": step.effect}
        if step.params:
            pictures = picture_fields(registry.parameters(step.effect))
            entry["params"] = {
                # a colour written for a picture is a flat one, in Python only: the core takes names and paths
                name: _value(_flat(step.effect, name, value) if name in pictures and _is_colour(value) else value, hold)
                for name, value in step.params.items()
            }
        if step.partition is not None:
            entry["partition"] = step.partition
    entry |= {"opacity": step.opacity, "blend_mode": step.blend_mode}
    for key in ("mask", "name"):
        if (value := getattr(step, key)) is not None:
            entry[key] = value
    return entry


def _flat(effect: str, name: str, colour: object) -> Picture:
    """The flat picture of the colour written for the picture parameter `name`; RecipeError naming the effect and it."""
    try:
        return Picture.of_color(colour)
    except ValueError as error:
        raise RecipeError(f"effect {effect!r}: `{name}`: {error}") from None


def _region(region: Kind) -> dict[str, Any]:
    """A region as the core reads it: its fields and its `kind`, the class's name in snake case (`LinearGradient` is
    `linear_gradient`)."""
    kind = re.sub(r"(?<!^)(?=[A-Z])", "_", type(region).__name__).lower()
    return {"kind": kind, **region.model_dump()}


def _is_colour(value: object) -> bool:
    """Whether `value` is a colour written as text (`'#rrggbb'`, `'#rgb'`)."""
    return isinstance(value, str) and value.startswith("#")


def _value(value: object, hold: Hold) -> object:
    """A parameter value as the plain data it is written as: a drive as its table, an array as a list, a `Picture` or
    a `Lut` as the reference `hold` gives."""
    if isinstance(value, Picture | Lut):
        return hold(value)
    if isinstance(value, Drive):
        return value.model_dump()
    if isinstance(value, Mapping):
        return {key: _value(item, hold) for key, item in value.items()}
    if isinstance(value, list | tuple):
        return [_value(item, hold) for item in value]
    if isinstance(value, np.ndarray):
        return value.tolist()
    if isinstance(value, np.generic):
        return value.item()
    return value


def _step_from_dict(written: Mapping[str, Any]) -> Step | Each:
    """The `Each` (it has `each` or `looks`) or the `Step` (it has `effect`) that the dict `written` describes, made by
    their own constructors; ValueError otherwise."""
    if "each" in written or "looks" in written:
        if not {"each", "looks"} <= written.keys():
            raise ValueError("an each step written as a dict needs both `each` and `looks`")
        return Each(**written)
    if "effect" not in written:
        raise ValueError(f"a step written as a dict needs `effect` (a step) or `each` and `looks`, got {list(written)}")
    return Step(**written)


def _as_drive(effect: str, name: str, value: Any) -> Any:
    """`value`, or the `Drive` it writes when it is a table with a `by` (what a recipe file gives); RecipeError naming
    the effect and the parameter when the table is not a drive."""
    if not (isinstance(value, dict) and "by" in value):
        return value
    try:
        return Drive(**value)
    except RecipeError as error:
        raise RecipeError(f"effect {effect!r}: `{name}`: {error}") from None


_ALONE = re.compile(r"^step 1: ")  # the place the core names in a recipe of one step


def _check_alone(step: "Step") -> None:
    """RecipeError (UnknownEffectError for an unknown effect) unless the core accepts `step` as a recipe of one step:
    its effect, parameters, drives and `partition`. The regions a `mask`, a `partition` or a drive's `by` name are the
    recipe's to check, so the step is put to the core with stand-ins for them: no mask, a declared grid for the
    partition, `light` for each field parameter, and for each different `by` a different built-in selection."""
    with registry.holding() as hold:
        entry = _entry(step, hold)
    entry = cast("dict[str, Any]", entry)
    entry.pop("mask", None)
    document: dict[str, Any] = {"steps": [entry]}
    if "partition" in entry:
        entry["partition"], document["regions"] = "stand_in", {"stand_in": {"kind": "grid"}}
    named = field_name_fields(registry.parameters(step.effect))  # the selections a field parameter names
    stand_ins: dict[
        str, str
    ] = {}  # each field a drive varies by, as one of the built-in selections, a different one each
    for name, value in entry.get("params", {}).items():
        if name in named and not isinstance(value, dict):
            entry["params"][name] = "light"
        elif isinstance(value, dict) and "by" in value:
            value["by"] = stand_ins.setdefault(
                str(value["by"]), ("light", "depth", "subject", "background")[len(stand_ins) % 4]
            )
    try:
        native.check_recipe(document)
    except RecipeError as error:
        raise RecipeError(_ALONE.sub("", str(error))) from None


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
    size: longest side of the working image in px, 1 or more (at most 67,108,864 pixels); None (the default) keeps
        the photo's own size. The photo
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
        description="The longest side in px the recipe renders at, 1 or more (at most 67,108,864 pixels); None: the "
        "photo's own. Its px values "
        "are always written for 480.",
    )
    frame: Framing = Field(
        default_factory=Framing, description="How the photo is framed before the steps and cut out after them."
    )

    def __init__(
        self,
        /,
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
        if isinstance(steps, str | Mapping | Step | Each):  # iterating one of these would give its fields or letters
            raise RecipeError(
                f"`steps`: a recipe's steps are a list, Recipe([step, ...]), got {type(steps).__name__}: "
                "wrap one step in a list"
            )
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

    @field_validator("steps", mode="before")
    @classmethod
    def _steps_from_dicts(cls, steps: object) -> object:
        """A step written as a dict (what `model_dump` gives, or a file read into data) is a `Step`, or an `Each` when
        it has `each` or `looks`; ValueError, naming the keys, when it is neither."""
        if not isinstance(steps, list | tuple):
            return steps  # pydantic says what is wrong
        return tuple(_step_from_dict(step) if isinstance(step, Mapping) else step for step in steps)

    @field_validator("regions", mode="before")
    @classmethod
    def _regions_from_dicts(cls, regions: object) -> object:
        """A region written as a dict is read as the kind whose fields it has, which only a dict with fields can name:
        `Given()` dumps to `{}`, which fits every kind (R3's canonical form names the kind), so an empty one is refused
        rather than read as another kind."""
        if isinstance(regions, Mapping):
            for name, region in regions.items():
                if isinstance(region, Mapping) and not region:
                    raise ValueError(
                        f"region {name!r} is an empty dict, which fits every kind of region: pass the region itself "
                        "(`Given()`)"
                    )
        return regions

    @model_validator(mode="after")
    def _complete(self) -> "Recipe":
        if not self.steps:
            raise RecipeError("a recipe needs at least one step")
        with registry.holding() as hold:  # the rules the core runs by, those of a region's path among them
            native.check_recipe(recipe_dict(self, hold))
        return self

    def parameters(self) -> dict[str, Parameter]:
        """Every number of the recipe that can be set, by address, in the order of the recipe: the numbers of each step
        (`pulse.gain`; for a driven one `pulse.gain.start`, `.end` and `.levels`), its `opacity`, an `Each`'s `feather`
        and its looks' numbers (`each.looks[0].gain`), then the numbers of each region (`region.near.low`), then of the
        frame (`frame.angle`). A step is addressed by its `name`, else by its effect (`gain`) when the recipe has one
        such step, else by the effect and its number among them (`gain#2`; an `Each` is `each`, `each#2`); the other
        forms an address may take are listed in `with_values`. Each is described by a `Parameter`: its value now (at its
        default where the recipe does not write it), unit, range and what it does. The core knows the addresses
        (`Recipe::numbers`)."""
        document, foreign = self._document(), self._foreign()
        return {found["address"]: _parameter(found) for found in native.numbers(document, foreign)}

    def parameter(self, address: str) -> Parameter:
        """The number at `address` (see `parameters`; any form an address may take); RecipeError naming the address
        when there is none, with what is valid."""
        return _parameter(native.locate(self._document(), address, self._foreign())["number"])

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
        document, foreign = self._document(), self._foreign()
        changes: dict[tuple, dict[str, Any]] = {}
        where: dict[tuple, dict[str, str]] = {}  # the address that set each number
        for address, value in values.items():
            located = native.locate(document, address, foreign)
            target, rest = tuple(located["place"]), located["rest"]
            if isinstance(value, np.generic):
                value = value.item()  # a number from numpy is the Python number it holds
            if located["number"]["whole"] and isinstance(value, float) and value.is_integer():
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

    def _foreign(self) -> dict[str, list[dict[str, Any]]]:
        """The numbers of the effects this recipe uses that are written in Python, as the core cannot read them from a
        table of its own: the declaration of each field that has a range."""
        steps = [look for step in self.steps for look in (step.looks if isinstance(step, Each) else (step,))]
        written = {step.effect for step in steps if isinstance(step, Step)}
        foreign: dict[str, list[dict[str, Any]]] = {}
        for effect in sorted(written):
            model = registry.parameters(effect)
            if model._core_effect is not None:  # noqa: SLF001  (the model says which Rust declaration it is)
                continue
            foreign[effect] = [
                {
                    "name": name,
                    "unit": unit_of(info) or "",
                    "doc": info.description or "",
                    "min": bounds(info)[0],
                    "max": bounds(info)[1],
                    "whole": info.annotation is int,
                    "cyclic": cyclic_of(info),
                    "default": info.default,
                }
                for name, info in model.model_fields.items()
                if bounds(info)[0] is not None
            ]
        return foreign

    def _document(self) -> dict[str, Any]:
        """The recipe as the dict the core reads."""
        with registry.holding() as hold:
            return recipe_dict(self, hold)


def _parameter(found: Mapping[str, Any]) -> Parameter:
    """The `Parameter` that the core's description `found` of a number gives."""
    return Parameter(
        found["value"], found["unit"], found["min"], found["max"], found["doc"], found["whole"], found["cyclic"]
    )


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
