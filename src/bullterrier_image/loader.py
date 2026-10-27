"""Reading a recipe from a small TOML file: `load_recipe(path)` returns a validated `Recipe`.

Promises: every error is a `RecipeError` that starts with the file's name, then names the step and the offending key
(a TOML syntax error keeps its line and column); nothing is read except that one file. The format (UTF-8 TOML):

    format = 3                     # optional: the recipe format version; this release reads 1 (the default), 2 and 3
    name = "green-terminal"        # optional (default: the file name, made safe): letters, digits, . _ -
    description = "Green text."    # optional, one sentence; a built-in look must have one (see "Looks" below)
    size = 1024                    # optional: longest side in px the recipe is written for; absent = tuned for
                                   # 480, rendered at the photo's own size

    [frame]                        # optional: frame the photo before the steps, cut it out after them (see below)
    aspect = 1                     # crop to this width over height...
    anchor = "subject"             # ...around the subject
    transparent = "circle"         # a circle cut-out: the render has an alpha channel (PNG or WebP, not JPEG)

    [region.panes]                 # format 2: a named region (see "Regions" below); `kind` is grid, shards, cuts, ...
    kind = "grid"
    columns = 3
    rows = 2

    [[step]]
    effect = "ascii"               # any effect from bullterrier_image.effects; the other keys are its parameters
    cell = 9
    ink = "#00ff9c"

    [[step]]
    effect = "gradient"
    colors = ["#000000"]
    mask = "background"            # "subject", "background", "depth", "light" or a region that is a selection
    opacity = 0.3                  # optional, 0..1: mix the effect in partway (default 1 = fully)

    [[step]]
    effect = "gradient"
    blend_mode = "screen"          # optional: how the result combines with the picture below (default "normal"):
                                   # normal, multiply, screen, overlay, soft_light, difference or color

    [[step]]
    effect = "gradient_map"
    preset = "sepia"               # optional: one of the effect's presets (`bt-image effects NAME` lists them); the
    opacity = 0.6                  # step's own keys win over the preset's

Regions (format 2). A `[region.NAME]` table declares a region by its `kind`: `grid` (`columns`, `rows`), `shards`
(`count`, `jitter`, `impact`, `focus`, `seed`, `preset`), `cuts` (`lines`: tables `{x, y, angle}` or `{path}`), `path`
(`d`), `svg` (`file`, relative to the recipe file), `pick` (`of`, `pieces`: numbers or svg ids), `subject` (`per`,
`context`, `threshold`) and, from format 3, `band` (`of`, `low`, `high`, `feather`), `bands` (`of`, `count`), `combine`
(`op`, `of`), `linear_gradient` (`angle`), `radial_gradient` (`x`, `y`, `radius`) and `given` (no fields); they are the
fields of `Grid`, `Shards`, `Cuts`, `Path`, `Svg`, `Pick`, `Subject`, `Band`, `Bands`, `Combine`, `LinearGradient`,
`RadialGradient` and `Given`. `fit` lays a grid, shards, cuts, path or svg over a selection and `edge` is a table
(`style`, `amount`, `scale`, `seed`) that tears the borders. A *partition* (a grid, shards, cuts, svg or bands) cuts the
picture into numbered pieces, a *selection* (a `path`, a `pick`, a `subject`, a `band`, a `combine`, a gradient, a
`given` map or the built-in `subject`, `background`, `depth` and `light`) chooses pixels; a `subject` with `per` finds
the subject inside each piece of a partition. `band`, `bands` and `combine` are made of another selection named by `of`,
read for its values: the built-in `depth` (1 = nearest), `light` (the picture's brightness) or any other (bands are
numbered highest first). Steps name regions: `mask` (a selection) limits a step to it, and `partition` names the
partition an effect such as `seams` draws. A step with `each` (a partition) and `looks` gives every piece its own look:

    [[step]]
    each = "panes"                 # a partition
    looks = [{ effect = "halftone", cell = 9 }, "none"]   # each look is a step table, or "none": the piece stays
    assign = "cycle"               # or "random" (with `seed`): how the pieces get looks
    feather = 0.001                # soft borders between pieces, a share of the long side
    mask = "subject"               # as on any step; `opacity` and `blend_mode` apply to the blended looks

Regions and `each` steps need `format = 2`; a file without `format`, or with `format = 1`, may not use them. The region
kinds `given` (a map handed in at render time), `band`, `bands`, `combine`, `linear_gradient` and `radial_gradient` need
`format = 3`.

A number an effect declares drivable (its documentation says "(drivable)") may be a table instead of a value, which
varies it across the picture by a field (format 3): `radius = { by = "near", start = 0, end = 30 }` is 0 where the field
`by` (a selection region of the recipe, or the built-in `depth`, `light` or `subject`) is 0 and 30 where it is 1; the
effect is rendered at `levels` evenly spaced values between (2 to 16, default 5) and each pixel mixes the two nearest.
All the tables of one step name one `by` and one `levels`.

A step (an `each` step too) may take `name = "pulse"` (format 3: letters, digits and underscores, one per step), which
addresses its numbers, `pulse.radius`, for a program that sets them (`Recipe.parameters`, `Recipe.with_values`).

A parameter that is a picture (a texture, a logo; the effect's documentation says which, and an effect with one calls
it `image`) is written as a path, relative to the recipe file's folder, or as a colour (a string starting with `#`, or
three numbers):

    [[step]]
    effect = "blend"
    image = "../assets/grain.png"

The picture is read once, when the recipe is loaded; a file that is missing or not a picture fails there, naming the
file, the step, the parameter and the path.

A parameter that is a colour lookup table (`file` of `lut`) is written the same way, as the path of a `.cube` file
relative to the recipe file's folder; it is read once at load, and a file that is missing, 1D or malformed fails there
with a `LutError` naming the file, its line, the step and the parameter.

The `[frame]` table holds the fields of `Framing` (`Framing.documentation()` lists them with their units and ranges):
`flip`, `angle` (with `fit` and `resample`), `aspect` (with `anchor` and `margin`), `pad_top`, `pad_right`, `pad_bottom`
and `pad_left` (with `fill`), applied to the photo and the subject map in that order, before the first step and
before `size`; and `transparent` (with `corner_radius`), applied after the last step. The transparency lives in the
table with the geometry because it is the same model, `Recipe(..., frame=Framing(...))` in Python: one place says how a
recipe's picture is framed and cut. Any other top-level key is refused, so a misspelt `size` or `format` is not
silently ignored; so is any other key in `[frame]`.

Looks. The recipes that ship with the package (`bt-image effects` lists them under "Looks") load by name:
`load_recipe("crt")`. A `str` that is exactly a look's name is that look; to load a file of the same name write
`load_recipe("./crt")` or pass a `Path`.

Dependency direction: imports `core` (steps, framing, regions, registry, params, looks), `imageio` and `errors`; it does
not know `runner`.
"""

import tomllib
from collections.abc import Mapping
from os import PathLike
from pathlib import Path

import numpy as np

from bullterrier_image.errors import BullterrierImageError, ImageReadError, LutError, RecipeError

from . import imageio
from .core import looks, registry
from .core.framing import Framing
from .core.params import Lut, Picture
from .core.regions import (
    Band,
    Bands,
    Combine,
    Cuts,
    Given,
    Grid,
    Kind,
    LinearGradient,
    Pick,
    RadialGradient,
    Shards,
    Subject,
    Svg,
)
from .core.regions import Path as PathRegion  # `Path` is pathlib's here
from .core.steps import Each, Recipe, Step, safe_name

__all__ = ["load_recipe", "with_files"]

# The recipe format versions this release reads; a file without `format` is the first. A release that changes the
# format adds its number here and keeps reading the old ones. Format 2 added regions and the step keys `each`, `looks`
# and `partition`; a file of format 1 may not use them, so its meaning never changes. Format 3 added the region kinds
# `given`, `band`, `bands`, `combine`, `linear_gradient` and `radial_gradient`, a parameter driven by a field (a table
# with `by`) and the step key `name`, and a file of an earlier format may not use them. It also replaced the kinds
# `depth` and `depth_bands` by `band` and `bands` and took the region name `light`: the one place a file's meaning
# changed, so a file of any format that uses the old kinds is refused saying what to write, and one that declares a
# region called `light` is refused as it is for `subject`.
_FORMATS = (1, 2, 3)
_NEW_IN_2 = ("each", "looks", "partition")
_KINDS_NEW_IN_3 = ("given", "band", "bands", "combine", "linear_gradient", "radial_gradient")
# region kinds format 3 replaced, with what to write instead
_REPLACED_IN_3 = {
    "depth": 'write `kind = "band"` with `of = "depth"`, `low` where `far` was and `high` where `near` was',
    "depth_bands": 'write `kind = "bands"` with `of = "depth"` and the same `count`',
}
# the top-level keys of a recipe file, each as it is written (a table or a list of tables in brackets), in the order the
# error for an unknown one lists them
_KEYS = {
    "format": "format",
    "name": "name",
    "description": "description",
    "size": "size",
    "frame": "[frame]",
    "region": "[region.NAME]",
    "step": "[[step]]",
}
# the region kinds a `[region.NAME]` table may have, by its `kind`
_KINDS: dict[str, type[Kind]] = {
    "grid": Grid,
    "shards": Shards,
    "cuts": Cuts,
    "path": PathRegion,
    "svg": Svg,
    "pick": Pick,
    "subject": Subject,
    "band": Band,
    "bands": Bands,
    "combine": Combine,
    "linear_gradient": LinearGradient,
    "radial_gradient": RadialGradient,
    "given": Given,
}


def _located[E: BullterrierImageError](error: E, where: str) -> E:
    """`error` with `where` (the file, and the step) in front, keeping its class (an unknown effect stays an
    `UnknownEffectError`, a missing picture an `ImageReadError`)."""
    return type(error)(f"{where}: {error}")


def with_files(effect: str, values: Mapping[str, object], folder: Path) -> dict[str, object]:
    """`values` (the parameters of `effect`) with each picture or lookup table given as a path or an array turned into
    a `Picture` or a `Lut`.

    A path (a `str` not starting with `#`, which is a colour, or a `PathLike`; for a lookup table any `str`) is read
    with `imageio.load_picture` or `imageio.load_lut`, relative to `folder` and labelled as written; an array becomes
    a `Picture` (3-d) or a `Lut` (4-d). A `Picture`, a `Lut`, a colour and every other parameter pass unchanged.
    ImageReadError (LutError for a table) naming the effect, the parameter and the path when a file cannot be read;
    an unknown effect is the registry's UnknownEffectError.
    """
    model = registry.parameters(effect)
    resolved = dict(values)
    for key in model.picture_fields() & values.keys():
        value = values[key]
        if isinstance(value, PathLike) or (isinstance(value, str) and not value.startswith("#")):
            try:
                resolved[key] = Picture(imageio.load_picture(folder / value), str(value))
            except ImageReadError as error:
                raise ImageReadError(f"effect {effect!r}: `{key}`: {error}") from None
        elif isinstance(value, np.ndarray) and value.ndim == 3:
            resolved[key] = Picture(value)
    for key in model.lut_fields() & values.keys():
        value = values[key]
        if isinstance(value, PathLike | str):
            try:
                resolved[key] = imageio.load_lut(folder / value)
            except LutError as error:
                raise LutError(f"effect {effect!r}: `{key}`: {error}") from None
        elif isinstance(value, np.ndarray):
            resolved[key] = Lut(value)
    return resolved


def _read(path: Path) -> dict:
    """The file's top-level table, checked: UTF-8 TOML, a format this release reads, and only the known keys."""
    try:
        with path.open("rb") as f:
            raw = tomllib.load(f)
    except tomllib.TOMLDecodeError as error:  # its message keeps the line and column
        raise RecipeError(f"{path.name}: {error}") from None
    except UnicodeDecodeError as error:
        raise RecipeError(f"{path.name}: not UTF-8 text ({error.reason} at byte {error.start})") from None
    version = raw.get("format", _FORMATS[0])
    if type(version) is not int or version not in _FORMATS:  # not `in` alone: true == 1 and 1.0 == 1 in Python
        raise RecipeError(
            f"{path.name}: `format` {version!r} is not one this release reads; it reads format "
            f"{', '.join(map(str, _FORMATS))}"
        )
    unknown = sorted(set(raw) - set(_KEYS))
    if unknown:
        raise RecipeError(f"{path.name}: `{unknown[0]}` is not a recipe key; the keys are: {', '.join(_KEYS.values())}")
    if "description" in raw and (type(raw["description"]) is not str or not raw["description"].strip()):
        raise RecipeError(f"{path.name}: `description` is {raw['description']!r}; write a sentence in quotes")
    return raw


def load_recipe(path: str | Path) -> Recipe:
    """Read a recipe file, or a built-in look by name, and return the validated `Recipe`.

    path: a TOML file in the format above, or a `str` that is exactly the name of a built-in look ("crt"). A `name` the
    file sets is used as written (and refused if it is not a safe file name); without one the recipe is named after
    the file, made safe (see `steps.safe_name`: "my look.toml" gives "my-look"), and a file name that leaves nothing
    usable ("日本.toml", "con.toml") is refused, asking for a `name`.
    Raises RecipeError (a ValueError; an unknown effect is an UnknownEffectError) for anything invalid, starting with
    the file's name, then the step and the offending key; OSError (FileNotFoundError...) if the file cannot be opened.
    """
    shipped = looks.read()
    path = shipped[path].file if isinstance(path, str) and path in shipped else Path(path)
    raw = _read(path)
    recipe_name = raw["name"] if "name" in raw else safe_name(path.stem)
    if not recipe_name:
        raise RecipeError(
            f'{path.name}: the file name gives no usable recipe name: add name = "..." (letters, digits, . _ -) or '
            "rename the file"
        )
    table = raw.get("frame", {})
    if not isinstance(table, dict):
        raise RecipeError(
            f"{path.name}: write the framing as a [frame] table (one: its keys are the fields of Framing)"
        )
    try:
        frame = Framing(**table)
    except RecipeError as error:
        raise _located(error, f"{path.name}: [frame]") from None
    entries = raw.get("step", [])
    if not isinstance(entries, list) or not all(isinstance(entry, dict) for entry in entries):
        raise RecipeError(f"{path.name}: write each step as a [[step]] table (two brackets: there may be several)")
    tables = raw.get("region", {})
    if raw.get("format", _FORMATS[0]) == 1 and (tables or any(key in entry for entry in entries for key in _NEW_IN_2)):
        raise RecipeError(
            f"{path.name}: regions, `each`, `looks` and `partition` need `format = 2` at the top of the file"
        )
    if raw.get("format", _FORMATS[0]) < 3 and isinstance(tables, dict):
        for name, table in tables.items():
            if isinstance(table, dict) and table.get("kind") in _KINDS_NEW_IN_3:
                raise RecipeError(
                    f"{path.name}: region {name!r}: `kind` {table['kind']!r} needs `format = 3` at the top of the file"
                )
    regions = _regions(tables, path.name, path.parent)
    steps = []
    for i, entry in enumerate(entries, start=1):
        where = f"{path.name}: step {i}"
        if raw.get("format", _FORMATS[0]) < 3 and "name" in entry:
            raise RecipeError(f"{where}: `name` needs `format = 3` at the top of the file")
        if raw.get("format", _FORMATS[0]) < 3 and (drive := _drive_in(entry)) is not None:
            raise RecipeError(
                f"{where}{drive[0]}: `{drive[1]}` is a table with `by`, a drive, which needs `format = 3`"
            )
        try:
            steps.append(_step(entry, where, path.parent))
        except (RecipeError, ImageReadError, LutError) as error:
            raise _located(error, where) from None
    try:
        return Recipe(recipe_name, tuple(steps), raw.get("size"), frame, regions)
    except RecipeError as error:
        raise _located(error, path.name) from None


def _drive_in(entry: dict) -> tuple[str, str] | None:
    """The first parameter of the step table `entry` (or of a look in its `looks`) that is a drive, a table with `by`,
    as (where in the step, the parameter's name); None when there is none."""
    for key, value in entry.items():
        if isinstance(value, dict) and "by" in value:
            return "", key
    looks = entry.get("looks")
    for number, look in enumerate(looks if isinstance(looks, list) else [], start=1):
        if isinstance(look, dict) and (found := _drive_in(look)) is not None:
            return f", look {number}", found[1]
    return None


def _regions(tables: object, file: str, folder: Path) -> dict[str, Kind]:
    """The regions of the `[region.NAME]` tables, each made by its `kind` (an svg's `file` is relative to `folder`, the
    recipe file's); RecipeError naming the file and region."""
    if not isinstance(tables, dict) or not all(isinstance(table, dict) for table in tables.values()):
        raise RecipeError(f"{file}: write each region as a [region.NAME] table (one table for each name)")
    regions: dict[str, Kind] = {}
    for name, table in tables.items():
        fields = dict(table)
        kind = fields.pop("kind", None)
        if kind in _REPLACED_IN_3:
            raise RecipeError(f"{file}: region {name!r}: `kind` {kind!r} is gone; {_REPLACED_IN_3[kind]} (format = 3)")
        if not isinstance(kind, str) or kind not in _KINDS:
            raise RecipeError(
                f"{file}: region {name!r}: `kind` is {kind!r}; write one of: {', '.join(_KINDS)}"
                if "kind" in table
                else f"{file}: region {name!r}: missing `kind`; write one of: {', '.join(_KINDS)}"
            )
        if kind == "svg" and isinstance(fields.get("file"), str):
            fields["file"] = str(folder / fields["file"])
        try:
            regions[name] = _KINDS[kind].parse(fields, f"region {name!r}")
        except RecipeError as error:
            raise _located(error, file) from None
    return regions


def _step(entry: dict, where: str, folder: Path) -> Step | Each:
    """The step a `[[step]]` table (or, in `looks`, an inline table) describes: an `Each` when it has `each` or
    `looks`, otherwise an effect step. RecipeError and ImageReadError as the models and pictures raise them."""
    if "each" not in entry and "looks" not in entry:
        return _effect_step(entry, where, folder)
    values = dict(entry)
    looks = values.pop("looks", None)
    if "each" not in values:
        raise RecipeError("`looks` needs `each`, the name of the partition whose pieces get them")
    if not isinstance(looks, list):
        raise RecipeError('an `each` step needs `looks`, a list of tables (one effect each) or "none"')
    made = []
    for number, look in enumerate(looks, start=1):
        if look == "none":
            made.append(look)
        elif isinstance(look, dict):
            try:
                made.append(_effect_step(look, f"look {number}", folder))
            except (RecipeError, ImageReadError, LutError) as error:
                raise _located(error, f"look {number}") from None
        else:
            raise RecipeError(f'look {number} is {look!r}: write a table with an `effect`, or "none"')
    return Each(looks=made, **values)


def _effect_step(entry: dict, where: str, folder: Path) -> Step:
    """The `Step` of a table: its step keys, and the rest as the parameters of its effect."""
    entry = dict(entry)
    keys = {key: entry.pop(key) for key in registry.STEP_KEYS if key in entry}  # the rest are the parameters
    if "effect" not in keys:
        raise RecipeError("missing `effect`")
    return Step(**keys, params=with_files(keys["effect"], entry, folder))
