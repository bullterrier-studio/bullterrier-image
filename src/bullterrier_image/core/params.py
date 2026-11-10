"""How an effect declares its parameters: a frozen Pydantic model whose every field is described, ranged and defaulted.

    class BloomParams(EffectParams):
        \"\"\"Bloom around the bright parts of the picture.\"\"\"

        radius: float = param(24.0, unit="px", min=0, max=500, doc="Gaussian sigma of the halo. Larger = wider.")
        tint: Color = param("#ffffff", doc="Colour of the halo.")

Promises:
- A model is checked when its class is made (that is, when the effect module is imported): every field is declared with
  `param` and has a non-empty description; a number has a unit from `UNITS` and a range (min and max) that holds its
  default; any other field takes no unit or range. A broken
  declaration raises `ValueError` naming the model and the field, so it never reaches a recipe.
- Values are checked when a model is made, strictly, for every kind of field: unknown fields, values of the wrong kind
  (a bool or a numeric-looking string for a number, a string for a switch, a number for a choice) and values out of
  range are rejected; a whole number is fine for a decimal field, a list (or numpy array) for a list field. Models are
  frozen.
- `ValidatedModel.parse` is the one way callers validate: a mapping in, the model out, or one `RecipeError` (a
  `ValueError`) naming the owner, each field, the constraint and the value (`describe_error` is its wording). A more
  specific `RecipeError` raised by a validator (an `UnknownEffectError`) keeps its class when it is the only problem.
- `EffectParams.signature`, `fields_doc` and `documentation` describe a model for `fx`, `help()`, the catalogue and the
  command line, in a caller's types (a `Color` is shown as the forms one may write).
- A `Picture` field (a second picture an effect uses: a texture, a logo; named `image` when an effect has one) holds a
  `Picture`, read-only RGBA pixels with a content digest, or none (`Picture | None`, default none: the step leaves the
  photo as it is); it is given a `Picture` or a colour (a flat picture).
  Recipe files and `fx` turn a path or an array into a `Picture` before validation (see `loader`).
- A `FieldName` field (a selection an effect works from: `depth`, `light`, a band of the recipe) holds the name of a
  selection, as a step's `mask` and a `Drive`'s `by` do; the recipe checks the name, the runner builds the field and the
  effect receives its array (see `registry`). It is not a number: it has no address and cannot be driven.
- A `Lut` field (a 3D colour lookup table, from a `.cube` file) holds a `Lut`, a read-only table with a content digest;
  it must have a default such as `Lut.identity()`. Recipe files and `fx` turn a path or an array into a `Lut` before
  validation (see `loader`).
- `class XParams(EffectParams, core="x")`, with an empty body, is the model of an effect the Rust core runs: its fields,
  descriptions, defaults and ranges are the ones its Rust `Params` struct declares (`_core.params("x")`), literals as
  written (a whole number stays an `int`), and every promise above holds for it. `ValueError` if the core has no such
  effect. Python validates; the core trusts.
- `param(..., advanced=True)` changes only how a field is listed (after the basic ones, or as a count); it is validated,
  defaulted, in the signature and in the JSON schema (`"advanced"`) like any other.
"""

import difflib
import inspect
import math
import textwrap
import types
import typing
from collections.abc import Callable, Iterable, Mapping, Sequence
from dataclasses import dataclass, field
from typing import Annotated, Any, ClassVar, Literal, NamedTuple, Self, get_args, get_origin

import annotated_types
import numpy as np
from pydantic import (
    AfterValidator,
    BaseModel,
    BeforeValidator,
    ConfigDict,
    Field,
    ValidationError,
    model_validator,
)
from pydantic.fields import FieldInfo
from pydantic.json_schema import DEFAULT_REF_TEMPLATE, GenerateJsonSchema, JsonSchemaMode

from bullterrier_image.errors import RecipeError

from . import native
from .types import Color as ColorForms
from .values import Color, FieldName, Lut, LutForms, Picture, PictureForms, ValidatedLut, ValidatedPicture, is_number

UNITS = (
    "px",
    "fraction",
    "degrees",
    "stops",
    "count",
    "ratio",
    "cycles",
    "density",
)  # also `Unit` in crates/core/src/params.rs


def _as_tuple(value: object) -> object:
    """A list or a 1-d numpy array as the tuple a list field holds (TOML writes lists); anything else unchanged."""
    if isinstance(value, np.ndarray) and value.ndim == 1:
        return tuple(value.tolist())
    return tuple(value) if isinstance(value, list) else value


@dataclass(frozen=True)
class _Spec:
    """What `param` was told, until the model class turns it into a pydantic field (see `EffectParams`)."""

    default: Any
    doc: str
    unit: str | None
    min: float | None
    max: float | None
    advanced: bool
    drivable: bool = False
    cyclic: bool = False
    at_most: str | None = None
    field_kwargs: dict[str, Any] = field(default_factory=dict)


def param(
    default: Any,
    *,
    doc: str,
    unit: str | None = None,
    min: float | None = None,  # noqa: A002  (the documented name of the range's lower bound)
    max: float | None = None,  # noqa: A002  (the documented name of the range's upper bound)
    advanced: bool = False,
    drivable: bool = False,
    cyclic: bool = False,
    at_most: str | None = None,
    **field_kwargs: Any,
) -> Any:
    """Declare one field of an `EffectParams` model.

    default: the value an effect uses when none is given; it must lie in the range. doc: what the field does, its unit
    and range in words, and what raising it does (shown in `help()`, the catalogue and the command line). unit: one of
    `UNITS`, required for a number, not allowed otherwise. min, max: the documented range, both required and inclusive
    for a number; not allowed for anything else. advanced: the user would change it to tune the technique rather
    than to choose the look; it is listed after the basic fields (and left out of short listings) but is settable,
    validated and in the schema like any other. drivable: a field (see `Drive`) may vary this decimal number pixel by
    pixel; declare it only where the effect's output changes smoothly with the value. Only a float can be.
    cyclic: an angle in degrees whose value and the value a turn (360) further mean the same picture; `min` to `max`
    must be exactly one turn, and a value outside it is stored as the same direction inside it (a value inside, the
    bounds included, is kept). at_most: the name of another number field of the model, in the same unit, that this
    one may not exceed (a lowest height below the highest); checked when a model is made, whichever of the two is
    given. field_kwargs: further pydantic `Field` constraints, e.g. `min_length=2` for a list.

    The rules are checked when the model class is made; a violation raises `ValueError` naming the model and the field.
    """
    return _Spec(default, doc, unit, min, max, advanced, drivable, cyclic, at_most, field_kwargs)


def describe_error(error: ValidationError, owner: str, valid: Iterable[str]) -> str:
    """One sentence for a failed validation: `owner` (e.g. "effect 'bloom'"; "" for none), then each problem with its
    field, the constraint and the value given. `valid` names the fields, listed when an unknown one was given."""
    problems = "; ".join(_problem(detail, sorted(valid)) for detail in error.errors())
    return f"{owner}: {problems}" if owner else problems


def _invalid(error: ValidationError, owner: str, valid: Iterable[str]) -> RecipeError:
    """The `RecipeError` for a failed validation, in the wording of `describe_error`. Pydantic wraps a `ValueError`
    raised inside a validator; when that is the only problem and it is a more specific `RecipeError` (an unknown effect
    is also a `KeyError`), the caller gets that class back, so `except UnknownEffectError` works on a `Step` too."""
    problems = [detail.get("ctx", {}).get("error") for detail in error.errors()]
    single = problems[0] if len(problems) == 1 else None
    kind = type(single) if isinstance(single, RecipeError) else RecipeError
    return kind(describe_error(error, owner, valid))


class ValidatedModel(BaseModel):
    """Base of bullterrier-image's validated values (effect parameters, recipe steps, recipes): frozen, unknown fields
    refused,
    and every validation failure reported as one `RecipeError` in the wording of `describe_error`."""

    model_config = ConfigDict(extra="forbid", frozen=True)

    @classmethod
    def parse(cls, values: Mapping[str, object], owner: str = "") -> Self:
        """The model for `values` (field name -> value); RecipeError naming `owner` (e.g. "effect 'bloom'"), each bad
        field, the constraint and the value, and listing the fields when an unknown one was given."""
        try:
            return super().model_validate(dict(values))
        except ValidationError as error:
            raise _invalid(error, owner, cls.model_fields) from None

    @classmethod
    def model_validate(cls, obj: Any, **options: Any) -> Self:
        """pydantic's `model_validate`, its failure a RecipeError as `parse` reports it."""
        try:
            return super().model_validate(obj, **options)
        except ValidationError as error:
            raise _invalid(error, "", cls.model_fields) from None

    @classmethod
    def model_validate_json(cls, json_data: str | bytes | bytearray, **options: Any) -> Self:
        """pydantic's `model_validate_json`, its failure a RecipeError as `parse` reports it."""
        try:
            return super().model_validate_json(json_data, **options)
        except ValidationError as error:
            raise _invalid(error, "", cls.model_fields) from None

    @classmethod
    def model_validate_strings(cls, obj: Any, **options: Any) -> Self:
        """pydantic's `model_validate_strings`, its failure a RecipeError as `parse` reports it."""
        try:
            return super().model_validate_strings(obj, **options)
        except ValidationError as error:
            raise _invalid(error, "", cls.model_fields) from None

    @classmethod
    def model_construct(cls, _fields_set: set[str] | None = None, **values: Any) -> Self:  # type: ignore[override]  # mypy reads pydantic's underscore name as private
        """Refused: pydantic's `model_construct` builds a value without checking it, which nothing may. Make one with
        its constructor or `model_validate`."""
        raise RecipeError(
            f"{cls.__name__}.model_construct() builds a value without checking it: use {cls.__name__}(...) or "
            f"{cls.__name__}.model_validate(...)"
        )

    def model_copy(self, *, update: Mapping[str, Any] | None = None, deep: bool = False) -> Self:
        """A copy with `update` laid over the values, checked as a new value is (pydantic's own copy checks nothing):
        a bad value is a RecipeError. Without `update` the copy is equal (with `deep`, of copied fields)."""
        if not update:
            return super().model_copy(deep=deep)
        return type(self).model_validate({**dict(self), **update})

    def _init_checked(self, owner: str, /, **values: Any) -> None:
        """For a subclass's own (positional) constructor: validate `values` into this instance; RecipeError like
        `parse`."""
        try:
            BaseModel.__init__(self, **values)
        except ValidationError as error:
            raise _invalid(error, owner, type(self).model_fields) from None


class Drive(ValidatedModel):
    """A parameter that varies across the picture: its value at a pixel is `start` where the field `by` is 0 and `end`
    where it is 1, and in between along `levels` evenly spaced values.

    The effect is rendered once at each of the `levels` values (so a driven step costs `levels` times the effect; a
    value no pixel needs is not rendered) and each pixel takes the two renders either side of its field value, mixed
    linearly. A field is a selection of the recipe or a built-in one (`depth`, `light`, `subject`...). Written in a
    recipe file as the table `radius = { by = "near", start = 0, end = 30 }` and in Python as
    `Drive(by="near", start=0, end=30)`; only a parameter its effect declares drivable takes one, and `start` and `end`
    (so every value between) must lie in that parameter's range; a cyclic parameter takes any angle.
    """

    by: str = Field(
        min_length=1,
        description="The name of the field, a selection of the recipe or a built-in one (`depth`, `light`, `subject`, "
        "`background`): the parameter is `start` where it is 0 and `end` where it is 1.",
    )
    start: float = Field(
        strict=True, allow_inf_nan=False, description="The parameter's value where the field is 0, in its own unit."
    )
    end: float = Field(
        strict=True, allow_inf_nan=False, description="The parameter's value where the field is 1, in its own unit."
    )
    levels: int = Field(
        5,
        ge=2,
        le=16,
        strict=True,
        json_schema_extra={"unit": "count"},
        description="How many evenly spaced values between `start` and `end` the effect is rendered at (2 to 16); each "
        "pixel mixes the two nearest. More is smoother and costs more renders; 2 mixes the effect at `start` and at "
        "`end`.",
    )

    def __init__(self, /, **values: Any) -> None:
        """Validate and make the drive from its keys (`by`, `start`, `end`, optionally `levels`); RecipeError naming
        each missing or bad key and listing the keys when one is unknown."""
        self._init_checked("drive", **values)

    def value(self, level: int) -> float:
        """The parameter's value at level `level` (0 is `start`, `levels - 1` is exactly `end`: a parameter at one of
        its bounds must not miss it by a float rounding)."""
        if level == self.levels - 1:
            return self.end
        return self.start + (self.end - self.start) * level / (self.levels - 1)

    def values(self) -> tuple[float, ...]:
        """The parameter's value at every level, `start` first."""
        return tuple(self.value(level) for level in range(self.levels))

    def describe(self) -> str:
        """A short readable form, e.g. `by near 0..30`."""
        return f"by {self.by} {self.start:g}..{self.end:g}"


class FieldDoc(NamedTuple):
    """The documentation of one parameter, in parts (see `EffectParams.fields_doc`)."""

    name: str
    type: str  # in words: "number", "whole number", "colour", "one of 'a', 'b'", "list of colours"...
    unit: str | None
    minimum: float | None
    maximum: float | None
    default: Any
    doc: str
    advanced: bool
    cyclic: bool  # an angle that wraps: `minimum` to `maximum` is one turn and any real value is accepted

    @property
    def range(self) -> str:
        """The documented range in words, e.g. "0 to 500"; empty for a field that is not a number."""
        if self.minimum is None or self.maximum is None:
            return ""
        return f"{_number(self.minimum)} to {_number(self.maximum)}"


class _EffectSchema(GenerateJsonSchema):
    """The JSON schema generator of effect parameters: a field whose default has no JSON form (a `Lut`) shows none."""

    ignored_warning_kinds = {"non-serializable-default"}  # noqa: RUF012  (pydantic reads it, never changes it)


class EffectParams(ValidatedModel):
    """Base class of every effect's parameter model: subclass it and declare each field with `param`.

    A text parameter is taken exactly as given, spaces included (a space can be data, as in a text-art palette).
    """

    model_config = ConfigDict(validate_default=True)

    _core_effect: ClassVar[str | None] = None  # the Rust core effect this model is declared by (`core="x"`), if any

    @classmethod
    def model_json_schema(
        cls,
        by_alias: bool = True,
        ref_template: str = DEFAULT_REF_TEMPLATE,
        schema_generator: type[GenerateJsonSchema] = _EffectSchema,
        mode: JsonSchemaMode = "validation",
        **kwargs: Any,
    ) -> dict[str, Any]:
        """The JSON schema of the parameters (pydantic's), without a default for a field that cannot show one."""
        return super().model_json_schema(by_alias, ref_template, schema_generator, mode, **kwargs)

    def __init_subclass__(cls, core: str | None = None, **kwargs: Any) -> None:
        """Check every field's declaration and turn it into a strict pydantic field.

        This runs before pydantic reads the fields, which is why the field (and, for a list, the type of its items) can
        depend on the annotation here. `core="grade"` declares the model from the Rust `Params` struct of the effect
        called so (see `_declare_from_core`) instead of from fields written in the class body.
        """
        super().__init_subclass__(**kwargs)
        if core is not None:
            _declare_from_core(cls, core)
            cls._core_effect = core
        annotations = cls.__annotations__  # this class's own, in definition order
        for name, annotation in inspect.get_annotations(cls, eval_str=True).items():
            if name.startswith("_"):  # private attributes are not parameters
                continue
            typed, made = _field(cls.__name__, name, annotation, cls.__dict__.get(name, _MISSING))
            annotations[name] = typed
            setattr(cls, name, made)

    @classmethod
    def __pydantic_init_subclass__(cls, **kwargs: Any) -> None:
        """Prove that the defaults themselves validate (a default colour of the wrong form, say), and that every
        `at_most` names another field of the model."""
        super().__pydantic_init_subclass__(**kwargs)
        for name, info in cls.model_fields.items():
            other = _extra(info).get("at_most")
            if other is not None and (other == name or other not in cls.model_fields):
                raise ValueError(
                    f"parameter model {cls.__name__!r}, field `{name}`: at_most names `{other}`, "
                    "which is not another field"
                )
        cls.parse({}, f"the defaults of {cls.__name__!r}")

    @model_validator(mode="after")
    def _numbers_within_their_limits(self) -> Self:
        """A number declared `at_most=other` may not exceed `other`: the rule `Table::check` applies to a Rust
        declaration, refused naming both and their values."""
        for name, info in type(self).model_fields.items():
            other = _extra(info).get("at_most")
            if other is not None and getattr(self, name) > getattr(self, other):
                raise ValueError(f"`{name}` ({getattr(self, name)}) must not exceed `{other}` ({getattr(self, other)})")
        return self

    @classmethod
    def fields_doc(cls) -> tuple[FieldDoc, ...]:
        """Every field's documentation, in declaration order."""
        hints = typing.get_type_hints(cls, include_extras=True)
        docs = []
        for name, info in cls.model_fields.items():
            extra = _extra(info)
            minimum, maximum = bounds(info)
            text = _type_text(hints[name]) + (" (drivable)" if extra.get("drivable") else "")
            text += " (cyclic)" if cyclic_of(info) else ""
            docs.append(
                FieldDoc(
                    name,
                    text,
                    unit_of(info),
                    minimum,
                    maximum,
                    info.default,
                    info.description or "",
                    bool(extra["advanced"]),
                    cyclic_of(info),
                )
            )
        return tuple(docs)

    @classmethod
    def documentation(cls, advanced: bool = True) -> str:
        """A "Parameters" section (numpydoc style) with each basic field's type, unit, range, default and description,
        then an "Advanced parameters" section of the same form if there are advanced fields. With `advanced=False` those
        are left out and one line says how many."""
        sections: dict[bool, list[str]] = {False: [], True: []}
        for doc in cls.fields_doc():
            parts = [doc.type]
            if doc.unit:
                parts.append(doc.unit)
            if doc.range:
                parts.append(doc.range)
            parts.append(f"default {doc.default!r}")
            words = " ".join(doc.doc.split())
            description = textwrap.fill(words, 88, initial_indent="    ", subsequent_indent="    ")
            sections[doc.advanced].append(f"{doc.name} : {', '.join(parts)}\n{description}")
        text = "Parameters\n----------\n" + "\n".join(sections[False])
        if not sections[True]:
            return text
        if advanced:
            return text + "\n\nAdvanced parameters\n-------------------\n" + "\n".join(sections[True])
        return text + f"\n\n(+{len(sections[True])} advanced)"


_MISSING = object()
_PLAIN_KINDS: dict[str, Any] = {
    "float": float,
    "int": int,
    "bool": bool,
    "color": Color,
    "colors": tuple[Color, ...],
    "lut": Lut,
    "picture": Picture,
    "field": FieldName,
    "text": str,
}  # the kinds of `Kind` in crates/core/src/params.rs -> their annotation; a "choice" is a `Literal` of its names


def _annotation(effect: str, entry: dict[str, Any]) -> Any:
    """The annotation of the field `entry` of the Rust declaration of `effect`; ValueError if its kind is one the core
    has and this module has not (a kind added to `Kind` without a line in `_PLAIN_KINDS`). A `nullable` field may also
    be none."""
    kind = entry["kind"]
    if kind == "choice":
        return Literal[tuple(entry["choices"])]
    if kind not in _PLAIN_KINDS:
        raise ValueError(
            f"the Rust core declares {effect}.{entry['name']} of the kind {kind!r}, which EffectParams cannot build a "
            f"field from (it knows {', '.join([*_PLAIN_KINDS, 'choice'])})"
        )
    annotation = _PLAIN_KINDS[kind]
    return annotation | None if entry.get("nullable") else annotation


def _default(entry: dict[str, Any]) -> Any:
    """The default of the field `entry`: a list of colours as a tuple, a lookup table (declared as none) as the identity
    table, else the literal the declaration wrote."""
    match entry["kind"]:
        case "colors":
            return tuple(entry["default"])
        case "lut":
            return Lut.identity()
        case _:
            return entry["default"]


def core_field(effect: str, entry: dict[str, Any]) -> tuple[Any, Any]:
    """The annotation and the `param` spec of the field `entry` of the Rust declaration of `effect` (one of the `fields`
    of `_core.params(effect)`), as `class X(EffectParams, core="x")` gives them to the model. A default and a bound are
    the literal the declaration wrote; a list of colours is a tuple, with the `min_length` (and `max_length`) the
    declaration gives, and a text its `min_distinct` too. ValueError if its kind is one the core has and this module
    has not."""
    annotation = _annotation(effect, entry)
    # written for a list of colours or a text only
    lengths = {key: entry[key] for key in ("min_length", "max_length", "min_distinct") if key in entry}
    spec = param(
        _default(entry),
        doc=entry["meaning"],
        unit=entry["unit"],
        min=entry["min"],
        max=entry["max"],
        advanced=entry["advanced"],
        drivable=entry["drivable"],
        cyclic=entry["cyclic"],
        at_most=entry.get("at_most"),  # written for a limited number only
        **lengths,
    )
    return annotation, spec


def _declare_from_core(cls: type[EffectParams], effect: str) -> None:
    """Give `cls` the docstring and the fields (annotations and `param` specs) that the Rust core's `Params` struct of
    `effect` declares, before pydantic reads them; the checks of `_field` then run on them like on any model. Every
    default and bound is the literal the declaration wrote, so a whole number stays an `int` and a decimal a `float`.
    ValueError if the class has fields of its own, or if the core implements no such effect (most often an extension
    built before the effect's kernel was written: the error says to rebuild)."""
    own = [name for name in inspect.get_annotations(cls) if not name.startswith("_")]
    own += [name for name, value in cls.__dict__.items() if isinstance(value, _Spec) and name not in own]
    if own:
        raise ValueError(
            f"parameter model {cls.__name__!r} is declared with core={effect!r} and also has fields of its own "
            f"({', '.join(own)}): the Rust declaration is the only one"
        )
    if effect not in native.effects(declared=True):
        raise ValueError(
            f"the Rust core has no effect called {effect!r}: if its kernel is new, rebuild the extension (`uv sync`)"
        )
    declared = native.params(effect)
    cls.__doc__ = declared["doc"]
    fields = {entry["name"]: core_field(effect, entry) for entry in declared["fields"]}
    cls.__annotations__ = {name: annotation for name, (annotation, _) in fields.items()}
    for name, (_, spec) in fields.items():
        setattr(cls, name, spec)


_TURN = 360  # constant: a full turn, in degrees


def _into_turn(low: float) -> Callable[[Any], Any]:
    """A validator that moves a real number outside `low` to `low + 360` into that turn by whole turns and passes
    anything else (a bool, text, nan, infinity) on for the field's own strict check to refuse."""

    def wrap(value: Any) -> Any:
        if not is_number(value) or not math.isfinite(value):
            return value
        if low <= value <= low + _TURN:
            return value
        return low + (value - low) % _TURN

    return wrap


def _extra(info: Any) -> dict[str, Any]:
    """What `param` put in a field's JSON-schema extras (unit, advanced, drivable, cyclic)."""
    return info.json_schema_extra if isinstance(info.json_schema_extra, dict) else {}


def core_effect(model: type[EffectParams]) -> str | None:
    """The Rust core effect the model is declared by (`core="x"`), if any."""
    return model._core_effect  # noqa: SLF001  (the model's own, read here beside its definition)


def unit_of(info: FieldInfo) -> str | None:
    """The unit the field `info` (a pydantic `FieldInfo`) was declared with, by `param` or by a `json_schema_extra` of
    `{"unit": ...}`; None when it has none."""
    unit = _extra(info).get("unit")
    return unit if isinstance(unit, str) else None


def cyclic_of(info: FieldInfo) -> bool:
    """Whether the field `info` (a pydantic `FieldInfo`) was declared `cyclic=True` by `param`."""
    return bool(_extra(info).get("cyclic"))


def whole_number_fields(model: type[EffectParams]) -> frozenset[str]:
    """The fields of `model` that hold a whole number (`int`): a range of values for one is rounded to whole numbers."""
    hints = typing.get_type_hints(model)
    return frozenset(name for name in model.model_fields if _scalar(hints[name]) == {int})


def picture_fields(model: type[EffectParams]) -> frozenset[str]:
    """The fields of `model` that hold a `Picture` (or none): a recipe file writes a path for one, `fx` takes a path or
    an array."""
    hints = typing.get_type_hints(model)
    return frozenset(name for name in model.model_fields if Picture in _scalar(hints[name]))


def field_name_fields(model: type[EffectParams]) -> tuple[str, ...]:
    """The fields of `model` that name a field (`FieldName`), in declaration order: the effect receives the array of
    each."""
    hints = typing.get_type_hints(model, include_extras=True)
    return tuple(name for name in model.model_fields if hints[name] == FieldName)


def lut_fields(model: type[EffectParams]) -> frozenset[str]:
    """The fields of `model` that hold a `Lut`: a recipe file writes a path to a `.cube` file, `fx` a path or an
    array."""
    hints = typing.get_type_hints(model)
    return frozenset(name for name in model.model_fields if hints[name] is Lut)


def arrays_as_values(model: type[EffectParams], values: Mapping[str, Any]) -> dict[str, Any]:
    """`values` (parameters of `model`) with each array given for a picture parameter (3-d) or a lookup table parameter
    turned into the `Picture` or `Lut` it holds; everything else passes unchanged (a path stays a path: reading files
    is the loader's, `fx`'s job). The one place an array becomes a value, for `fx` and `Step.replace` alike."""
    made = dict(values)
    for key in picture_fields(model) & values.keys():
        if isinstance(values[key], np.ndarray) and values[key].ndim == 3:  # anything else is `Picture`'s to refuse
            made[key] = Picture(values[key])
    for key in lut_fields(model) & values.keys():
        if isinstance(values[key], np.ndarray):
            made[key] = Lut(values[key])
    return made


def drivable_fields(model: type[EffectParams]) -> frozenset[str]:
    """The fields of `model` a `Drive` may vary across the picture (declared with `param(..., drivable=True)`)."""
    return frozenset(name for name, info in model.model_fields.items() if _extra(info).get("drivable"))


def signature(model: type[EffectParams]) -> inspect.Signature:
    """The fields of `model` as a call signature, in declaration order: each a keyword-only parameter with its default
    and the type a caller writes (a `Color` as `str | list[float] | tuple[float, float, float]`; a drivable field as
    `float | Drive`)."""
    hints = typing.get_type_hints(model, include_extras=True)
    drivable = drivable_fields(model)
    return inspect.Signature(
        [
            inspect.Parameter(
                name,
                inspect.Parameter.KEYWORD_ONLY,
                default=info.default,
                annotation=_public(hints[name]) | Drive if name in drivable else _public(hints[name]),
            )
            for name, info in model.model_fields.items()
        ]
    )


def scaled[P: EffectParams](params: P, factor: float) -> P:
    """A copy of `params` with every pixel value (a field with the unit `px`) multiplied by `factor` and every
    `density` (marks per 1000 design px²) divided by `factor` squared: the same look at another size. A whole number is
    rounded. When shrinking (`factor` below 1) a pixel value stops at the field's `min`, which keeps a small picture
    valid (a cell of at least 3 pixels); growing has no ceiling, as a range is in design pixels and the copy is not
    checked against it. Other units are unchanged."""
    whole = whole_number_fields(type(params))
    changes = {}
    for name, info in type(params).model_fields.items():
        unit = unit_of(info)
        if unit == "density":
            # marks per area: an area grows with the square of a length
            changes[name] = getattr(params, name) / factor**2
        elif unit == "px":
            value = getattr(params, name) * factor
            value = round(value) if name in whole else value
            changes[name] = max(bounds(info)[0], value) if factor < 1 else value
    # pydantic's own copy, which checks nothing: a pixel value grown past its design range is meant (see above)
    return BaseModel.model_copy(params, update=changes)


def _range_problem(spec: _Spec) -> str | None:
    """What is wrong with the unit, range, default and cyclic flag of a number's declaration, or None if nothing."""
    if spec.unit not in UNITS:
        return f"a number needs a unit from {UNITS}, got {spec.unit!r}"
    if spec.min is None or spec.max is None:
        return "a number needs both min and max (its documented range)"
    if spec.min > spec.max:
        return f"min {spec.min} is above max {spec.max}"
    if spec.default is not None and not (is_number(spec.default) and spec.min <= spec.default <= spec.max):
        return f"the default {spec.default!r} is outside min {spec.min} .. max {spec.max}"
    if spec.cyclic and (spec.unit != "degrees" or spec.max - spec.min != _TURN):
        return f"a cyclic field is in degrees, min to max one turn ({_TURN}), not {spec.min} to {spec.max}"
    return None


def _field(model: str, name: str, annotation: Any, spec: Any) -> tuple[Any, Any]:
    """The annotation and pydantic field for a `param` declaration, after checking it; ValueError naming the model and
    the field if it is broken. Every field is strict; a list field also takes a list or an array."""

    def broken(problem: str) -> ValueError:
        return ValueError(f"parameter model {model!r}, field `{name}`: {problem}")

    if spec is _MISSING:
        raise broken("has no default: declare it with param(default, doc=..., ...)")
    if not isinstance(spec, _Spec):
        raise broken("declare it with param(default, doc=..., unit=..., min=..., max=...), not a plain default")
    if not spec.doc.strip():
        raise broken("needs a doc: what it does, its unit and range, and what raising it does")
    for choice in _choices(annotation):
        if str(choice) not in spec.doc:
            raise broken(f"the doc must explain every choice; it does not mention {choice!r}")
    numeric = _scalar(annotation) <= {int, float} and bool(_scalar(annotation))
    # strict for every kind of value; a choice (Literal) is matched exactly anyway, and pydantic refuses `strict` on it
    constraints: dict[str, Any] = {} if _choices(annotation) else {"strict": True}
    if numeric:
        if problem := _range_problem(spec):
            raise broken(problem)
    elif spec.cyclic:
        raise broken("only a number in degrees can be cyclic")
    elif spec.unit is not None or spec.min is not None or spec.max is not None:
        raise broken("only a number takes a unit, min and max")
    if spec.drivable and _scalar(annotation) != {float}:
        what = "a whole number" if _scalar(annotation) == {int} else "not a number"
        raise broken(f"only a decimal number can be drivable, and this is {what}")
    typed: Any = annotation
    if numeric:
        constraints |= {"ge": spec.min, "le": spec.max}
        if float in _scalar(annotation):
            constraints["allow_inf_nan"] = False  # so a nan is reported as such, not as out of range
        if spec.cyclic:
            typed = Annotated[annotation, BeforeValidator(_into_turn(constraints["ge"]))]
    elif get_origin(annotation) is tuple:
        typed = Annotated[annotation, BeforeValidator(_as_tuple)]
    elif Picture in _scalar(annotation):
        typed = ValidatedPicture | None if type(None) in _members(annotation) else ValidatedPicture
        constraints = {}  # the validator is the whole check; `strict` has no schema to apply to in a union
    elif annotation is Lut:
        typed = ValidatedLut
    typed, kwargs = _with_distinct(typed, spec.field_kwargs)
    made = Field(
        spec.default,
        description=spec.doc,
        json_schema_extra={
            "unit": spec.unit,
            "advanced": spec.advanced,
            "drivable": spec.drivable,
            "cyclic": spec.cyclic,
            "at_most": spec.at_most,
        },
        **constraints,
        **kwargs,
    )
    return typed, made


def _with_distinct(typed: Any, kwargs: Mapping[str, Any]) -> tuple[Any, dict[str, Any]]:
    """`typed` and the pydantic constraints `kwargs` without `min_distinct`, which is not one: a text's own rule, made a
    validator on `typed`."""
    rest = {key: value for key, value in kwargs.items() if key != "min_distinct"}
    if kwargs.get("min_distinct") is None:
        return typed, rest
    return Annotated[typed, AfterValidator(_at_least_distinct(kwargs["min_distinct"]))], rest


def _at_least_distinct(least: int) -> Callable[[str], str]:
    """A validator that passes a text with at least `least` different characters and refuses any other."""

    def check(text: str) -> str:
        if len(set(text)) < least:
            raise ValueError(f"must have at least {least} different characters")
        return text

    return check


def _members(annotation: Any) -> tuple[Any, ...]:
    """The types of a union (`X | None` -> X, None), or the annotation alone."""
    if get_origin(annotation) in (typing.Union, types.UnionType):
        return get_args(annotation)
    return (annotation,)


def _scalar(annotation: Any) -> set[Any]:
    """The types of a field without None: {int} for `int`, {float} for `float | None`, {bool} for `bool`..."""
    return {m for m in _members(annotation) if m is not type(None)}


def bounds(info: FieldInfo) -> tuple[float | None, float | None]:
    """The (min, max) the constraints of the field `info` (a pydantic `FieldInfo`) give, None for a side it has not."""
    low = next((m.ge for m in info.metadata if isinstance(m, annotated_types.Ge)), None)
    high = next((m.le for m in info.metadata if isinstance(m, annotated_types.Le)), None)
    return low, high  # type: ignore[return-value]  # bounds given to `param` as numbers


def _choices(annotation: Any) -> tuple[Any, ...]:
    """The choices of a `Literal[...]` field (also inside `X | None`), else nothing."""
    return tuple(c for m in _members(annotation) if get_origin(m) is Literal for c in get_args(m))


def _public(annotation: Any) -> Any:
    """The type a caller writes for a field: `Color` becomes the colour forms; other metadata is dropped."""
    if annotation == Color:
        return ColorForms
    if annotation is Picture:
        return PictureForms
    if annotation is Lut:
        return LutForms
    origin, args = get_origin(annotation), get_args(annotation)
    if origin is Annotated:
        return _public(args[0])
    if origin is tuple and Ellipsis in args:  # a list field: callers may pass any sequence (TOML gives lists)
        return Sequence[_public(args[0])]  # type: ignore[misc]  # built at run time
    if origin is tuple:
        return tuple[tuple(_public(arg) for arg in args)]  # type: ignore[misc]  # built at run time
    if origin in (typing.Union, types.UnionType):
        public = [_public(arg) for arg in args]
        union = public[0]
        for member in public[1:]:
            union = union | member
        return union
    return annotation


def _type_text(annotation: Any) -> str:
    """A field's type in words, for the documentation."""
    if annotation == Color:
        return "colour"
    if annotation is Picture:
        return "picture"
    if annotation == FieldName:
        return "field name"
    if annotation is Lut:
        return "lookup table"
    origin, args = get_origin(annotation), get_args(annotation)
    if origin is Annotated:
        return _type_text(args[0])
    if origin is Literal:
        return "one of " + ", ".join(repr(arg) for arg in args)
    if origin is tuple:
        count = "list of" if Ellipsis in args else f"{len(args)}"
        return f"{count} {_type_text(args[0])}s"
    if origin in (typing.Union, types.UnionType):
        return " or ".join("none" if arg is type(None) else _type_text(arg) for arg in args)
    words = {float: "number", int: "whole number", str: "text", bool: "true or false"}
    return words.get(annotation, inspect.formatannotation(annotation))


def _number(value: float) -> str:
    """A bound as a person writes it: 0, 500, 0.5, 4294967295."""
    return str(int(value)) if float(value).is_integer() and abs(value) < 1e15 else repr(value)


def _nearest(given: object, valid: list[str]) -> str:
    """ " (did you mean `x`?)" for the name `x` in `valid` nearest the misspelt `given`; "" when none is near."""
    near = difflib.get_close_matches(str(given), valid, n=1)
    return f" (did you mean `{near[0]}`?)" if near else ""


def _problem(detail: Any, valid: list[str]) -> str:
    location = detail["loc"]
    if not location:  # a rule about the whole model, not one field
        return str(detail.get("ctx", {}).get("error") or detail["msg"])
    name = "`" + "".join(f"[{part}]" if isinstance(part, int) else f".{part}" for part in location).lstrip(".") + "`"
    kind, ctx, given = detail["type"], detail.get("ctx", {}), detail.get("input")
    got = f", got {given!r}"
    wording = {
        "extra_forbidden": f"{name} is not a parameter{_nearest(location[-1], valid)}; the parameters are: "
        f"{', '.join(valid)}",
        "missing": f"{name} is required",
        "greater_than_equal": f"{name} must be >= {_number(ctx.get('ge', 0))}{got}",
        "less_than_equal": f"{name} must be <= {_number(ctx.get('le', 0))}{got}",
        "greater_than": f"{name} must be > {_number(ctx.get('gt', 0))}{got}",
        "less_than": f"{name} must be < {_number(ctx.get('lt', 0))}{got}",
        "float_type": f"{name} must be a number{got}",
        "int_type": f"{name} must be a whole number{got}",
        "bool_type": f"{name} must be true or false{got}",
        "string_type": f"{name} must be text{got}",
        "tuple_type": f"{name} must be a list{got}",
        "finite_number": f"{name} must be a finite number{got}",
        "literal_error": f"{name} must be one of {ctx.get('expected')}{got}",
        "too_short": f"{name} needs at least {ctx.get('min_length')} items{got}",
        "string_pattern_mismatch": f"{name} must match the pattern {ctx.get('pattern')}{got}",
        "value_error": f"{name}: {ctx.get('error')}",
    }
    return wording.get(kind, f"{name}: {detail['msg']}{got}")
