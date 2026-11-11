"""How a plugin declares the parameters of an effect: a plain class with every field described, ranged and defaulted.

    class BloomParams(EffectParams):
        \"\"\"Bloom around the bright parts of the picture.\"\"\"

        radius: float = param(24.0, unit="px", min=0, max=500, doc="Gaussian sigma of the halo. Larger = wider.")
        tint: Color = param("#ffffff", doc="Colour of the halo.")

Promises:
- A class is declared when it is made (when the effect module is imported): its fields become the parameter table of
  the ecosystem's format (`table_of`), which the core reads and holds to the format's rules (a number has a unit and a
  range that holds its default, a default must pass its own check...). A broken declaration raises `ValueError` naming
  the class and the field, so it never reaches a recipe. Every field is declared with `param` and has a non-empty
  description. The table is the one description every caller reads: `fx`, the catalogue, the command line and the
  loader see a plugin's effect as they see one the core runs.
- Values are checked by the core when an instance is made: the rules are the format's, for every kind of field (unknown
  names, values of the wrong kind, values out of range are refused, a whole number is fine for a decimal field, a
  list or numpy array for a list field). Instances are frozen, compare and hash by their values.
- A `Picture` field (a second picture an effect uses: a texture, a logo; named `image` when an effect has one) holds a
  `Picture`, read-only RGBA pixels with a content digest, or none (`Picture | None`, default none: the step leaves the
  photo as it is); it is given a `Picture` or a colour (a flat picture). Recipe files and `fx` turn a path or an array
  into a `Picture` before validation (see `loader`).
- A `FieldName` field (a selection an effect works from: `depth`, `light`, a band of the recipe) holds the name of a
  selection, as a step's `mask` and a `Drive`'s `by` do; the recipe checks the name, the runner builds the field and the
  effect receives its array (see `registry`). It is not a number: it has no address and cannot be driven.
- A `Lut` field (a 3D colour lookup table, from a `.cube` file) holds a `Lut`, a read-only table with a content digest;
  it must have a default such as `Lut.identity()`. Recipe files and `fx` turn a path or an array into a `Lut` before
  validation (see `loader`).
- `param(..., advanced=True)` changes only how a field is listed (after the basic ones, or as a count); it is validated
  and defaulted like any other.
"""

import inspect
import types
import typing
from collections.abc import Mapping
from dataclasses import FrozenInstanceError, dataclass
from typing import Any, ClassVar, Literal, Self, get_args, get_origin

import numpy as np

from bullterrier_image.errors import RecipeError

from . import native
from .parameters import default_of
from .values import FieldName, Lut, Picture

__all__ = ["EffectParams", "from_checked", "param", "prepare", "restore", "table_of"]

_COLOUR = tuple[float, float, float]  # a `Color` without its validation: what a hint says of a colour
_MISSING = object()


@dataclass(frozen=True)
class _Spec:
    """What `param` was told, until the class turns it into a table."""

    default: Any
    doc: str
    unit: str | None
    min: float | None
    max: float | None
    advanced: bool
    drivable: bool = False
    cyclic: bool = False
    at_most: str | None = None
    min_length: int | None = None
    max_length: int | None = None
    min_distinct: int | None = None


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
    min_length: int | None = None,
    max_length: int | None = None,
    min_distinct: int | None = None,
) -> Any:
    """Declare one field of an `EffectParams` class.

    default: the value an effect uses when none is given; it must lie in the range. doc: what the field does, its unit
    and range in words, and what raising it does (shown in `help()`, the catalogue and the command line). unit: one of
    the format's units, required for a number, not allowed otherwise. min, max: the documented range, both required and
    inclusive for a number; not allowed for anything else. advanced: the user would change it to tune the technique
    rather than to choose the look; it is listed after the basic fields (and left out of short listings) but is
    settable, validated and in the schema like any other. drivable: a field (see `Drive`) may vary this decimal number
    pixel by pixel; declare it only where the effect's output changes smoothly with the value. Only a float can be.
    cyclic: an angle in degrees whose value and the value a turn (360) further mean the same picture; `min` to `max`
    must be exactly one turn, and a value outside it is stored as the same direction inside it (a value inside, the
    bounds included, is kept). at_most: the name of another number field of the class, in the same unit, that this
    one may not exceed (a lowest height below the highest); checked when a value is made, whichever of the two is
    given. min_length, max_length: the length of a list of colours or a text; min_distinct: the different characters a
    text needs.

    The rules are checked when the class is made; a violation raises `ValueError` naming the class and the field.
    """
    return _Spec(
        default, doc, unit, min, max, advanced, drivable, cyclic, at_most, min_length, max_length, min_distinct
    )


def _members(annotation: Any) -> tuple[Any, ...]:
    """The types of a union (`X | None` -> X, None), or the annotation alone."""
    if get_origin(annotation) in (typing.Union, types.UnionType):
        return get_args(annotation)
    return (annotation,)


def _scalar(annotation: Any) -> set[Any]:
    """The types of a field without None: {int} for `int`, {float} for `float | None`, {bool} for `bool`..."""
    return {m for m in _members(annotation) if m is not type(None)}


def _choices(annotation: Any) -> tuple[Any, ...]:
    """The choices of a `Literal[...]` field (also inside `X | None`), else nothing."""
    return tuple(c for m in _members(annotation) if get_origin(m) is Literal for c in get_args(m))


def _hex(colour: Any) -> str:
    """A colour default (`'#rrggbb'`, `'#rgb'` or three numbers) as the `'#rrggbb'` text the parameter format writes."""
    channels = native.parse_color(colour if isinstance(colour, str) else list(colour))
    if channels is None:
        raise ValueError(f"{colour!r} is not a colour")
    return "#" + "".join(f"{round(channel * 255):02x}" for channel in channels)  # constant: 8-bit levels


def _kind(annotation: Any, default: Any) -> tuple[str, Any, dict[str, Any]]:
    """The kind of the format that `annotation` is, the default as the format writes it and what else the kind needs."""
    members = _scalar(annotation)
    nullable = type(None) in _members(annotation)
    if _choices(annotation):
        return "choice", default, {"choices": [str(choice) for choice in _choices(annotation)]}
    if members in ({bool}, {int}, {float}):
        return {bool: "bool", int: "int", float: "float"}[next(iter(members))], default, {}
    if members == {_COLOUR}:
        return "color", None if default is None else _hex(default), {"nullable": True} if nullable else {}
    if get_origin(annotation) is tuple and get_args(annotation) == (_COLOUR, Ellipsis):
        return "colors", [_hex(item) for item in default], {}
    if annotation == FieldName:
        return "field", default, {}
    if Lut in members:
        return "lut", None, {}
    if Picture in members:
        return "picture", None, {"nullable": True}
    if members == {str}:
        return "text", default, {}
    raise ValueError(
        "its type is not one the parameter format has (a number, a switch, a choice, a colour, a list of colours, "
        "a field name, a text, a picture or a lookup table)"
    )


def _entry(model: str, name: str, annotation: Any, spec: Any) -> dict[str, Any]:
    """The table entry of the field `name` of class `model`; ValueError naming both if its declaration is broken."""

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
    try:
        kind, default, written = _kind(annotation, spec.default)
    except ValueError as problem:
        raise broken(str(problem)) from None
    if kind in {"colors", "text"}:
        written |= {key: getattr(spec, key) for key in ("min_length", "max_length") if getattr(spec, key) is not None}
    if kind == "text" and spec.min_distinct is not None:
        written["min_distinct"] = spec.min_distinct
    numeric = kind in {"float", "int"}
    if not numeric and (spec.unit is not None or spec.min is not None or spec.max is not None):
        raise broken("only a number takes a unit, min and max")
    entry = {
        "name": name,
        "kind": kind,
        "meaning": spec.doc,
        "default": default,
        "unit": spec.unit if numeric else None,
        "min": spec.min if numeric else None,
        "max": spec.max if numeric else None,
        "cyclic": spec.cyclic,
        "drivable": spec.drivable,
        "advanced": spec.advanced,
        **written,
    }
    if spec.at_most is not None:
        entry["at_most"] = spec.at_most
    return entry


def table_of(model: type["EffectParams"]) -> dict[str, Any]:
    """The parameters of `model` in the ecosystem's parameter format (the dict `_core.register(.., table=)` takes):
    `{"doc": ..., "fields": [...]}`, each field with its kind, meaning, default, unit, bounds and flags."""
    return model._table  # noqa: SLF001  (the class's own, made when it was declared)


def _declare(cls: type["EffectParams"]) -> dict[str, Any]:
    """The table of `cls`: from its annotated fields, each with its `param`; the core proves the declaration."""
    fields = [
        _entry(cls.__name__, name, annotation, cls.__dict__.get(name, _MISSING))
        for name, annotation in inspect.get_annotations(cls, eval_str=True).items()
        if not name.startswith("_")
    ]
    table = {"doc": inspect.cleandoc(cls.__doc__ or ""), "fields": fields}
    try:
        native.check_table(table, cls.__name__, {})  # reads the declaration, and checks that every default passes
    except ValueError as error:
        raise ValueError(f"parameter model {cls.__name__!r}: {error}") from None
    return table


def prepare(table: Mapping[str, Any], values: Mapping[str, Any]) -> tuple[dict[str, Any], dict[str, Any]]:
    """`values` as data the core reads, and the objects held back: a `Lut` or a `Picture` goes as a placeholder name
    (a colour given for a picture becomes a flat `Picture`), an array or numpy number as the list or number it holds."""
    kinds = {entry["name"]: entry["kind"] for entry in table["fields"]}
    sent: dict[str, Any] = {}
    held: dict[str, Any] = {}
    for name, value in values.items():
        kind = kinds.get(name)
        if kind == "lut" and value is not None:
            if not isinstance(value, Lut):
                shown = f"an array of shape {value.shape}" if isinstance(value, np.ndarray) else repr(value)[:40]
                raise RecipeError(
                    f"`{name}`: a lookup table is a Lut; a path or an array is turned into a Lut by a recipe file or "
                    f"fx, got {shown}"
                )
            held[name], value = value, name
        elif kind == "picture" and value is not None:
            held[name] = value if isinstance(value, Picture) else _flat(name, value)
            value = name
        elif isinstance(value, np.ndarray):
            value = value.tolist()
        elif isinstance(value, np.generic):
            value = value.item()
        elif isinstance(value, list | tuple):
            value = [item.item() if isinstance(item, np.generic) else item for item in value]
        sent[name] = value
    return sent, held


def _flat(name: str, value: object) -> Picture:
    try:
        return Picture.of_color(value)
    except ValueError:
        shown = f"an array of shape {value.shape}" if isinstance(value, np.ndarray) else repr(value)[:40]
        raise RecipeError(
            f"`{name}`: a picture is a Picture or a colour ('#rrggbb', '#rgb' or three numbers 0..1); a path or an "
            f"array is turned into a Picture by a recipe file or fx, got {shown}"
        ) from None


def restore(table: Mapping[str, Any], checked: Mapping[str, Any], held: Mapping[str, Any]) -> dict[str, Any]:
    """What the core answered for the values (every parameter, normal form) as the values an effect receives: a colour
    as a tuple, a list of colours as a tuple of them, the `Lut` or `Picture` given back and a table left out the
    identity table."""
    made = {}
    for entry in table["fields"]:
        name, value = entry["name"], checked[entry["name"]]
        match entry["kind"]:
            case "lut":
                value = held.get(name) or Lut.identity()
            case "picture":
                value = held.get(name)
            case "color" if value is not None:
                value = tuple(value)
            case "colors":
                value = tuple(tuple(item) for item in value)
        made[name] = value
    return made


def from_checked[P: EffectParams](model: type[P], values: Mapping[str, Any]) -> P:
    """The instance of `model` for `values`, which the core has checked and scaled (every field, a colour as a list, a
    lookup table and a picture as the objects they stand for): built as they are, with no check, because a pixel value
    scaled to a bigger picture may lie beyond its design range."""
    made = object.__new__(model)
    for entry in model._table["fields"]:  # noqa: SLF001  (the class's own)
        name, value = entry["name"], values[entry["name"]]
        if entry["kind"] == "lut" and value is None:
            value = Lut.identity()  # a table left out is the identity table
        elif entry["kind"] == "color" and value is not None:
            value = tuple(value)
        elif entry["kind"] == "colors":
            value = tuple(tuple(item) for item in value)
        object.__setattr__(made, name, value)
    return made


class EffectParams:
    """Base class of a plugin's parameters: subclass it and declare each field with `param`.

    A text parameter is taken exactly as given, spaces included (a space can be data, as in a text-art palette).
    Make one with keywords (`BloomParams(radius=30)`): the core checks them and a bad value is a `RecipeError` naming
    the class, the field, the rule and the value. An instance has an attribute per field and is frozen.
    """

    _table: ClassVar[dict[str, Any]]

    def __init_subclass__(cls, **kwargs: Any) -> None:
        """Declare the class: its table is made once, here, and the core proves it."""
        super().__init_subclass__(**kwargs)
        cls._table = _declare(cls)

    def __init__(self, **values: Any) -> None:
        """The parameters for `values`, every other field at its default; the core checks them."""
        for name, entry in self.fields().items():
            if name not in values:
                values[name] = default_of(entry)
        sent, held = prepare(self._table, values)
        checked = native.check_table(self._table, type(self).__name__, sent)
        for name, value in restore(self._table, checked, held).items():
            object.__setattr__(self, name, value)

    @classmethod
    def fields(cls) -> dict[str, Mapping[str, Any]]:
        """The entries of the table by field name, in declaration order."""
        return {entry["name"]: entry for entry in cls._table["fields"]}

    def to_dict(self) -> dict[str, Any]:
        """The values by field name: a colour as a tuple, a `Lut` or `Picture` as the object."""
        return {name: getattr(self, name) for name in self.fields()}

    def __setattr__(self, name: str, value: object) -> None:
        raise FrozenInstanceError(f"cannot assign to field {name!r}")

    def __delattr__(self, name: str) -> None:
        raise FrozenInstanceError(f"cannot delete field {name!r}")

    def __eq__(self, other: object) -> bool:
        """Equal when of one class and every field is."""
        return isinstance(other, EffectParams) and type(other) is type(self) and self.to_dict() == other.to_dict()

    def __hash__(self) -> int:
        """By the values, as `==`."""
        return hash((type(self), *self.to_dict().values()))

    def __repr__(self) -> str:
        """`BloomParams(radius=24.0, tint=(1.0, 1.0, 1.0))`."""
        return f"{type(self).__name__}({', '.join(f'{k}={v!r}' for k, v in self.to_dict().items())})"

    def replace(self, **changes: Any) -> Self:
        """A copy with `changes` laid over the values, checked as a new value is."""
        return type(self)(**{**self.to_dict(), **changes})
