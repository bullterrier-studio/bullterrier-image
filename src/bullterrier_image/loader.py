"""Reading a recipe from a small TOML file: `load_recipe(path)` returns a validated `Recipe`.

Promises: every error is a `RecipeError` that starts with the file's name, then names the step and the offending key
(a TOML syntax error keeps its line and column); the files read are that one and the pictures and lookup tables
it names. The format (UTF-8 TOML):

    format = 3                     # optional: the recipe format version; this release reads 1 (the default), 2 and 3
    name = "green terminal"        # optional, free text (default: the file name without `.toml`)
    description = "Green text."    # optional, one sentence; a built-in look must have one (see "Looks" below)
    size = 1024                    # optional: longest side in px of the result; absent = the photo's own. The
                                   # recipe's px values are always written for 480, whatever the size

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
(`style`, `amount` and `scale` in px, `seed`) that tears the borders. A *partition* (a grid, shards, cuts, svg or
bands) cuts the picture into numbered pieces, a *selection* (a `path`, a `pick`, a `subject`, a `band`, a `combine`, a
gradient, a `given` map or the built-in `subject`, `background`, `depth` and `light`) chooses pixels; a `subject` with
`per` finds the subject inside each piece of a partition. `band`, `bands` and `combine` are made of another selection
named by `of`, read for its values: the built-in `depth` (1 = nearest), `light` (the picture's brightness) or any other
(bands are numbered highest first). Steps name regions: `mask` (a selection) limits a step to it, and `partition` names
the partition an effect such as `seams` draws. A step with `each` (a partition) and `looks` gives every piece its own
look:

    [[step]]
    each = "panes"                 # a partition
    looks = [{ effect = "halftone", cell = 9 }, "none"]   # each look is a step table, or "none": the piece stays
    assign = "cycle"               # or "random" (with `seed`): how the pieces get looks
    feather = 0.5                  # soft borders between pieces, in px
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
not know `bridge`.
"""

from collections.abc import Mapping
from os import PathLike
from pathlib import Path

from bullterrier_image.errors import BullterrierImageError, ImageReadError, LutError, RecipeError

from . import imageio
from .core import native, registry
from .core.framing import Framing
from .core.params import arrays_as_values, lut_fields, picture_fields
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
from .core.steps import Each, Recipe, Step
from .core.values import Picture

__all__ = ["load_recipe", "with_files"]

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
    for key in picture_fields(model) & values.keys():
        value = values[key]
        if isinstance(value, PathLike) or (isinstance(value, str) and not value.startswith("#")):
            try:
                resolved[key] = Picture(imageio.load_picture(folder / value), str(value))
            except ImageReadError as error:
                raise ImageReadError(f"effect {effect!r}: `{key}`: {error}") from None
    for key in lut_fields(model) & values.keys():
        value = values[key]
        if isinstance(value, PathLike | str):
            try:
                resolved[key] = imageio.load_lut(folder / value)
            except LutError as error:
                raise LutError(f"effect {effect!r}: `{key}`: {error}") from None
    return arrays_as_values(model, resolved)


def load_recipe(path: str | Path) -> Recipe:
    """Read a recipe file, or a built-in look by name, and return the validated `Recipe`.

    path: a TOML file in the format above, or a `str` that is exactly the name of a built-in look ("crt"). The recipe
    takes the file's `name`, else the file's name without `.toml` ("my look.toml" gives "my look"), and its
    `description`. The core reads the file and finds the files it names (`Recipe::from_file`); this builds the model.
    Raises RecipeError (a ValueError; an unknown effect is an UnknownEffectError) for anything invalid, starting with
    the file's name, then the step and the offending key; OSError (FileNotFoundError...) if the file cannot be opened.
    """
    if not isinstance(path, str):
        path.stat()  # a file that is not there is an OSError, as opening it would be
    read = native.read_recipe_source(str(path), not isinstance(path, str))
    named = Path(path).name if not isinstance(path, str) or Path(path).exists() else f"{path}.toml"
    folder = Path(path).parent if not isinstance(path, str) or Path(path).exists() else Path()
    regions: dict[str, Kind] = {}
    for name, table in read.get("regions", {}).items():
        fields = dict(table)
        kind = fields.pop("kind")
        try:
            regions[name] = _KINDS[kind].parse(fields, f"region {name!r}")
        except RecipeError as error:
            raise _located(error, named) from None
    try:
        frame = Framing(**read.get("frame", {}))
    except RecipeError as error:
        raise _located(error, f"{named}: [frame]") from None
    steps = []
    for i, entry in enumerate(read.get("steps", []), start=1):
        where = f"{named}: step {i}"
        try:
            steps.append(_step(entry, folder))
        except (RecipeError, ImageReadError, LutError) as error:
            raise _located(error, where) from None
    try:
        return Recipe(
            steps,
            name=read.get("name"),
            description=read.get("description"),
            size=read.get("size"),
            frame=frame,
            regions=regions,
        )
    except RecipeError as error:
        raise _located(error, named) from None


def _step(entry: dict, folder: Path) -> Step | Each:
    """The step the core read from a `[[step]]` table (or, in `looks`, an inline table): an `Each` when it has `each`,
    otherwise an effect step. RecipeError and ImageReadError as the models and pictures raise them."""
    if "each" not in entry:
        return _effect_step(entry, folder)
    looks_made: list[Step | None] = []
    for number, look in enumerate(entry["looks"], start=1):
        if look is None:  # a file's word "none": a piece left as it is
            looks_made.append(None)
            continue
        try:
            looks_made.append(_effect_step(look, folder))
        except (RecipeError, ImageReadError, LutError) as error:
            raise _located(error, f"look {number}") from None
    keys = ("each", "assign", "seed", "feather", "mask", "opacity", "blend_mode", "name")
    return Each(looks=looks_made, **_set(entry, *keys))


def _set(entry: dict, *keys: str) -> dict:
    """The `keys` of `entry` the file wrote (the core leaves the others `None`)."""
    return {key: entry[key] for key in keys if entry.get(key) is not None}


def _effect_step(entry: dict, folder: Path) -> Step:
    """The `Step` of a step the core read, its picture and lookup table paths read."""
    # the core made the files an effect it runs names into paths that stand on their own; those of an effect written in
    # Python are as written, so they are found beside the recipe here
    beside = Path() if registry.parameters(entry["effect"])._core_effect is not None else folder  # noqa: SLF001
    return Step(
        entry["effect"],
        params=with_files(entry["effect"], entry.get("params", {}), beside),
        **_set(entry, "mask", "opacity", "blend_mode", "preset", "partition", "name"),
    )
