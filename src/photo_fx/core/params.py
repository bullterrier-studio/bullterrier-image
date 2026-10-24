"""How an effect declares its parameters: a frozen Pydantic model whose every field is described, ranged and defaulted.

    class GlowParams(EffectParams):
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
- `param(..., advanced=True)` changes only how a field is listed (after the basic ones, or as a count); it is validated,
  defaulted, in the signature and in the JSON schema (`"advanced"`) like any other.
"""

import inspect
import math
import re
import textwrap
import types
import typing
from collections.abc import Iterable, Mapping, Sequence
from dataclasses import dataclass, field
from typing import Annotated, Any, Literal, NamedTuple, Self, get_args, get_origin

import annotated_types
import numpy as np
from PIL import ImageColor
from pydantic import BaseModel, BeforeValidator, ConfigDict, Field, ValidationError, WithJsonSchema

from photo_fx.errors import RecipeError

from .types import Color as ColorForms

UNITS = ("px", "fraction", "degrees", "stops", "count", "ratio", "cycles")

_HEX = re.compile(r"#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6})")


def _is_number(value: object) -> bool:
    """A real number as Python or numpy writes it; a bool is not one here."""
    return isinstance(value, int | float | np.integer | np.floating) and not isinstance(value, bool | np.bool_)


def _to_color(value: object) -> tuple[float, float, float]:
    """A colour as written ('#rrggbb', '#rgb', three numbers 0..1, a numpy array of three) -> its three floats 0..1."""
    if isinstance(value, str) and _HEX.fullmatch(value.strip()):
        red, green, blue = (channel / 255.0 for channel in ImageColor.getrgb(value.strip()))  # constant: 8-bit levels
        return red, green, blue
    parts: list[Any] = []
    if isinstance(value, np.ndarray) and value.ndim == 1:
        parts = value.tolist()
    elif isinstance(value, list | tuple):
        parts = list(value)
    if len(parts) == 3 and all(_is_number(c) and math.isfinite(c) and 0 <= c <= 1 for c in parts):
        return float(parts[0]), float(parts[1]), float(parts[2])
    raise ValueError(f"a colour is '#rrggbb', '#rgb' or three numbers from 0 to 1, got {value!r}")


Color = Annotated[
    tuple[float, float, float],
    BeforeValidator(_to_color),
    WithJsonSchema(
        {
            "anyOf": [
                {"type": "string", "pattern": f"^{_HEX.pattern}$"},
                {
                    "type": "array",
                    "items": {"type": "number", "minimum": 0, "maximum": 1},
                    "minItems": 3,
                    "maxItems": 3,
                },
            ]
        }
    ),
]
"""A colour parameter: written '#rrggbb', '#rgb' or as three numbers 0..1; held as three floats 0..1 (red, green,
blue): `np.asarray(color, np.float32)` is the colour as an array."""


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
    field_kwargs: dict[str, Any] = field(default_factory=dict)


def param(
    default: Any,
    *,
    doc: str,
    unit: str | None = None,
    min: float | None = None,  # noqa: A002  (the documented name of the range's lower bound)
    max: float | None = None,  # noqa: A002  (the documented name of the range's upper bound)
    advanced: bool = False,
    **field_kwargs: Any,
) -> Any:
    """Declare one field of an `EffectParams` model.

    default: the value an effect uses when none is given; it must lie in the range. doc: what the field does, its unit
    and range in words, and what raising it does (shown in `help()`, the catalogue and the command line). unit: one of
    `UNITS`, required for a number, not allowed otherwise. min, max: the documented range, both required and inclusive
    for a number; not allowed for anything else. advanced: the user would change it to tune the technique rather
    than to choose the look; it is listed after the basic fields (and left out of short listings) but is settable,
    validated and in the schema like any other.
    field_kwargs: further pydantic `Field` constraints, e.g. `min_length=2` for a list.

    The rules are checked when the model class is made; a violation raises `ValueError` naming the model and the field.
    """
    return _Spec(default, doc, unit, min, max, advanced, field_kwargs)


def describe_error(error: ValidationError, owner: str, valid: Iterable[str]) -> str:
    """One sentence for a failed validation: `owner` (e.g. "effect 'glow'"; "" for none), then each problem with its
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
    """Base of photo-fx's validated values (effect parameters, recipe steps, recipes): frozen, unknown fields refused,
    and every validation failure reported as one `RecipeError` in the wording of `describe_error`."""

    model_config = ConfigDict(extra="forbid", frozen=True)

    @classmethod
    def parse(cls, values: Mapping[str, object], owner: str = "") -> Self:
        """The model for `values` (field name -> value); RecipeError naming `owner` (e.g. "effect 'glow'"), each bad
        field, the constraint and the value, and listing the fields when an unknown one was given."""
        try:
            return cls.model_validate(dict(values))
        except ValidationError as error:
            raise _invalid(error, owner, cls.model_fields) from None

    def _init_checked(self, owner: str, **values: Any) -> None:
        """For a subclass's own (positional) constructor: validate `values` into this instance; RecipeError like
        `parse`."""
        try:
            BaseModel.__init__(self, **values)
        except ValidationError as error:
            raise _invalid(error, owner, type(self).model_fields) from None


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

    @property
    def range(self) -> str:
        """The documented range in words, e.g. "0 to 500"; empty for a field that is not a number."""
        if self.minimum is None or self.maximum is None:
            return ""
        return f"{_number(self.minimum)} to {_number(self.maximum)}"


class EffectParams(ValidatedModel):
    """Base class of every effect's parameter model: subclass it and declare each field with `param`.

    A text parameter is taken exactly as given, spaces included (a space can be data, as in a text-art palette).
    """

    model_config = ConfigDict(validate_default=True)

    def __init_subclass__(cls, **kwargs: Any) -> None:
        """Check every field's declaration and turn it into a strict pydantic field.

        This runs before pydantic reads the fields, which is why the field (and, for a list, the type of its items) can
        depend on the annotation here.
        """
        super().__init_subclass__(**kwargs)
        annotations = cls.__annotations__  # this class's own, in definition order
        for name, annotation in inspect.get_annotations(cls, eval_str=True).items():
            if name.startswith("_"):  # private attributes are not parameters
                continue
            typed, made = _field(cls.__name__, name, annotation, cls.__dict__.get(name, _MISSING))
            annotations[name] = typed
            setattr(cls, name, made)

    @classmethod
    def __pydantic_init_subclass__(cls, **kwargs: Any) -> None:
        """Prove that the defaults themselves validate (a default colour of the wrong form, say)."""
        super().__pydantic_init_subclass__(**kwargs)
        cls.parse({}, f"the defaults of {cls.__name__!r}")

    @classmethod
    def whole_number_fields(cls) -> frozenset[str]:
        """The fields that hold a whole number (`int`): a range of values for one is rounded to whole numbers."""
        return _whole_number_fields(cls)

    @classmethod
    def signature(cls) -> inspect.Signature:
        """The fields as a call signature, in declaration order: each a positional-or-keyword parameter with its
        default and the type a caller writes (a `Color` as `str | list[float] | tuple[float, float, float]`)."""
        hints = typing.get_type_hints(cls, include_extras=True)
        return inspect.Signature(
            [
                inspect.Parameter(
                    name, inspect.Parameter.POSITIONAL_OR_KEYWORD, default=info.default, annotation=_public(hints[name])
                )
                for name, info in cls.model_fields.items()
            ]
        )

    @classmethod
    def fields_doc(cls) -> tuple[FieldDoc, ...]:
        """Every field's documentation, in declaration order."""
        hints = typing.get_type_hints(cls, include_extras=True)
        docs = []
        for name, info in cls.model_fields.items():
            extra = info.json_schema_extra if isinstance(info.json_schema_extra, dict) else {}
            unit = extra.get("unit")
            minimum, maximum = _bounds(info.metadata)
            text = _type_text(hints[name])
            docs.append(
                FieldDoc(
                    name,
                    text,
                    unit if isinstance(unit, str) else None,
                    minimum,
                    maximum,
                    info.default,
                    info.description or "",
                    bool(extra["advanced"]),
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


def _whole_number_fields(model: type[EffectParams]) -> frozenset[str]:
    hints = typing.get_type_hints(model)
    return frozenset(name for name in model.model_fields if _scalar(hints[name]) == {int})


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
        if spec.unit not in UNITS:
            raise broken(f"a number needs a unit from {UNITS}, got {spec.unit!r}")
        if spec.min is None or spec.max is None:
            raise broken("a number needs both min and max (its documented range)")
        if spec.min > spec.max:
            raise broken(f"min {spec.min} is above max {spec.max}")
        if spec.default is not None and not (_is_number(spec.default) and spec.min <= spec.default <= spec.max):
            raise broken(f"the default {spec.default!r} is outside min {spec.min} .. max {spec.max}")
    elif spec.unit is not None or spec.min is not None or spec.max is not None:
        raise broken("only a number takes a unit, min and max")
    typed: Any = annotation
    if numeric:
        constraints |= {"ge": spec.min, "le": spec.max}
        if float in _scalar(annotation):
            constraints["allow_inf_nan"] = False  # so a nan is reported as such, not as out of range
    elif get_origin(annotation) is tuple:
        typed = Annotated[annotation, BeforeValidator(_as_tuple)]
    made = Field(
        spec.default,
        description=spec.doc,
        json_schema_extra={"unit": spec.unit, "advanced": spec.advanced},
        **constraints,
        **spec.field_kwargs,
    )
    return typed, made


def _members(annotation: Any) -> tuple[Any, ...]:
    """The types of a union (`X | None` -> X, None), or the annotation alone."""
    if get_origin(annotation) in (typing.Union, types.UnionType):
        return get_args(annotation)
    return (annotation,)


def _scalar(annotation: Any) -> set[Any]:
    """The types of a field without None: {int} for `int`, {float} for `float | None`, {bool} for `bool`..."""
    return {m for m in _members(annotation) if m is not type(None)}


def _bounds(metadata: list[Any]) -> tuple[float | None, float | None]:
    """The (min, max) a field's constraints give."""
    low = next((m.ge for m in metadata if isinstance(m, annotated_types.Ge)), None)
    high = next((m.le for m in metadata if isinstance(m, annotated_types.Le)), None)
    return low, high  # type: ignore[return-value]  # bounds given to `param` as numbers


def _choices(annotation: Any) -> tuple[Any, ...]:
    """The choices of a `Literal[...]` field (also inside `X | None`), else nothing."""
    return tuple(c for m in _members(annotation) if get_origin(m) is Literal for c in get_args(m))


def _public(annotation: Any) -> Any:
    """The type a caller writes for a field: `Color` becomes the colour forms; other metadata is dropped."""
    if annotation == Color:
        return ColorForms
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


def _problem(detail: Any, valid: list[str]) -> str:
    location = detail["loc"]
    if not location:  # a rule about the whole model, not one field
        return str(detail.get("ctx", {}).get("error") or detail["msg"])
    name = "`" + "".join(f"[{part}]" if isinstance(part, int) else f".{part}" for part in location).lstrip(".") + "`"
    kind, ctx, given = detail["type"], detail.get("ctx", {}), detail.get("input")
    got = f", got {given!r}"
    wording = {
        "extra_forbidden": f"{name} is not a parameter; the parameters are: {', '.join(valid)}",
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
