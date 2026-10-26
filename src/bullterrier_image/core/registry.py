"""The table of effects, the rules for getting into it, and the queries over it.

Effect modules register themselves here with `@effect(category=..., params=XParams)`; recipes, `fx`, the catalogue and
the command line look effects up here. Every rule is checked when an effect registers, so a broken effect fails when its
module is imported, naming itself, instead of in some recipe much later.

Where the effects come from: the first query (`get`, `all_effects`, `describe`, `categories`, any of them) loads every
installed effect package, once. An effect package is an entry point of the group `bullterrier_image.effects` naming a
module whose import registers its effects: bullterrier-image's own effects are one (`bullterrier_image.effects`,
declared in bullterrier-image's `pyproject.toml`) and are loaded first; a third-party package declares its own (see
`bullterrier_image.plugin`). With the environment variable `BT_IMAGE_PLUGINS=builtin` only bullterrier-image's own are
loaded. A package that fails to load raises `BullterrierImageError` naming its entry point, and the next query tries
again. The registry names no effect package itself, so `core` never imports `effects`, and importing bullterrier-image
loads no effect at all.

Ordering and threads: every listing is sorted by effect name. Loading happens once, under a lock: threads that make the
first query together wait for it and all see the whole table. After that, queries only read, so rendering from several
threads at once is safe. Registering is for import time: registering or unregistering while other threads query is not
supported.

An effect is written `def glow(rgb, params: GlowParams, *, subject=None) -> rgb` and registered with its parameter model
(see `bullterrier_image.core.params`); the decorator returns a `ModelledEffect`, which validates its values through the
model whichever way it is called.
"""

import functools
import inspect
import os
import re
import threading
from collections.abc import Callable, Mapping
from dataclasses import dataclass, field
from importlib import metadata
from typing import Any

import numpy as np

from bullterrier_image.errors import BullterrierImageError, UnknownEffectError

from . import presets as preset_files
from .params import EffectParams
from .presets import Preset

# The categories of the effect catalogue in catalogue order, each with the line that says what belongs in it (the
# catalogue's index shows it). Every effect is in exactly one category. A dict, so no category lacks its description.
CATEGORY_DESCRIPTIONS = {
    "colour": "Grades, gradient maps, tints and hue changes: they recolour the picture and leave its shapes alone.",
    "light": "Glow, bloom and local contrast: they add or shape the light in the picture.",
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

# The maps of a photo an effect can take, by name: `subject`, the subject's silhouette, float32 (h, w) 0..1; `depth`,
# how near each pixel is, float32 (h, w) 0..1, 1 = nearest. A map has the same name wherever it is handed in (`maps=`,
# `finders=`, `--map`), named by a region (`mask = "subject"`) and taken by an effect.
MAP_INPUTS = ("subject", "depth")

# What a recipe hands an effect besides the image and its parameters, by name: the maps above and `partition`, the
# pieces of the region the step names with its `partition` key, int32 (h, w) numbered 0..n-1 in piece order (reading
# order, `depth_bands` nearest first). An effect declares the ones it needs as keyword-only parameters after `params`,
# each with a default.
INPUTS = (*MAP_INPUTS, "partition")

# The keys of a recipe step besides its effect's parameters, written the same in a recipe file and in `fx`: which
# effect, where it applies, how much of it shows, how it combines with the picture below it, one of its presets, the
# partition it draws, and (of a step that gives each piece of a partition its look, an `Each`) `each` and `looks`.
# No effect parameter may take one of these names (nor an input's), so a key means the same in every step.
STEP_KEYS = ("effect", "mask", "opacity", "blend_mode", "preset", "partition", "each", "looks")

# The end of a sentence: a full stop, ! or ? followed by a space or the end. Decimals ("0.5") and ranges ("0..1") are
# never followed by a space; "e.g." and "i.e." are, so they are skipped.
_SENTENCE_END = re.compile(r"(?<!e\.g)(?<!i\.e)[.!?](?=\s|$)")


class ModelledEffect[P: EffectParams]:
    """An effect registered with its parameter model: what `@effect(category=..., params=Model)` returns.

    Call it like the function it wraps, with the values given in any of three ways, all validated by the model:
    `glow(rgb)` (the defaults), `glow(rgb, radius=30)`, or `glow(rgb, GlowParams(radius=30))`. Giving both a model and
    keyword values, or a model of another effect, is a `TypeError`; an unknown or invalid value is a `RecipeError`
    naming the effect, the field, the constraint and the value. Injected inputs (`subject`, `depth`) are passed by
    keyword in every form.
    """

    __name__: str  # the effect's name, copied from the wrapped function like its docstring and module
    __qualname__: str  # copied too: with the module, where the effect was defined
    params_model: type[P]  # the effect's parameter model
    category: str  # its section of the catalogue, one of `CATEGORIES`
    inputs: tuple[str, ...]  # the injected inputs it declares, from `INPUTS`
    opacity: float  # the opacity of a step of this effect that sets none, 0..1 (see `effect`)

    def __init__(
        self,
        function: Callable[..., np.ndarray],
        params_model: type[P],
        category: str,
        inputs: tuple[str, ...],
        opacity: float,
    ) -> None:
        """Wrap `function(rgb, params, *, inputs...)`; the registry does this, after checking every rule."""
        functools.update_wrapper(self, function)
        self._function = function
        self.params_model, self.category, self.inputs, self.opacity = params_model, category, inputs, opacity
        fields = [p.replace(kind=inspect.Parameter.KEYWORD_ONLY) for p in params_model.signature().parameters.values()]
        given = [p for p in inspect.signature(function).parameters.values() if p.name in inputs]
        self.__signature__ = inspect.Signature(
            [
                inspect.Parameter("rgb", inspect.Parameter.POSITIONAL_ONLY, annotation=np.ndarray),
                inspect.Parameter("params", inspect.Parameter.POSITIONAL_ONLY, default=None, annotation=params_model),
                *fields,
                *given,
            ],
            return_annotation=np.ndarray,
        )

    def __call__(self, rgb: np.ndarray, params: P | None = None, /, **values: Any) -> np.ndarray:
        """Run the effect on `rgb` (float32 (h, w, 3) 0..1) with a model, keyword values, or the defaults."""
        given = {name: values.pop(name) for name in self.inputs if name in values}
        if params is None:
            params = self.validate(values)
        elif values:
            raise TypeError(f"{self.__name__}(): give a {self.params_model.__name__} or keyword values, not both")
        elif not isinstance(params, self.params_model):
            raise TypeError(f"{self.__name__}() takes a {self.params_model.__name__}, not a {type(params).__name__}")
        return self._function(rgb, params, **given)

    def validate(self, values: Mapping[str, object]) -> P:
        """The model for `values` (parameter name -> value); RecipeError naming the effect and every problem."""
        return self.params_model.parse(values, f"effect {self.__name__!r}")


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


def effect[P: EffectParams](
    *, category: str, params: type[P], opacity: float = 1.0
) -> Callable[[Callable[..., np.ndarray]], ModelledEffect[P]]:
    """Decorator: `@effect(category="light", params=GlowParams)` makes `glow(rgb, params, *, subject=None) -> rgb`
    available under its own name to recipes, `fx` and the command line, and returns it as a `ModelledEffect`.

    `category` must be one of `CATEGORIES`; `params` an `EffectParams` subclass, whose instance the function receives
    as its second argument. Keyword-only parameters after it are injected inputs, each named in `INPUTS` and defaulted.
    `opacity` (0..1, default 1) is the opacity a step of this effect has when it sets none, in a recipe file, `Step`
    and `fx` alike: an effect that is meant to be mixed in partway (a shadow, rain) says so here, and a step's own
    `opacity` replaces it. `fx` signatures, `documentation` and the catalogue show it.
    Raises `ValueError`, and registers nothing, when the category is unknown, `params` is not an `EffectParams`
    subclass, the function has another shape, an input is unknown, a field is named like an input or a step key
    (`STEP_KEYS`), `opacity` is not a number from 0 to 1, or a different function already has the same name.
    Registering the same function again, as reloading its module does, replaces the old one (category and opacity
    included); registering a `ModelledEffect` again keeps that very object.
    """
    return lambda fn: _register(fn, category, params, opacity)


def unregister(name: str) -> None:
    """Remove the effect called `name` (UnknownEffectError if there is none). For tests that register throwaway
    effects."""
    get(name)
    del _EFFECTS[name]


def get(name: str) -> ModelledEffect:
    """The effect called `name`; UnknownEffectError (a KeyError) listing the available ones if there is none."""
    _load_effect_packages()
    try:
        return _EFFECTS[name]
    except KeyError:
        raise UnknownEffectError(f"unknown effect {name!r}; available: {', '.join(sorted(_EFFECTS))}") from None


def all_effects() -> dict[str, ModelledEffect]:
    """Every registered effect by name, sorted by name (a copy)."""
    _load_effect_packages()
    return {name: _EFFECTS[name] for name in sorted(_EFFECTS)}


def category(name: str) -> str:
    """The category of the effect called `name` (KeyError like `get`)."""
    return get(name).category


def categories() -> dict[str, list[str]]:
    """Every category, in the order of `CATEGORIES`, with the sorted names of its effects (empty if it has none)."""
    _load_effect_packages()
    return {c: sorted(name for name, fn in _EFFECTS.items() if fn.category == c) for c in CATEGORIES}


def parameters(name: str) -> type[EffectParams]:
    """The parameter model of the effect called `name` (KeyError like `get`)."""
    return get(name).params_model


def presets(name: str) -> dict[str, Preset]:
    """The presets of the effect called `name` by name (empty if it has none; KeyError like `get`)."""
    return preset_files.read(name, parameters(name))


def opacity(name: str) -> float:
    """The opacity a step of the effect called `name` has when it sets none (declared with `@effect(opacity=...)`;
    usually 1). KeyError like `get`."""
    return get(name).opacity


def inputs(name: str) -> tuple[str, ...]:
    """The injected inputs (from `INPUTS`) the effect called `name` declares, e.g. ("subject",) (KeyError like
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
    1 by default, then a "Presets" section if it has any. KeyError like `get`."""
    fn = get(name)
    text = f"{inspect.cleandoc(fn.__doc__ or '')}\n\n{fn.params_model.documentation(advanced)}"
    if fn.opacity != 1:
        text += (
            f"\n\nStep opacity\n------------\n{fn.opacity:g}: a step of this effect is mixed in at {fn.opacity:g} "
            "unless it sets its own `opacity`, which replaces this."
        )
    if not (found := presets(name)):
        return text
    lines = [f"{preset} : {found[preset].description}" for preset in found]
    return f"{text}\n\nPresets\n-------\n" + "\n".join(lines)


def summary(name: str) -> str:
    """The first sentence of the documentation of the effect called `name`, on one line."""
    text = " ".join((get(name).__doc__ or "").split())
    end = _SENTENCE_END.search(text)
    return text[: end.end()] if end else text


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
                try:
                    entry_point.load()
                # A package's import can fail in any way (a missing module, a broken effect, a duplicate name); each is
                # reported the same way, naming the package, with the original error chained.
                except Exception as error:
                    raise BullterrierImageError(_failure(entry_point, error)) from error
            _LOADING.done = True
        finally:
            _LOADING.running = False


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


def _register(fn: Any, category: str, params: Any, opacity: Any) -> ModelledEffect:
    name = fn.__name__
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
        if params is not fn.params_model:
            raise ValueError(
                f"effect {name!r} has the model {fn.params_model.__name__}, not {getattr(params, '__name__', params)}"
            )
        fn.category, fn.opacity = category, opacity
        registered = fn
    else:
        registered = ModelledEffect(fn, params, category, _check_shape(name, fn, params), opacity)
    existing = _EFFECTS.get(name)
    if existing is not None and _origin(existing) != _origin(registered):
        raise ValueError(
            f"two effects are called {name!r}: one in {existing.__module__}, another in {registered.__module__}; "
            "rename one"
        )
    _EFFECTS[name] = registered
    return registered


def _check_shape(name: str, fn: Callable, params: Any) -> tuple[str, ...]:
    """Check a modelled effect's function and model; returns the inputs it declares."""
    if not (isinstance(params, type) and issubclass(params, EffectParams)):
        raise ValueError(f"effect {name!r}: params must be an EffectParams subclass, got {params!r}")
    shape = (
        f"write it as {name}(rgb, params, *, subject=None): the image, the model, then keyword-only inputs from "
        f"{INPUTS}"
    )
    every = list(inspect.signature(fn).parameters.values())
    positional = [p for p in every if p.kind in (p.POSITIONAL_ONLY, p.POSITIONAL_OR_KEYWORD)]
    if len(positional) != 2 or any(p.kind in (p.VAR_POSITIONAL, p.VAR_KEYWORD) for p in every):
        raise ValueError(f"effect {name!r}: {shape}")
    declared = []
    for parameter in (p for p in every if p.kind is p.KEYWORD_ONLY):
        if parameter.name not in INPUTS:
            raise ValueError(f"effect {name!r}: {parameter.name!r} is not an input ({', '.join(INPUTS)}); {shape}")
        if parameter.default is inspect.Parameter.empty:
            raise ValueError(
                f"effect {name!r}: the input `{parameter.name}` needs a default (None), so that {name}(rgb) works"
            )
        declared.append(parameter.name)
    clash = sorted(set(params.model_fields) & {*INPUTS, *STEP_KEYS})
    if clash:
        raise ValueError(
            f"effect {name!r}: `{clash[0]}` is supplied by the recipe (an input or a key of the step), so it cannot be "
            "a parameter"
        )
    return tuple(declared)


def _origin(fn: ModelledEffect) -> tuple[str, str]:
    """Where an effect was defined. Reloading a module makes new function objects with the same origin."""
    return fn.__module__, fn.__qualname__
