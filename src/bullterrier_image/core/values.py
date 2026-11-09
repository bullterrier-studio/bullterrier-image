"""The value types a parameter can hold: `Color`, `FieldName`, `Picture` and `Lut`.

Promises:
- `Color` is three floats 0..1, written '#rrggbb', '#rgb' or as three numbers 0..1; `FieldName` is the name of a
  selection.
- `Picture` and `Lut` are read-only and compare, hash and print by a content digest, never by their arrays; a wrong
  array is a `RecipeError` (`Picture`) or a `LutError` (`Lut`). A `Lut` is the Rust core's table (checked there once,
  shared by every render), a `Picture` holds its pixels.
- `ValidatedPicture` and `ValidatedLut` are what a `Picture` or `Lut` field is validated as; `PictureForms`, `LutForms`
  and `ColorForms` (from `types`) are what a caller may write in `fx`.
"""

import hashlib
import math
import os
import re
from collections.abc import Sequence
from typing import Annotated, Any, Self

import numpy as np
from PIL import ImageColor
from pydantic import BeforeValidator, PlainValidator, StringConstraints, WithJsonSchema

from bullterrier_image._native import _core
from bullterrier_image.errors import LutError, RecipeError

from . import native
from .types import Color as ColorForms

_HEX = re.compile(r"#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6})")


def is_number(value: object) -> bool:
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
    if len(parts) == 3 and all(is_number(c) and math.isfinite(c) and 0 <= c <= 1 for c in parts):
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


FieldName = Annotated[str, StringConstraints(min_length=1)]
"""A parameter that names a field: the name of a selection (`depth`, `light`, `subject`, `background` or a region the
recipe declares). The runner builds the named field; the effect receives it as an array under the parameter's name."""


class Picture:
    """A picture an effect uses besides the photo (a texture, a logo, a cut-out): read-only RGBA float32 pixels.

    `Picture(array)`: `array` is float (height, width, 3 or 4), values 0..1, RGB or straight-alpha RGBA (RGB gets
    alpha 1); anything else is a `RecipeError`. The pixels are copied and locked, so a Picture never changes.
    `Picture.of_color` makes a flat 1x1 picture. Two pictures are equal when their pixels are, and hash alike, so a
    `Step` holding one compares, hashes and prints without touching an array; `label` (a file name, say) is only for
    messages.
    """

    __slots__ = ("digest", "label", "pixels")

    def __init__(self, array: np.ndarray, label: str = "array") -> None:
        """The picture of `array`, called `label` in messages."""
        data = np.asarray(array)
        if data.ndim != 3 or data.shape[2] not in (3, 4) or min(data.shape[:2]) < 1 or data.dtype.kind != "f":
            raise RecipeError(
                f"a picture is a float array of shape (height, width, 3 or 4) with values 0..1, got {data.dtype} "
                f"of shape {data.shape}"
            )
        if not (np.isfinite(data).all() and data.min() >= 0 and data.max() <= 1):
            raise RecipeError(f"a picture's values are 0..1 and finite; {label!r} has others")
        pixels = np.ones((*data.shape[:2], 4), np.float32)
        pixels[..., : data.shape[2]] = data
        pixels.setflags(write=False)
        self.pixels, self.label = pixels, label
        self.digest = hashlib.sha256(np.asarray(pixels.shape, np.int64).tobytes() + pixels.tobytes()).hexdigest()

    @classmethod
    def of_color(cls, color: object) -> Self:
        """A flat 1x1 picture of `color` ('#rrggbb', '#rgb' or three numbers 0..1), opaque."""
        red, green, blue = _to_color(color)
        label = "#" + "".join(f"{round(channel * 255):02x}" for channel in (red, green, blue))  # constant: 8-bit levels
        return cls(np.array([[[red, green, blue]]], np.float32), label)

    @property
    def width(self) -> int:
        """Pixels across."""
        return int(self.pixels.shape[1])

    @property
    def height(self) -> int:
        """Pixels down."""
        return int(self.pixels.shape[0])

    def __eq__(self, other: object) -> bool:
        """Equal pixels (by digest), whatever the labels."""
        return self.digest == other.digest if isinstance(other, Picture) else NotImplemented

    def __hash__(self) -> int:
        """By digest, as `==`."""
        return hash(self.digest)

    def __repr__(self) -> str:
        """`Picture('tex.png', 480x640)`: label, then width x height; never the pixels."""
        return f"Picture({self.label!r}, {self.width}x{self.height})"


def _to_picture(value: object) -> Picture:
    """A `Picture` as it is (validation runs at every apply: no copy), or a colour as a flat picture."""
    if isinstance(value, Picture):
        return value
    try:
        return Picture.of_color(value)
    except ValueError:
        shown = f"an array of shape {value.shape}" if isinstance(value, np.ndarray) else repr(value)[:40]
        raise ValueError(
            f"a picture is a Picture or a colour ('#rrggbb', '#rgb' or three numbers 0..1); a path or an array is "
            f"turned into a Picture by a recipe file or fx, got {shown}"
        ) from None


ValidatedPicture = Annotated[
    Picture,
    PlainValidator(_to_picture),
    WithJsonSchema(
        {
            "anyOf": [
                {"type": "string", "description": "a path, relative to the recipe file, or a colour '#rrggbb'"},
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
"""What a `Picture` field is made of: a `Picture` or a colour is accepted (a path or an array only through `loader`)."""

PictureForms = Picture | os.PathLike[str] | np.ndarray | ColorForms
"""A picture as a caller may write it in `fx`: a Picture, a path, an array, or a colour."""


class Lut:
    """A 3D colour lookup table an effect uses besides the photo (a film look, a grade): read-only and hashable.

    `Lut(table)`: `table` is a finite float array (n, n, n, 3) with 2 <= n <= 256, indexed `[red, green, blue]` by the
    input colour's position on each axis, holding the output colour; `domain_min` and `domain_max` are the input
    values at the first and last entry of each axis (three numbers each, min below max). Anything else is a
    `LutError`. `Lut.identity()` is the table that changes nothing. The table is held by the Rust core, which checked it
    and shares it with every render that uses it; `table` is a read-only copy of its entries as float32. Two tables
    are equal when their values are, and hash alike; `label` (a file name, say) is only for messages.
    """

    __slots__ = ("_core", "_digest", "label")
    _digest: str | None

    def __init__(
        self,
        table: np.ndarray,
        domain_min: Sequence[float] = (0.0, 0.0, 0.0),
        domain_max: Sequence[float] = (1.0, 1.0, 1.0),
        label: str = "array",
    ) -> None:
        """The table, with the domain it covers, called `label` in messages."""
        data = np.asarray(table)
        if data.dtype.kind != "f":
            raise LutError(
                f"a lookup table is a float array of shape (n, n, n, 3), got {data.dtype} of shape {data.shape}"
            )
        self._set(native.make_lut(data.astype(np.float32), tuple(domain_min), tuple(domain_max)), label)

    def _set(self, held: _core.Lut, label: str) -> None:
        self._core, self.label, self._digest = held, label, None

    @classmethod
    def identity(cls, size: int = 2) -> Self:
        """The table of `size` entries per axis (2 to 256) that returns every colour as it is, over the domain 0..1."""
        made = cls.__new__(cls)
        made._set(native.identity_lut(size), "identity")  # noqa: SLF001  (a table of the core, labelled)
        return made

    @property
    def size(self) -> int:
        """Entries per axis."""
        return int(self._core.size)

    @property
    def table(self) -> np.ndarray:
        """The entries, a new read-only float32 array (n, n, n, 3) indexed `[red, green, blue]`."""
        entries = self._core.table()
        entries.setflags(write=False)
        return entries

    @property
    def domain_min(self) -> tuple[float, float, float]:
        """The input value at the first entry of each axis (red, green, blue)."""
        return self._core.domain_min

    @property
    def domain_max(self) -> tuple[float, float, float]:
        """The input value at the last entry of each axis (red, green, blue)."""
        return self._core.domain_max

    @property
    def digest(self) -> str:
        """A SHA-256 hex digest of the entries and the domain: equal tables have equal digests."""
        if self._digest is None:
            domain = np.asarray([*self.domain_min, *self.domain_max], np.float32)
            self._digest = hashlib.sha256(self.table.tobytes() + domain.tobytes()).hexdigest()
        return self._digest

    def __eq__(self, other: object) -> bool:
        """Equal values and domain (by digest), whatever the labels."""
        return self.digest == other.digest if isinstance(other, Lut) else NotImplemented

    def __hash__(self) -> int:
        """By digest, as `==`."""
        return hash(self.digest)

    def __copy__(self) -> Self:
        """The table itself: it is read-only, and its Rust table cannot be pickled, which `copy` would try."""
        return self

    def __deepcopy__(self, _memo: dict[int, object]) -> Self:
        """The table itself, as `__copy__`."""
        return self

    def __repr__(self) -> str:
        """`Lut('film.cube', 33)`: label, then entries per axis, never the values; `Lut.identity(2)` for an identity."""
        return f"Lut.identity({self.size})" if self.label == "identity" else f"Lut({self.label!r}, {self.size})"


def read_lut(path: str | os.PathLike[str]) -> Lut:
    """The `Lut` of the Adobe/Resolve `.cube` file at `path`, read by the core and labelled with `path` as given; the
    rules and the `LutError`s are `imageio.load_lut`'s."""
    made = Lut.__new__(Lut)
    made._set(native.read_lut(path), str(path))  # noqa: SLF001  (a table of the core, labelled)
    return made


def core_table(lut: Lut) -> _core.Lut:
    """The core's table of `lut`, which a render or `apply` is given."""
    return lut._core  # noqa: SLF001  (the core layer hands it on)


def _to_lut(value: object) -> Lut:
    """A `Lut` as it is (validation runs at every apply: no copy)."""
    if isinstance(value, Lut):
        return value
    shown = f"an array of shape {value.shape}" if isinstance(value, np.ndarray) else repr(value)[:40]
    raise ValueError(
        f"a lookup table is a Lut; a path or an array is turned into a Lut by a recipe file or fx, got {shown}"
    )


ValidatedLut = Annotated[
    Lut,
    PlainValidator(_to_lut),
    WithJsonSchema({"type": "string", "description": "a path to a .cube file, relative to the recipe file"}),
]
"""What a `Lut` field is made of: a `Lut` is accepted (a path or an array only through `loader`)."""

LutForms = Lut | os.PathLike[str] | str | np.ndarray
"""A lookup table as a caller may write it in `fx`: a Lut, a path to a `.cube` file, or an (n, n, n, 3) array."""
