"""What the parameter table of an effect says, read for `fx`, the loader, the gallery and a step.

A table is the dict of the parameter format (`{"doc": ..., "fields": [...]}`, see `docs/parameters.md`): the core's
own for an effect it runs, `params.table_of` for a plugin's model. Every function here reads that one representation,
so an effect of either kind looks the same to its callers. The core owns the rules (`native.check`); this module only
names which fields are pictures, tables or selections, writes a signature and splits the documentation into parts.

Dependency direction: imports `values` and `pieces` only.
"""

import inspect
from collections.abc import Mapping, Sequence
from typing import Any, Literal, NamedTuple

import numpy as np

from .pieces import Drive
from .types import Color as ColorForms
from .values import Lut, LutForms, Picture, PictureForms

Table = Mapping[str, Any]

_TYPES: dict[str, Any] = {
    "float": float,
    "int": int,
    "bool": bool,
    "color": ColorForms,
    "colors": Sequence[ColorForms],
    "lut": LutForms,
    "picture": PictureForms,
    "field": str,
    "text": str,
}  # the kinds of the format -> what a caller writes; a "choice" is a `Literal` of its names


def _fields(table: Table) -> list[Mapping[str, Any]]:
    return table["fields"]


def names(table: Table) -> tuple[str, ...]:
    """The parameters' names, in declaration order."""
    return tuple(entry["name"] for entry in _fields(table))


def _named(table: Table, kind: str) -> tuple[str, ...]:
    return tuple(entry["name"] for entry in _fields(table) if entry["kind"] == kind)


def picture_fields(table: Table) -> frozenset[str]:
    """The parameters that hold a `Picture` (or none): a recipe file writes a path for one, `fx` a path or an array."""
    return frozenset(_named(table, "picture"))


def lut_fields(table: Table) -> frozenset[str]:
    """The parameters that hold a `Lut`: a recipe file writes a path to a `.cube` file, `fx` a path or an array."""
    return frozenset(_named(table, "lut"))


def field_name_fields(table: Table) -> tuple[str, ...]:
    """The parameters that name a field (a selection), in declaration order: the effect receives the array of each."""
    return _named(table, "field")


def default_of(entry: Mapping[str, Any]) -> Any:
    """The default of the parameter `entry` as a caller sees it: a list of colours as a tuple, a table as the identity
    table."""
    default = entry["default"]
    if entry["kind"] == "lut":
        return Lut.identity()  # a table left out is the identity table
    return tuple(default) if entry["kind"] == "colors" else default


def arrays_as_values(table: Table, values: Mapping[str, Any]) -> dict[str, Any]:
    """`values` (parameters of the table) with each array given for a picture parameter (3-d) or a lookup table
    parameter turned into the `Picture` or `Lut` it holds; everything else passes unchanged (a path stays a path:
    reading files is the loader's, `fx`'s job). The one place an array becomes a value, for `fx` and `Step.replace`
    alike."""
    made = dict(values)
    for key in picture_fields(table) & values.keys():
        if isinstance(values[key], np.ndarray) and values[key].ndim == 3:  # anything else is `Picture`'s to refuse
            made[key] = Picture(values[key])
    for key in lut_fields(table) & values.keys():
        if isinstance(values[key], np.ndarray):
            made[key] = Lut(values[key])
    return made


def _annotation(entry: Mapping[str, Any]) -> Any:
    """What a caller writes for the parameter `entry`."""
    kind = entry["kind"]
    written = Literal[tuple(entry["choices"])] if kind == "choice" else _TYPES[kind]
    if entry.get("nullable"):
        written = written | None
    if entry["drivable"]:
        written = written | Drive
    return written


def signature(table: Table) -> inspect.Signature:
    """The parameters as a call signature, in declaration order: each a keyword-only parameter with its default and the
    type a caller writes (a colour as `str | list[float] | tuple[float, float, float]`; a drivable number as
    `float | Drive`)."""
    return inspect.Signature(
        [
            inspect.Parameter(
                entry["name"], inspect.Parameter.KEYWORD_ONLY, default=default_of(entry), annotation=_annotation(entry)
            )
            for entry in _fields(table)
        ]
    )


class FieldDoc(NamedTuple):
    """The documentation of one parameter, in parts (see `fields_doc`)."""

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


_WORDS = {
    "float": "number",
    "int": "whole number",
    "bool": "true or false",
    "color": "colour",
    "colors": "list of colours",
    "field": "field name",
    "lut": "lookup table",
    "picture": "picture",
    "text": "text",
}


def _type_text(entry: Mapping[str, Any]) -> str:
    kind = entry["kind"]
    text = "one of " + ", ".join(repr(choice) for choice in entry["choices"]) if kind == "choice" else _WORDS[kind]
    if entry.get("nullable"):
        text += " or none"
    return text + (" (drivable)" if entry["drivable"] else "") + (" (cyclic)" if entry["cyclic"] else "")


def _number(value: float) -> str:
    """A bound as a person writes it: 0, 500, 0.5, 4294967295."""
    return str(int(value)) if float(value).is_integer() and abs(value) < 1e15 else repr(value)


def fields_doc(table: Table) -> tuple[FieldDoc, ...]:
    """Every parameter's documentation, in declaration order."""
    return tuple(
        FieldDoc(
            entry["name"],
            _type_text(entry),
            entry["unit"],
            entry["min"],
            entry["max"],
            default_of(entry),
            entry["meaning"],
            entry["advanced"],
            entry["cyclic"],
        )
        for entry in _fields(table)
    )
