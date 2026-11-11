"""The table of effects, the rules for getting into it, and the queries over it.

Effect modules register themselves here with `@effect(category=..., params=XParams)`; recipes, `fx`, the catalogue and
the command line look effects up here. Every rule is checked when an effect registers, so a broken effect fails when its
module is imported, naming itself, instead of in some recipe much later.

Where the effects come from: the first query (`get`, `all_effects`, `describe`, `categories`, any of them) loads every
installed effect package, once. An effect package is an entry point of the group `bullterrier_image.effects` naming a
module whose import registers its effects: bullterrier-image's own effects are the core's and need no package; a
third-party package declares its own (see `bullterrier_image.plugin`). With the environment variable
`BT_IMAGE_PLUGINS=builtin` no package is loaded. A package that fails to load raises `BullterrierImageError` naming its
entry point, and the next query tries again. The registry names no effect package itself, and importing
bullterrier-image loads no effect at all.

Ordering and threads: every listing is sorted by effect name. Loading happens once, under a lock: threads that make the
first query together wait for it and all see the whole table. After that, queries only read, so rendering from several
threads at once is safe. Registering is for import time: registering or unregistering while other threads query is not
supported.

An effect is written `def bloom(rgb, params: BloomParams) -> rgb` and registered with its parameter
model (see `bullterrier_image.core.params`); the decorator returns a `ModelledEffect`, which validates its values
through the model whichever way it is called.

A parameter of the type `FieldName` names a field (a selection: `depth`, `light`, a region of the recipe). The effect
takes the field's array, float32 (h, w) 0..1, as a keyword-only argument of the parameter's name, without a default:
`def relight(rgb, params, *, field)`. A recipe's runner builds the named field and passes it; a direct call passes the
array itself (`relight(rgb, field=array)`); the parameter's value, a name, is then not used. This is how an effect gets
any map of the photo (the subject, the depth, a given map).
"""

import difflib
import functools
import inspect
import os
import re
import threading
from collections.abc import Callable, Iterator, Mapping
from contextlib import contextmanager
from contextvars import ContextVar
from dataclasses import dataclass, field
from importlib import metadata
from typing import Any

import numpy as np

from bullterrier_image._native import _core
from bullterrier_image.errors import BullterrierImageError, MapNotFoundError, UnknownEffectError

from . import looks as look_files
from . import native
from . import presets as preset_files
from .parameters import default_of, field_name_fields, lut_fields, picture_fields, signature
from .parameters import names as field_names
from .params import EffectParams, from_checked, prepare, restore, table_of
from .presets import Preset
from .values import Lut, Picture, core_table

# The categories of the effect catalogue in catalogue order, each with the line that says what belongs in it (the
# catalogue's index shows it). Every effect is in exactly one category. A dict, so no category lacks its description.
CATEGORY_DESCRIPTIONS = {
    "colour": "Grades, gradient maps, tints and hue changes: they recolour the picture and leave its shapes alone.",
    "light": "Bloom and local contrast: they add or shape the light in the picture.",
    "texture": "Grain and other surface texture laid over the picture.",
    "stylised": "Looks that redraw the picture in a printing or drawing style, such as halftone.",
    "text": "Pictures redrawn as characters, such as ASCII art.",
    "distortion": "Warps that bend, ripple or displace the picture's geometry.",
    "glitch": "Digital-error looks: displaced slices, colour splits and corrupted blocks.",
    "atmosphere": "Weather and air: fog, haze, rain and the like.",
    "subject": "Effects that work on the cut-out subject, apart from its background.",
    "compositing": "Ways of combining the picture with other layers.",
    "backgrounds": "Generated or replaced backgrounds behind the subject.",
    "utilities": "Plain helpers such as resizing, cropping and borders.",
}
CATEGORIES = tuple(CATEGORY_DESCRIPTIONS)

# What a recipe hands an effect besides the image, its parameters and its fields, by name: `partition`, the
# pieces of the region the step names with its `partition` key, int32 (h, w) numbered 0..n-1 in piece order (reading
# order, `bands` highest first); and `size_ratio`, a float: working px per design px (1.0 for a call outside a recipe),
# for an effect whose light or density is per working pixel. An effect declares the ones it needs as keyword-only
# parameters after `params`, each with a default. `place` (where a band lies on its canvas) is the core's alone: it is
# listed so that no parameter takes its name, and a Python effect cannot declare it.
INPUTS = ("partition", "size_ratio", "place")

# The keys of a recipe step besides its effect's parameters, written the same in a recipe file and in `fx`: which
# effect, where it applies, how much of it shows, how it combines with the picture below it, one of its presets, the
# partition it draws, (of a step that gives each piece of a partition its look, an `Each`) `each` and `looks`, and an
# optional `name` that addresses the step's numbers (`Recipe.parameters`).
# No effect parameter may take one of these names (nor an input's), so a key means the same in every step.
STEP_KEYS = ("effect", "mask", "opacity", "blend_mode", "preset", "partition", "each", "looks", "name")

_LISTED = 5  # how many effects an unknown-name message names at most, for each thing the name can be
# The end of a sentence: a full stop, ! or ? followed by a space or the end. Decimals ("0.5") and ranges ("0..1") are
# never followed by a space; "e.g." and "i.e." are, so they are skipped.
_SENTENCE_END = re.compile(r"(?<!e\.g)(?<!i\.e)[.!?](?=\s|$)")


def _for_the_core(array: np.ndarray) -> np.ndarray:
    """`array` as float32, C-contiguous and aligned, the layout an effect (and the Rust core) reads in place: the same
    array when it is so already, else a copy."""
    return np.require(array, dtype=np.float32, requirements=["C", "A"])


class ModelledEffect:
    """An effect registered with its parameter table: what `@effect(category=..., params=Model)` returns.

    Call it like the function it wraps, with the values by keyword, all checked by the core: `bloom(rgb)` (the
    defaults) or `bloom(rgb, radius=30)`. An unknown or invalid value is a `RecipeError` naming the effect, the field,
    the constraint and the value. Injected inputs (`partition`, `size_ratio`) are passed by keyword in every form. So is
    the array of each field parameter (`FieldName`), required: float (h, w) of the picture's size, 0..1, else a
    `MapNotFoundError` naming the parameter. The result is held within 0..1.
    """

    __name__: str  # the effect's name, copied from the wrapped function like its docstring and module
    __qualname__: str  # copied too: with the module, where the effect was defined
    table: dict[str, Any]  # its parameters, in the parameter format: the core's own, or the plugin class's
    params_model: type[EffectParams] | None  # a plugin's class (the function receives an instance), none for the core's
    category: str  # its section of the catalogue, one of `CATEGORIES`
    inputs: tuple[str, ...]  # the injected inputs it declares, from `INPUTS`
    opacity: float  # the opacity of a step of this effect that sets none, 0..1 (see `effect`)

    def __init__(
        self,
        function: Callable[..., np.ndarray],
        table: dict[str, Any],
        params_model: type[EffectParams] | None,
        category: str,
        inputs: tuple[str, ...],
        opacity: float,
    ) -> None:
        """Wrap `function(rgb, params, *, inputs...)`; the registry does this, after checking every rule."""
        functools.update_wrapper(self, function)
        self._function = function
        self.table, self.params_model, self.category, self.inputs, self.opacity = (
            table,
            params_model,
            category,
            inputs,
            opacity,
        )
        named = field_name_fields(table)
        fields = [
            p.replace(annotation=np.ndarray, default=inspect.Parameter.empty) if p.name in named else p
            for p in signature(table).parameters.values()
        ]
        given = [p for p in inspect.signature(function).parameters.values() if p.name in inputs]
        self.__signature__ = inspect.Signature(
            [inspect.Parameter("rgb", inspect.Parameter.POSITIONAL_ONLY, annotation=np.ndarray), *fields, *given],
            return_annotation=np.ndarray,
        )

    def __call__(self, rgb: np.ndarray, /, **values: Any) -> np.ndarray:
        """Run the effect on `rgb` (float32 (h, w, 3) 0..1) with keyword values, or the defaults."""
        given = {name: values.pop(name) for name in self.inputs if name in values}
        for name in field_name_fields(self.table):
            given[name] = self._field_array(name, rgb, values.pop(name, None))
        return self.run(rgb, self.validate(values), **given)

    def run(self, rgb: np.ndarray, params: Any, **given: Any) -> np.ndarray:
        """Run the effect on `rgb` with `params` as `validate` makes them (or the core has checked and scaled them) and
        the inputs and field arrays `given`."""
        # every effect gets the contract's picture, float32, C-contiguous and aligned, which the Rust core reads in
        # place: the values were checked where the photo entered (`ops.image.as_image`); only the layout is made here,
        # for the output of a step before (a view, or float64) or a direct call. `np.require` copies only when one of
        # the three is wrong, where `np.ascontiguousarray` would hand a misaligned float32 array on as it is
        # the contract's one clamp: whatever the effect returns, a picture stays within 0..1
        return np.clip(self._function(_for_the_core(rgb), params, **given), 0.0, 1.0)

    def _field_array(self, name: str, rgb: np.ndarray, array: object) -> np.ndarray:
        """The array given for the field parameter `name`, as the float32 the effect receives; MapNotFoundError unless
        it is a float array of the picture's (h, w) with finite values in 0..1 (the runner's fields always are)."""
        where = f"{self.__name__}(): `{name}` is a field"
        if not (isinstance(array, np.ndarray) and array.dtype.kind == "f"):
            raise MapNotFoundError(
                f"{where}: pass its array, float (h, w) 0..1, as {name}=array (a recipe names it; a call cannot), "
                f"got {type(array).__name__}"
            )
        if array.shape != rgb.shape[:2]:
            raise MapNotFoundError(f"{where}: its array is {array.shape} but the picture is {rgb.shape[:2]}")
        if (problem := native.map_problem(array)) is not None:
            raise MapNotFoundError(f"{where}: its array has {problem}")
        return _for_the_core(array)

    def fields_named(self, values: Mapping[str, object]) -> dict[str, str]:
        """The field each field parameter names in `values` (parameter name -> value, already validated), else its
        default: `{"field": "light"}`; empty for an effect with no field parameter."""
        defaults = {entry["name"]: default_of(entry) for entry in self.table["fields"]}
        return {name: str(values.get(name, defaults[name])) for name in field_name_fields(self.table)}

    def validate(self, values: Mapping[str, object]) -> Any:
        """The parameters for `values` (parameter name -> value) as the effect receives them: its class, or for an
        effect the core runs a dict. RecipeError naming the effect and the first problem."""
        sent, held = prepare(self.table, values)
        made = restore(self.table, native.check(self.__name__, sent), held)
        return made if self.params_model is None else from_checked(self.params_model, made)


_EFFECTS: dict[str, ModelledEffect] = {}

# The effect packages: entry points of this group, each naming a module whose import registers effects.
_GROUP = "bullterrier_image.effects"
# Set to "builtin" to load only bullterrier-image's own effect package, e.g. to get past a broken third-party one.
_PLUGINS_VARIABLE = "BT_IMAGE_PLUGINS"


@dataclass
class _Loading:
    """Whether the effect packages are loaded. `running` is set while the thread holding `lock` loads them: its own
    queries meanwhile (bullterrier-image's folder checks make some) see the table as it is, instead of loading again."""

    lock: threading.RLock = field(default_factory=threading.RLock)
    done: bool = False
    running: bool = False


_LOADING = _Loading()


def effect(
    *, category: str | None = None, params: type[EffectParams], opacity: float | None = None
) -> Callable[[Callable[..., np.ndarray]], ModelledEffect]:
    """Decorator: `@effect(category="light", params=BloomParams)` makes `bloom(rgb, params) -> rgb`
    available under its own name to recipes, `fx` and the command line, and returns it as a `ModelledEffect`.

    `params` is an `EffectParams` subclass, whose instance the function receives as its second argument.
    `category` (required, one of `CATEGORIES`) is where the effect sits in the catalogue. Keyword-only parameters after
    the model are injected inputs, each named in `INPUTS` and defaulted.
    `opacity` (0..1, default 1) is the opacity a step of this effect has when it sets none, in a recipe file, `Step`
    and `fx` alike: an effect that is meant to be mixed in partway (a shadow, rain) says so here, and a step's own
    `opacity` replaces it. `fx` signatures, `documentation` and the catalogue show it.
    Raises `ValueError`, and registers nothing, when the category is missing or unknown, `params` is not an
    `EffectParams` subclass, the function has another shape, an input is unknown, a field is named like an input or a
    step key (`STEP_KEYS`), `opacity` is not a number from 0 to 1, or a different function already has the same name.
    Registering the same function again, as reloading its module does, replaces the old one (category and opacity
    included); registering a `ModelledEffect` again keeps that very object.
    """
    return lambda fn: _register(fn, category, params, opacity)


def register_again(fn: ModelledEffect) -> ModelledEffect:
    """Register `fn`, an effect that was registered before and removed (`unregister`), again as it was. For tests that
    empty the registry and put the real effects back."""
    params = fn.params_model if fn.params_model is not None else fn.table
    return _register(fn, fn.category, params, fn.opacity)


def unregister(name: str) -> None:
    """Remove the effect called `name` (UnknownEffectError if there is none). For tests that register throwaway
    effects."""
    removed = get(name)
    del _EFFECTS[name]
    if removed.params_model is not None:
        native.unregister(name)


def get(name: str) -> ModelledEffect:
    """The effect called `name`. UnknownEffectError (a KeyError) if there is none: its message says where the name
    lives: every way the name is a preset, a built-in look (with its description), a parameter, or a phrase in an
    effect's summary (quoted whole); else it offers the close matches among effects, presets and looks. Recipes, `fx`,
    the command line and the gallery all name effects through here, so they all give this message."""
    _load_effect_packages()
    if not isinstance(name, str):  # a recipe file may write any value
        raise UnknownEffectError(f"an effect is named by a string, got {name!r}")
    try:
        return _EFFECTS[name]
    except KeyError:
        raise UnknownEffectError(_not_an_effect(name)) from None


def _not_an_effect(name: str) -> str:
    # Still Python: it reads the presets of effects registered from Python, which the core does not hold; the core's own
    # `documentation::unknown_effect` (the command line) says the same for the catalogue it holds.
    """Why `name` is no effect: everything it is, listed together (a preset of some effects, a built-in look, a
    parameter of some effects, a phrase in the summary of some effects), or else the close matches among effects,
    presets and looks."""
    homes = {effect: sorted(presets(effect)) for effect in _EFFECTS}  # effect -> its preset names
    looks = look_files.read()
    meanings = [f'a preset (fx.{effect}(preset="{name}"))' for effect, names in homes.items() if name in names]
    if name in looks:
        meanings.append(f'a look (load_recipe("{name}"), `bt-image render PHOTO {name}`): {looks[name].description}')
    owners = sorted(
        (effect for effect, fn in _EFFECTS.items() if name in field_names(fn.table)),
        key=lambda effect: (len(field_names(_EFFECTS[effect].table)), effect),  # the narrowest effect first
    )
    if owners:
        meanings.append(f"a parameter of {_some(owners)}")
    words = re.findall(r"[a-z0-9]+", name.lower())
    mentions = [effect for effect in _EFFECTS if _mentions(summary(effect), words)]
    meanings.extend(f"named in the summary of {effect}: {summary(effect)}" for effect in mentions[:_LISTED])
    if len(mentions) > _LISTED:
        meanings.append(f"named in the summary of {len(mentions) - _LISTED} more")
    if meanings:
        return f"{name!r} is {'; and '.join(meanings)}"
    names = {**dict.fromkeys(_EFFECTS, "effect"), **dict.fromkeys(looks, "look")}
    for effect, preset_names in homes.items():
        names.update((preset, f"preset of {effect}") for preset in preset_names)
    near = [f"{close} ({names[close]})" for close in difflib.get_close_matches(name, names)]
    hint = f"; did you mean: {', '.join(near)}" if near else ""
    return f"unknown effect {name!r}{hint}; available: {', '.join(sorted(_EFFECTS))}"


def _some(effects: list[str]) -> str:
    """The first `_LISTED` of `effects`, comma-separated, with how many more there are."""
    more = f" and {len(effects) - _LISTED} more" if len(effects) > _LISTED else ""
    return f"{', '.join(effects[:_LISTED])}{more}"


def _mentions(text: str, words: list[str]) -> bool:
    """Whether `text` has all of `words` in a row (case, hyphens and other punctuation between words ignored)."""
    found = [m.group().lower() for m in re.finditer(r"[A-Za-z0-9]+", text)]
    return any(found[i : i + len(words)] == words for i in range(len(found) - len(words) + 1))


def all_effects() -> dict[str, ModelledEffect]:
    """Every registered effect by name, sorted by name (a copy)."""
    _load_effect_packages()
    return {name: _EFFECTS[name] for name in sorted(_EFFECTS)}


def category(name: str) -> str:
    """The category of the effect called `name` (KeyError like `get`)."""
    return get(name).category


def categories() -> dict[str, list[str]]:
    """Every category, in the order of `CATEGORIES`, with the sorted names of its effects (empty if it has none)."""
    effects = all_effects()
    return {c: [name for name, fn in effects.items() if fn.category == c] for c in CATEGORIES}


def parameters(name: str) -> dict[str, Any]:
    """The parameter table of the effect called `name`, in the parameter format (KeyError like `get`)."""
    return get(name).table


def presets(name: str) -> dict[str, Preset]:
    """The presets of the effect called `name` by name (empty if it has none; KeyError like `get`)."""
    get(name)  # an unknown name is refused here, not read as an effect without presets
    return preset_files.read(name)


def opacity(name: str) -> float:
    """The opacity a step of the effect called `name` has when it sets none (declared with `@effect(opacity=...)`;
    usually 1). KeyError like `get`."""
    return get(name).opacity


def inputs(name: str) -> tuple[str, ...]:
    """The injected inputs (from `INPUTS`) the effect called `name` declares, e.g. ("partition",) (KeyError like
    `get`)."""
    return get(name).inputs


def apply(name: str, rgb: np.ndarray, **params: Any) -> np.ndarray:
    """Run the effect called `name` with `params` (and any inputs it declares, by keyword); unknown names and invalid
    parameters raise with the valid choices."""
    return get(name)(rgb, **params)


def describe() -> dict[str, str]:
    """Effect name -> its documentation (see `documentation`), sorted by name."""
    _load_effect_packages()
    return {name: documentation(name) for name in sorted(_EFFECTS)}


def documentation(name: str, *, advanced: bool = True) -> str:
    """The documentation of the effect called `name`: its docstring, then a "Parameters" section with every basic
    field's type, unit, range, default and description, then an "Advanced parameters" section (or, with
    `advanced=False`, one line counting them), then a "Step opacity" section if a step of it has an opacity other than
    1 by default, then a "Presets" section if it has any. KeyError like `get`. The core writes it for an effect it
    declares; an effect registered from Python is written here, the same way."""
    get(name)
    return native.documentation(name, advanced=advanced)


def summary(name: str) -> str:
    """The first sentence of the documentation of the effect called `name`, on one line."""
    get(name)
    return native.summary(name)


def _load_effect_packages() -> None:
    """Import every effect package, the first time any query asks; see the module documentation."""
    if _LOADING.done:
        return
    with _LOADING.lock:
        if _LOADING.done or _LOADING.running:  # another thread loaded them meanwhile, or this one is loading them
            return
        _LOADING.running = True
        try:
            for entry_point in _effect_packages():
                _load(entry_point)
            _register_core_effects()
            _LOADING.done = True
        finally:
            _LOADING.running = False


def _load(entry_point: metadata.EntryPoint) -> None:
    """Import the effect package of `entry_point`. A package's import can fail in any way (a missing module, a broken
    effect, a duplicate name); each is reported the same way, naming the package, with the original error chained."""
    try:
        entry_point.load()
    except Exception as error:
        raise BullterrierImageError(_failure(entry_point, error)) from error


def load_plugins() -> None:
    """Import every effect package, ours included, so that the effects written in Python are known to the core: the
    command line runs in Rust and parses its recipes before any query here would have loaded them. The effects the
    core declares need no import."""
    for entry_point in _effect_packages():
        _load(entry_point)


def _register_core_effects() -> None:
    """Register each effect the Rust core declares that no Python package registered: the declaration (parameters,
    documentation, category, opacity) is the whole effect, so it has no Python folder."""
    for name in native.effects(declared=True):
        if name in _EFFECTS:
            continue
        table = native.params(name)
        run = _core_runner(name, table)
        run.__doc__ = table["doc"]
        _register(run, None, table, None)


def _core_runner(name: str, table: dict[str, Any]) -> Callable[..., np.ndarray]:
    """The function of the core effect `name`: it hands the validated parameters to the core, a `Lut` parameter as
    the core's table, which the effect finds in its inputs under the parameter's name (the value is then none), and the
    array of each field parameter in `fields` (keyword-only, as the registry asks of every effect) the same way. An
    effect that declares `size_ratio` takes it as a keyword, 1 outside a recipe; one that declares `partition` takes the
    piece labels (int32 (h, w)) as a keyword, which the core refuses to run without."""
    fields, luts_of, pictures_of = field_name_fields(table), lut_fields(table), picture_fields(table)
    declared = native.effect(name)["inputs"]
    sized, partitioned = "size_ratio" in declared, "partition" in declared

    def run(
        rgb: np.ndarray,
        values: dict[str, Any],
        *,
        size_ratio: float = 1.0,
        partition: np.ndarray | None = None,
        **arrays: np.ndarray,
    ) -> np.ndarray:
        luts = {field: core_table(values[field]) for field in luts_of}
        pictures = {field: values[field].pixels for field in pictures_of if values[field] is not None}
        return _core.apply(
            name,
            rgb,
            values | dict.fromkeys(luts) | dict.fromkeys(pictures_of),
            size_ratio=size_ratio,
            partition=partition,
            luts=luts,
            pictures=pictures,
            fields=arrays,
        )

    run.__signature__ = inspect.Signature(  # type: ignore[attr-defined]  # what `_check_shape` reads, in place of `**arrays`
        [
            inspect.Parameter("rgb", inspect.Parameter.POSITIONAL_OR_KEYWORD),
            inspect.Parameter("values", inspect.Parameter.POSITIONAL_OR_KEYWORD),
            *(inspect.Parameter(field, inspect.Parameter.KEYWORD_ONLY) for field in fields),
            *([inspect.Parameter("size_ratio", inspect.Parameter.KEYWORD_ONLY, default=1.0)] if sized else []),
            *([inspect.Parameter("partition", inspect.Parameter.KEYWORD_ONLY, default=None)] if partitioned else []),
        ]
    )
    run.__name__ = run.__qualname__ = name
    return run


def _effect_packages() -> list[metadata.EntryPoint]:
    """The entry points to load: our own first, then the others by name (unless BT_IMAGE_PLUGINS=builtin)."""
    setting = os.environ.get(_PLUGINS_VARIABLE, "")
    if setting not in {"", "builtin"}:
        raise BullterrierImageError(
            f"{_PLUGINS_VARIABLE}={setting!r} is not a setting: leave it unset to load every installed effect package, "
            f"or set it to 'builtin' to load only bullterrier-image's own effects"
        )
    found = sorted(metadata.entry_points(group=_GROUP), key=lambda entry_point: entry_point.name)
    own = [entry_point for entry_point in found if _is_bullterrier_image(entry_point)]
    others = [entry_point for entry_point in found if not _is_bullterrier_image(entry_point)]
    return own if setting == "builtin" else own + others


def _is_bullterrier_image(entry_point: metadata.EntryPoint) -> bool:
    return entry_point.module.partition(".")[0] == __name__.partition(".")[0]


def _failure(entry_point: metadata.EntryPoint, error: Exception) -> str:
    """What to say when the effect package of `entry_point` failed to load with `error`."""
    origin = f"distribution {entry_point.dist.name!r}, " if entry_point.dist is not None else ""
    message = (
        f"the effect package {entry_point.name!r} ({origin}entry point {entry_point.value!r} in the group {_GROUP!r}) "
        f"failed to load: {type(error).__name__}: {error}"
    )
    if _is_bullterrier_image(entry_point):
        return message
    return f"{message}; fix or uninstall it, or set {_PLUGINS_VARIABLE}=builtin to load only our own effects"


def _register(fn: Any, category: str | None, params: Any, opacity: Any) -> ModelledEffect:
    """Register `fn` under its name. `params` is a plugin's `EffectParams` class, or the table of an effect the Rust
    core runs (its category and opacity are then the core's)."""
    name = fn.__name__
    core = isinstance(params, dict)
    if core:
        category, opacity = native.effect(name)["category"], native.effect(name)["opacity"]
        table, model = params, None
    elif category is None:
        raise ValueError(
            f"effect {name!r} needs a category: write @effect(category=...) with one of: {', '.join(CATEGORIES)}"
        )
    elif opacity is None:
        opacity = 1.0
    if not core:
        if not (isinstance(params, type) and issubclass(params, EffectParams)):
            raise ValueError(f"effect {name!r}: params must be an EffectParams subclass, got {params!r}")
        # the function's docstring documents the effect; the class's, its parameters as a group
        table, model = {**table_of(params), "doc": inspect.cleandoc(fn.__doc__ or "")}, params
    if category not in CATEGORIES:
        raise ValueError(
            f"effect {name!r} has the unknown category {category!r}: write @effect(category=...) with one of: "
            f"{', '.join(CATEGORIES)}"
        )
    if type(opacity) not in {int, float} or not 0 <= opacity <= 1:  # not a bool; NaN fails the comparison
        raise ValueError(
            f"effect {name!r}: opacity, the default opacity of its steps, must be a number from 0 to 1, got {opacity!r}"
        )
    if isinstance(fn, ModelledEffect):  # registered again (a module reloaded, a test restoring the registry)
        if model is not fn.params_model:
            raise ValueError(
                f"effect {name!r} has the model {getattr(fn.params_model, '__name__', None)}, not "
                f"{getattr(params, '__name__', params)}"
            )
        fn.category, fn.opacity = category, opacity
        registered = fn
    else:
        registered = ModelledEffect(fn, table, model, category, _check_shape(name, fn, table), opacity)
    existing = _EFFECTS.get(name)
    if existing is not None and _origin(existing) != _origin(registered):
        raise ValueError(
            f"two effects are called {name!r}: one in {existing.__module__}, another in {registered.__module__}; "
            "rename one"
        )
    if model is not None:
        _mirror(registered, model)
    _EFFECTS[name] = registered
    return registered


def _mirror(effect: "ModelledEffect", model: type[EffectParams]) -> None:
    """Make `effect` known to the Rust core, so that a recipe the core runs can use it; replaces the core's copy of an
    earlier registration of the name. The core calls `run` with the parameters as a step wrote them (a Python effect
    describes none to the core) and `size_ratio`, always declared: the wrapper validates and scales them, passes the
    ratio on only to an effect that declares it, and refuses a result of another shape. An effect the core runs itself
    is not mirrored (`_register`): the core has it."""
    name = effect.__name__

    def run(rgb: np.ndarray, params: dict[str, Any], *, size_ratio: float, **inputs: Any) -> np.ndarray:
        values = from_checked(model, _resolved(params))  # the core has checked and scaled them
        if "size_ratio" in effect.inputs:
            inputs["size_ratio"] = size_ratio
        result = effect.run(rgb, values, **inputs)
        if result.shape != rgb.shape:
            raise RuntimeError(f"effect {name!r} changed the image shape {rgb.shape} -> {result.shape}")
        return result

    if name in native.effects():
        native.unregister(name)
    declared = [name for name in INPUTS if name == "size_ratio" or name in effect.inputs]
    native.register(
        name,
        effect.category,
        effect.opacity,
        run,
        inputs=declared,
        fields=effect.fields_named({}),
        table=effect.table,
    )


# The only form any other object can take in the dict a recipe is run from: a reference to an object the call holds. A
# Lut and a Picture are written as the name the core finds them under (`Held.luts`, `Held.pictures_held`).
_REFERENCE = "$object"


class Held:
    """What a call that runs a recipe in the core keeps (`holding`): the parameters that cannot be written as data.

    `hold(obj)` keeps `obj` until the block ends and returns what to put in the recipe's dict in its place: for a
    `Picture` its name, `$picture:` and its digest, for a `Lut` its name, `$lut:` and its digest (one is held once,
    however many steps use it), for anything else a reference.
    `tables()` gives the core's table of each held `Lut` by name, and `pictures()` the RGBA pixels of each held
    `Picture`, to hand to the render. A Python effect the core runs receives the objects back where the references and
    names were.
    """

    def __init__(self) -> None:
        self.objects: list[object] = []
        self.luts: dict[str, Lut] = {}
        self.pictures_held: dict[str, Picture] = {}

    def __call__(self, obj: object) -> object:
        """What the recipe's dict has in place of `obj`, which is kept."""
        if isinstance(obj, Lut):
            name = f"$lut:{obj.digest}"
            self.luts[name] = obj
            return name
        if isinstance(obj, Picture):
            name = f"$picture:{obj.digest}"
            self.pictures_held[name] = obj
            return name
        self.objects.append(obj)
        return {_REFERENCE: len(self.objects) - 1}

    def tables(self) -> dict[str, _core.Lut]:
        """The core's table of each held `Lut`, by the name the recipe writes."""
        return {name: core_table(lut) for name, lut in self.luts.items()}

    def pictures(self) -> dict[str, np.ndarray]:
        """The pixels (float32 RGBA) of each held `Picture`, by the name the recipe writes."""
        return {name: picture.pixels for name, picture in self.pictures_held.items()}


_HELD: ContextVar[Held | None] = ContextVar("held", default=None)


@contextmanager
def holding() -> Iterator[Held]:
    """For a call that runs a recipe in the core: the `Held` that keeps a parameter that cannot be written as data (a
    Picture, a Lut) until the block ends; `hold(obj)` returns what the recipe's dict has in its place."""
    held = Held()
    token = _HELD.set(held)
    try:
        yield held
    finally:
        _HELD.reset(token)


def _resolved(value: Any) -> Any:
    """`value`, a parameter as the core hands it over, with the objects the call holds back in place of references."""
    held = _HELD.get()
    if held is None:
        return value
    if isinstance(value, str):
        return held.luts.get(value) or held.pictures_held.get(value) or value
    if isinstance(value, dict):
        if value.keys() == {_REFERENCE}:
            return held.objects[value[_REFERENCE]]
        return {key: _resolved(item) for key, item in value.items()}
    if isinstance(value, list):
        return [_resolved(item) for item in value]
    return value


def _check_shape(name: str, fn: Callable, table: dict[str, Any]) -> tuple[str, ...]:
    """Check a modelled effect's function against its table; returns the inputs it declares."""
    shape = (
        f"write it as {name}(rgb, params, *, size_ratio=1.0): the image, the model, then keyword-only inputs from "
        f"{INPUTS} and fields"
    )
    every = list(inspect.signature(fn).parameters.values())
    positional = [p for p in every if p.kind in (p.POSITIONAL_ONLY, p.POSITIONAL_OR_KEYWORD)]
    if len(positional) != 2 or any(p.kind in (p.VAR_POSITIONAL, p.VAR_KEYWORD) for p in every):
        raise ValueError(f"effect {name!r}: {shape}")
    fields = field_name_fields(table)
    declared, taken = [], []
    for parameter in (p for p in every if p.kind is p.KEYWORD_ONLY):
        if parameter.name in fields:
            if parameter.default is not inspect.Parameter.empty:
                raise ValueError(
                    f"effect {name!r}: the field `{parameter.name}` takes no default: a recipe or a caller gives it"
                )
            taken.append(parameter.name)
            continue
        if parameter.name not in INPUTS:
            raise ValueError(f"effect {name!r}: {parameter.name!r} is not an input ({', '.join(INPUTS)}); {shape}")
        if parameter.default is inspect.Parameter.empty:
            raise ValueError(
                f"effect {name!r}: the input `{parameter.name}` needs a default, so that {name}(rgb) works"
            )
        declared.append(parameter.name)
    if unused := [field for field in fields if field not in taken]:
        raise ValueError(
            f"effect {name!r}: the field parameter `{unused[0]}` names a field the effect must receive: add the "
            f"keyword-only argument `{unused[0]}` after `params`"
        )
    clash = sorted(set(field_names(table)) & {*INPUTS, *STEP_KEYS})
    if clash:
        raise ValueError(
            f"effect {name!r}: `{clash[0]}` is supplied by the recipe (an input or a key of the step), so it cannot be "
            "a parameter"
        )
    return tuple(declared)


def _origin(fn: ModelledEffect) -> tuple[str, str]:
    """Where an effect was defined. Reloading a module makes new function objects with the same origin."""
    return fn.__module__, fn.__qualname__
