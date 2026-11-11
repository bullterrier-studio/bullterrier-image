"""The data a recipe holds besides its steps: the framing, the regions and a drive. The core owns their rules.

Each is a frozen dataclass of keyword fields, every field with its default and its documentation (`help(Grid)` lists
them). Making one asks the core: it reads the fields strictly (a field the kind lacks, a value of another kind or one
outside its range is a `RecipeError` naming the field and the value) and answers with the piece in its normal form (a
count of shares as the list, nested parts as their own dicts), which is what the instance holds. Nothing here checks a
rule: a rule the core changes changes here with it. What a piece names (`fit`, `of`, `per`, a drive's `by`) is the
recipe's to look up.

Dependency direction: imports `native` only.
"""

import textwrap
from collections.abc import Callable
from dataclasses import dataclass, field, fields, is_dataclass
from typing import Any, ClassVar

from . import native

__all__ = ["Drive", "plain"]


def plain(value: Any) -> Any:
    """`value` as data the core reads: a piece as its dict, a tuple or array as a list."""
    if is_dataclass(value) and not isinstance(value, type):
        return {each.name: plain(getattr(value, each.name)) for each in fields(value)}
    if isinstance(value, list | tuple):
        return [plain(item) for item in value]
    if hasattr(value, "tolist"):  # a numpy array or number, which scripts pass
        return plain(value.tolist())
    return value


def _frozen(value: Any) -> Any:
    """`value`, what the core answered, with its lists as tuples, so that a piece is hashable."""
    if isinstance(value, list | tuple):
        return tuple(_frozen(item) for item in value)
    return value


@dataclass(frozen=True, kw_only=True)
class _Piece:
    """What the pieces share: made by the core's answer to their fields."""

    _normal: ClassVar[Callable[[dict[str, Any]], dict[str, Any]]]
    _make: ClassVar[dict[str, Callable[[Any], Any]]] = {}  # the parts that are pieces of their own, by field name

    def __post_init__(self) -> None:
        """Ask the core for the normal form of the fields and hold that. Only a field the caller passed is sent: the
        core fills in the rest, and a preset (`Shards(preset=...)`) gives the fields beside it only what was passed."""
        # A piece made without `__init__` (pydantic revalidating a recipe) holds every field: all are sent.
        names = getattr(self, "_given", None) or [each.name for each in fields(self)]
        given = {name: plain(getattr(self, name)) for name in names}
        answer = type(self)._normal(given)  # noqa: SLF001  (the class's own)
        for each in fields(self):
            made = self._make.get(each.name)
            object.__setattr__(self, each.name, made(answer[each.name]) if made else _frozen(answer[each.name]))


def piece[P](normal: Callable[[dict[str, Any]], dict[str, Any]] | None = None) -> Callable[[type[P]], type[P]]:
    """A class decorator for a frozen keyword dataclass (below it): `normal` (given the fields as a dict, the
    core's answer) makes its instances, and its docstring ends with the documentation of its fields."""

    def wrap(cls: type[P]) -> type[P]:
        made = cls
        made_init = made.__init__

        def init(self: Any, /, **values: Any) -> None:
            object.__setattr__(self, "_given", frozenset(values))
            made_init(self, **values)

        made.__init__ = init  # type: ignore[method-assign,assignment]
        if normal is not None:
            made._normal = staticmethod(normal)  # type: ignore[attr-defined]  # noqa: SLF001
        described = [
            textwrap.fill(f"{each.name} : {each.metadata['doc']}", 100, initial_indent="", subsequent_indent="    ")
            for each in fields(made)  # type: ignore[arg-type]
            if "doc" in each.metadata
        ]
        if described:
            made.__doc__ = f"{(made.__doc__ or '').strip()}\n\nAttributes\n----------\n" + "\n".join(described)
        return made

    return wrap


@piece(native.drive)
@dataclass(frozen=True, kw_only=True)
class Drive(_Piece):
    """A parameter that varies across the picture: its value at a pixel is `start` where the field `by` is 0 and `end`
    where it is 1, and in between along `levels` evenly spaced values.

    The effect is rendered once at each of the `levels` values (so a driven step costs `levels` times the effect; a
    value no pixel needs is not rendered) and each pixel takes the two renders either side of its field value, mixed
    linearly. A field is a selection of the recipe or a built-in one (`depth`, `light`, `subject`...). Written in a
    recipe file as the table `radius = { by = "near", start = 0, end = 30 }` and in Python as
    `Drive(by="near", start=0, end=30)`; only a parameter its effect declares drivable takes one, and `start` and `end`
    (so every value between) must lie in that parameter's range; a cyclic parameter takes any angle.
    """

    by: str = field(
        metadata={
            "doc": "The name of the field, a selection of the recipe or a built-in one (`depth`, `light`, `subject`, "
            "`background`): the parameter is `start` where it is 0 and `end` where it is 1."
        }
    )
    start: float = field(metadata={"doc": "The parameter's value where the field is 0, in its own unit."})
    end: float = field(metadata={"doc": "The parameter's value where the field is 1, in its own unit."})
    levels: int = field(
        default=5,
        metadata={
            "doc": "How many evenly spaced values between `start` and `end` the effect is rendered at (2 to 16); each "
            "pixel mixes the two nearest. More is smoother and costs more renders; 2 mixes the effect at `start` and "
            "at `end`."
        },
    )

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
