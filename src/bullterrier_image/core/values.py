"""The value types a parameter can hold: `Color`, `FieldName`, `Picture` and `Lut`.

Promises:
- `Color` is three floats 0..1, written '#rrggbb', '#rgb' or as three numbers 0..1; `FieldName` is the name of a
  selection.
- `Picture` and `Lut` are read-only and compare, hash and print by a content digest, never by their arrays; a wrong
  array is a `RecipeError` (`Picture`) or a `LutError` (`Lut`).
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

from bullterrier_image.errors import LutError, RecipeError

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


_LUT_MAX = 256  # the largest LUT_3D_SIZE the Adobe/Resolve .cube specification allows


class Lut:
    """A 3D colour lookup table an effect uses besides the photo (a film look, a grade): read-only and hashable.

    `Lut(table)`: `table` is a finite float array (n, n, n, 3) with 2 <= n <= 256, indexed `[red, green, blue]` by the
    input colour's position on each axis, holding the output colour; `domain_min` and `domain_max` are the input
    values at the first and last entry of each axis (three numbers each, min below max). Anything else is a
    `LutError`. `Lut.identity()` is the table that changes nothing. Two tables are equal when their values are, and
    hash alike; `label` (a file name, say) is only for messages.
    """

    __slots__ = ("digest", "domain_max", "domain_min", "label", "table")

    def __init__(
        self,
        table: np.ndarray,
        domain_min: Sequence[float] = (0.0, 0.0, 0.0),
        domain_max: Sequence[float] = (1.0, 1.0, 1.0),
        label: str = "array",
    ) -> None:
        """The table, with the domain it covers, called `label` in messages."""
        data = np.asarray(table)
        size = data.shape[0] if data.ndim else 0
        if data.shape != (size, size, size, 3) or data.dtype.kind != "f" or not 2 <= size <= _LUT_MAX:
            raise LutError(
                f"a lookup table is a float array of shape (n, n, n, 3) with 2 to {_LUT_MAX} entries per axis, got "
                f"{data.dtype} of shape {data.shape}"
            )
        low, high = np.asarray(domain_min, np.float64), np.asarray(domain_max, np.float64)
        if low.shape != (3,) or high.shape != (3,) or not (np.isfinite(low).all() and np.isfinite(high).all()):
            raise LutError(f"{label}: the domain is three finite numbers for its minimum and for its maximum")
        if not (low < high).all():
            raise LutError(f"{label}: the domain minimum must be below its maximum on every channel, got {low}, {high}")
        if not np.isfinite(data).all():
            raise LutError(f"{label}: the values of a lookup table must be finite")
        values = np.array(data, np.float32)
        values.setflags(write=False)
        self.table, self.label = values, label
        self.domain_min, self.domain_max = tuple(low.tolist()), tuple(high.tolist())
        self.digest = hashlib.sha256(values.tobytes() + low.tobytes() + high.tobytes()).hexdigest()

    @classmethod
    def identity(cls, size: int = 2) -> Self:
        """The table of `size` entries per axis that returns every colour as it is, over the domain 0..1."""
        axis = np.linspace(0.0, 1.0, size, dtype=np.float32)
        red, green, blue = np.meshgrid(axis, axis, axis, indexing="ij")
        return cls(np.stack([red, green, blue], axis=-1), label="identity")

    @property
    def size(self) -> int:
        """Entries per axis."""
        return int(self.table.shape[0])

    def __eq__(self, other: object) -> bool:
        """Equal values and domain (by digest), whatever the labels."""
        return self.digest == other.digest if isinstance(other, Lut) else NotImplemented

    def __hash__(self) -> int:
        """By digest, as `==`."""
        return hash(self.digest)

    def __repr__(self) -> str:
        """`Lut('film.cube', 33)`: label, then entries per axis, never the values; `Lut.identity(2)` for an identity."""
        return f"Lut.identity({self.size})" if self.label == "identity" else f"Lut({self.label!r}, {self.size})"


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
