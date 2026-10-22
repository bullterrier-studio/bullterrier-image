"""How an effect declares its parameters: a frozen Pydantic model whose every field is described, ranged and defaulted.

    class GlowParams(EffectParams):
        \"\"\"Bloom around the bright parts of the picture.\"\"\"

        radius: float = param(24.0, unit="px", min=0, max=500, doc="Gaussian sigma of the halo. Larger = wider.")
        tint: Color = param("#ffffff", doc="Colour of the halo.")

Promises:
- A model is checked when its class is made (that is, when the effect module is imported): every field is declared with
  `param`, has a non-empty description, and a numeric field has a unit from `UNITS` and a range that holds its default.
  A broken declaration raises `ValueError` naming the model and the field, so it never reaches a recipe.
- Values are checked when a model is made: unknown fields, values of the wrong kind (a bool or a numeric-looking string
  for a number) and values out of range are rejected; a whole number is fine for a decimal field. Models are frozen.
- `describe_error` turns a validation failure into one sentence naming the owner, each field, the constraint and value.
- `EffectParams.signature`, `fields_doc` and `documentation` describe a model for `fx`, `help()`, the catalogue and the
  command line, in a caller's types (a `Color` is shown as the forms one may write).
"""

import inspect
import math
import re
import textwrap
import types
import typing
from collections.abc import Iterable
from dataclasses import dataclass, field
from typing import Annotated, Any, Literal, NamedTuple, get_args, get_origin

import annotated_types
from pydantic import BaseModel, BeforeValidator, ConfigDict, Field, ValidationError, WithJsonSchema

from photo_fx.ops.color import parse_color

from .types import Color as ColorForms

UNITS = ("px", "fraction", "degrees", "stops", "seconds", "count", "ratio", "cycles")

_HEX = re.compile(r"#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6})")


def _to_color(value: object) -> tuple[float, float, float]:
    """A colour as written ('#rrggbb', '#rgb', three numbers 0..1) -> the three floats `parse_color` gives."""
    if isinstance(value, str) and _HEX.fullmatch(value.strip()):
        red, green, blue = (float(c) for c in parse_color(value.strip()))
        return red, green, blue
    if isinstance(value, list | tuple) and len(value) == 3:
        parts = list(value)
        if all(
            isinstance(c, int | float) and not isinstance(c, bool) and math.isfinite(c) and 0 <= c <= 1 for c in parts
        ):
            red, green, blue = (float(c) for c in parse_color(parts))
            return red, green, blue
    raise ValueError(f"a colour is '#rrggbb', '#rgb' or three numbers from 0 to 1, got {value!r}")


Color = Annotated[
    tuple[float, float, float],
    BeforeValidator(_to_color),
    WithJsonSchema(
        {
            "anyOf": [
                {"type": "string", "pattern": _HEX.pattern},
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
blue), exactly the float32 values `photo_fx.ops.color.parse_color` gives, so `np.asarray(color, np.float32)` is it."""


@dataclass(frozen=True)
class _Spec:
    """What `param` was told, until the model class turns it into a pydantic field (see `EffectParams`)."""

    default: Any
    doc: str
    unit: str | None
    min: float | None
    max: float | None
    field_kwargs: dict[str, Any] = field(default_factory=dict)


def param(
    default: Any,
    *,
    doc: str,
    unit: str | None = None,
    min: float | None = None,  # noqa: A002  (the documented name of the range's lower bound)
    max: float | None = None,  # noqa: A002  (the documented name of the range's upper bound)
    **field_kwargs: Any,
) -> Any:
    """Declare one field of an `EffectParams` model.

    default: the value an effect uses when none is given; it must lie in the range. doc: what the field does, its unit
    and range in words, and what raising it does (shown in `help()`, the catalogue and the command line). unit: one of
    `UNITS`, required for a number, not allowed otherwise. min, max: the documented range of a number, both required
    and inclusive; not allowed for anything else. field_kwargs: further pydantic `Field` constraints, e.g.
    `min_length=2` for a list.

    The rules are checked when the model class is made; a violation raises `ValueError` naming the model and the field.
    """
    return _Spec(default, doc, unit, min, max, field_kwargs)


class FieldDoc(NamedTuple):
    """The documentation of one parameter, in parts (see `EffectParams.fields_doc`)."""

    name: str
    type: str  # in words: "number", "whole number", "colour", "one of 'a', 'b'", "list of colours"...
    unit: str | None
    minimum: float | None
    maximum: float | None
    default: Any
    doc: str


class EffectParams(BaseModel):
    """Base class of every effect's parameter model: subclass it and declare each field with `param`."""

    model_config = ConfigDict(extra="forbid", frozen=True, validate_default=True, str_strip_whitespace=True)

    def __init_subclass__(cls, **kwargs: Any) -> None:
        """Check every field's declaration and turn it into a pydantic field (strict for numbers).

        This runs before pydantic reads the fields, which is why strictness can depend on the annotation here.
        """
        super().__init_subclass__(**kwargs)
        for name, annotation in inspect.get_annotations(cls, eval_str=True).items():
            if name.startswith("_"):  # private attributes are not parameters
                continue
            spec = cls.__dict__.get(name, _MISSING)
            setattr(cls, name, _field(cls.__name__, name, annotation, spec))

    @classmethod
    def __pydantic_init_subclass__(cls, **kwargs: Any) -> None:
        """Prove that the defaults themselves validate (a default colour of the wrong form, say)."""
        super().__pydantic_init_subclass__(**kwargs)
        try:
            cls()
        except ValidationError as error:
            raise ValueError(describe_error(error, f"the defaults of {cls.__name__!r}", cls.model_fields)) from None

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
            minimum = next((m.ge for m in info.metadata if isinstance(m, annotated_types.Ge)), None)
            maximum = next((m.le for m in info.metadata if isinstance(m, annotated_types.Le)), None)
            docs.append(
                FieldDoc(
                    name,
                    _type_text(hints[name]),
                    unit if isinstance(unit, str) else None,
                    minimum,  # type: ignore[arg-type]  # a bound given to `param` as a number
                    maximum,  # type: ignore[arg-type]  # a bound given to `param` as a number
                    info.default,
                    info.description or "",
                )
            )
        return tuple(docs)

    @classmethod
    def documentation(cls) -> str:
        """A "Parameters" section (numpydoc style) with each field's type, unit, range, default and description."""
        entries = []
        for doc in cls.fields_doc():
            parts = [doc.type]
            if doc.unit:
                parts.append(doc.unit)
            if doc.minimum is not None and doc.maximum is not None:
                parts.append(f"{_number(doc.minimum)} to {_number(doc.maximum)}")
            parts.append(f"default {doc.default!r}")
            description = textwrap.fill(" ".join(doc.doc.split()), 88, initial_indent="    ", subsequent_indent="    ")
            entries.append(f"{doc.name} : {', '.join(parts)}\n{description}")
        return "Parameters\n----------\n" + "\n".join(entries)


_MISSING = object()


def _field(model: str, name: str, annotation: Any, spec: Any) -> Any:
    """The pydantic field for a `param` declaration, after checking it; ValueError naming model and field if broken."""

    def broken(problem: str) -> ValueError:
        return ValueError(f"parameter model {model!r}, field `{name}`: {problem}")

    if spec is _MISSING:
        raise broken("has no default: declare it with param(default, doc=..., ...)")
    if not isinstance(spec, _Spec):
        raise broken("declare it with param(default, doc=..., unit=..., min=..., max=...), not a plain default")
    if not spec.doc.strip():
        raise broken("needs a doc: what it does, its unit and range, and what raising it does")
    numeric = _numeric(annotation)
    constraints: dict[str, Any] = {}
    if numeric:
        if spec.unit not in UNITS:
            raise broken(f"a number needs a unit from {UNITS}, got {spec.unit!r}")
        if spec.min is None or spec.max is None:
            raise broken("a number needs both min and max (its documented range)")
        if spec.min > spec.max:
            raise broken(f"min {spec.min} is above max {spec.max}")
        default_is_number = isinstance(spec.default, int | float) and not isinstance(spec.default, bool)
        if spec.default is not None and not (default_is_number and spec.min <= spec.default <= spec.max):
            raise broken(f"the default {spec.default!r} is outside min {spec.min} .. max {spec.max}")
        constraints = {"ge": spec.min, "le": spec.max, "strict": True}  # the range also rules out nan and inf
    elif spec.unit is not None or spec.min is not None or spec.max is not None:
        raise broken("only a number takes a unit, min and max")
    for choice in _choices(annotation):
        if str(choice) not in spec.doc:
            raise broken(f"the doc must explain every choice; it does not mention {choice!r}")
    return Field(
        spec.default, description=spec.doc, json_schema_extra={"unit": spec.unit}, **constraints, **spec.field_kwargs
    )


def _members(annotation: Any) -> tuple[Any, ...]:
    """The types of a union (`X | None` -> X, None), or the annotation alone."""
    if get_origin(annotation) in (typing.Union, types.UnionType):
        return get_args(annotation)
    return (annotation,)


def _numeric(annotation: Any) -> bool:
    """An int or float field, possibly optional (bool is not a number here)."""
    members = [m for m in _members(annotation) if m is not type(None)]
    return bool(members) and all(m in (int, float) for m in members)


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
    if origin is tuple:
        return tuple[tuple(arg if arg is Ellipsis else _public(arg) for arg in args)]  # type: ignore[misc]  # built at run time
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
    if origin is Literal:
        return "one of " + ", ".join(repr(arg) for arg in args)
    if origin is tuple:
        return f"list of {_type_text(args[0])}s"
    if origin in (typing.Union, types.UnionType):
        return " or ".join("none" if arg is type(None) else _type_text(arg) for arg in args)
    if origin is Annotated:
        return _type_text(args[0])
    words = {float: "number", int: "whole number", str: "text", bool: "true or false"}
    return words.get(annotation, inspect.formatannotation(annotation))


def _number(value: float) -> str:
    """A bound as a person writes it: 0, 500, 0.5, 4294967295."""
    return str(int(value)) if float(value).is_integer() and abs(value) < 1e15 else repr(value)


def describe_error(error: ValidationError, owner: str, valid: Iterable[str]) -> str:
    """One sentence for a failed validation: `owner` (e.g. "effect 'glow'"), then each problem with its field, the
    constraint and the value given. `valid` names the fields, listed when an unknown one was given."""
    problems = [_problem(detail, sorted(valid)) for detail in error.errors()]
    return f"{owner}: " + "; ".join(problems)


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
        "finite_number": f"{name} must be a finite number{got}",
        "literal_error": f"{name} must be one of {ctx.get('expected')}{got}",
        "too_short": f"{name} needs at least {ctx.get('min_length')} items{got}",
        "value_error": f"{name}: {ctx.get('error')}",
    }
    return wording.get(kind, f"{name}: {detail['msg']}{got}")
