"""The table of effects, the rules for getting into it, and the queries over it.

Effect modules register themselves here with `@effect(category=..., params=XParams)`; recipes, `fx`, the catalogue and
the command line look effects up here. The registry knows nothing about where effects live: the app layer imports the
effect library (`photo_fx.effects`), which registers every effect, before anything asks. Every rule is checked when an
effect registers, so a broken effect fails at import, naming itself, instead of in some recipe much later.

An effect is written `def glow(rgb, params: GlowParams, *, matte=None) -> rgb` and registered with its parameter model
(see `photo_fx.core.params`); the decorator returns a `ModelledEffect`, which validates its values through the model
whichever way it is called. TRANSITIONAL: an effect written in the older style, `fn(rgb, **params)` with every parameter
defaulted and no model, still registers (`@effect(category=...)`) until every effect module has been migrated.
"""

import functools
import inspect
import re
from collections.abc import Callable, Mapping
from dataclasses import dataclass
from typing import Any, overload

import numpy as np
from pydantic import ValidationError

from .params import EffectParams, describe_error

Effect = Callable[..., np.ndarray]

# The sections of the effect catalogue (docs/effects.md is laid out in this order). Every effect is in exactly one.
CATEGORIES = (
    "colour", "light", "texture", "stylised", "text", "distortion", "glitch", "atmosphere", "subject", "compositing",
    "backgrounds", "utilities", "animation",
)  # fmt: skip

# What a recipe hands an effect besides the image and its parameters, by name: the subject's silhouette, float32
# (h, w) 0..1. An effect declares the ones it needs as keyword-only parameters after `params`, each with a default.
INPUTS = ("matte",)

# The end of a sentence: a full stop, ! or ? followed by a space or the end. Decimals ("0.5") and ranges ("0..1") are
# never followed by a space; "e.g." and "i.e." are, so they are skipped.
_SENTENCE_END = re.compile(r"(?<!e\.g)(?<!i\.e)[.!?](?=\s|$)")


class ModelledEffect[P: EffectParams]:
    """An effect registered with its parameter model: what `@effect(category=..., params=Model)` returns.

    Call it like the function it wraps, with the values given in any of three ways, all validated by the model:
    `glow(rgb)` (the defaults), `glow(rgb, radius=30)`, or `glow(rgb, GlowParams(radius=30))`. Giving both a model and
    keyword values, or a model of another effect, is a `TypeError`; an unknown or invalid value is a `ValueError` naming
    the effect, the field, the constraint and the value. Injected inputs (`matte`) are passed by keyword in every form.
    """

    __name__: str  # the effect's name, copied from the wrapped function like its docstring and module
    params_model: type[P]  # the effect's parameter model
    category: str  # its section of the catalogue, one of `CATEGORIES`
    inputs: tuple[str, ...]  # the injected inputs it declares, from `INPUTS`

    def __init__(
        self, function: Callable[..., np.ndarray], params_model: type[P], category: str, inputs: tuple[str, ...]
    ) -> None:
        """Wrap `function(rgb, params, *, inputs...)`; the registry does this, after checking every rule."""
        functools.update_wrapper(self, function)
        self._function = function
        self.params_model, self.category, self.inputs = params_model, category, inputs
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
        """The model for `values` (parameter name -> value); ValueError naming the effect and every problem."""
        try:
            return self.params_model.model_validate(dict(values))
        except ValidationError as error:
            owner = f"effect {self.__name__!r}"
            raise ValueError(describe_error(error, owner, self.params_model.model_fields)) from None


@dataclass(frozen=True)
class _Entry:
    function: Effect  # what callers run: a ModelledEffect, or a TRANSITIONAL legacy function itself
    category: str
    model: type[EffectParams] | None  # None only for a TRANSITIONAL legacy effect
    inputs: tuple[str, ...]


_EFFECTS: dict[str, _Entry] = {}


@overload
def effect[F: Effect](fn: F, *, category: str | None = None) -> F: ...
@overload
def effect[F: Effect](fn: None = None, *, category: str | None = None, params: None = None) -> Callable[[F], F]: ...
@overload
def effect[P: EffectParams](
    fn: None = None, *, category: str | None = None, params: type[P]
) -> Callable[[Callable[[np.ndarray, P], np.ndarray]], ModelledEffect[P]]: ...
def effect(fn: Any = None, *, category: str | None = None, params: Any = None) -> Any:
    """Decorator: `@effect(category="light", params=GlowParams)` makes `glow(rgb, params, *, matte=None) -> rgb`
    available under its own name to recipes, `fx` and the command line, and returns it as a `ModelledEffect`.

    `category` must be one of `CATEGORIES`; `params` an `EffectParams` subclass, whose instance the function receives
    as its second argument. Keyword-only parameters after it are injected inputs, each named in `INPUTS` and defaulted.
    Raises `ValueError`, and registers nothing, when the category is missing or unknown, `params` is not an
    `EffectParams` subclass, the function has another shape, an input is unknown, a field is named like an input, or a
    different function already has the same name. Registering the same function again, as reloading its module does,
    replaces the old one; registering a `ModelledEffect` again keeps that very object.

    TRANSITIONAL: without `params`, `fn(rgb, **params)` registers in the older style and is returned unchanged; every
    parameter after the image needs a default and none may collect arbitrary arguments.
    """
    if fn is not None:  # used bare, as `@effect`: there is no category, so this raises and says how to give one
        return _register(fn, category, None)
    return lambda fn: _register(fn, category, params)


def unregister(name: str) -> None:
    """Remove the effect called `name` (KeyError if there is none). For tests that register throwaway effects."""
    del _EFFECTS[name]


def get(name: str) -> Effect:
    """The effect called `name`; KeyError listing the available ones if there is none."""
    return _entry(name).function


def all_effects() -> dict[str, Effect]:
    """Every registered effect by name (a copy)."""
    return {name: entry.function for name, entry in _EFFECTS.items()}


def category(name: str) -> str:
    """The category of the effect called `name` (KeyError like `get`)."""
    return _entry(name).category


def categories() -> dict[str, list[str]]:
    """Every category, in the order of `CATEGORIES`, with the sorted names of its effects (empty if it has none)."""
    return {c: sorted(name for name, entry in _EFFECTS.items() if entry.category == c) for c in CATEGORIES}


def parameters(name: str) -> type[EffectParams] | None:
    """The parameter model of the effect called `name` (KeyError like `get`); None only for a TRANSITIONAL legacy
    effect, which has none yet."""
    return _entry(name).model


def schema(name: str) -> dict[str, Any] | None:
    """The JSON schema of the effect's parameter model, with a `unit` on every property (None like `parameters`)."""
    model = parameters(name)
    return None if model is None else model.model_json_schema()


def inputs(name: str) -> tuple[str, ...]:
    """The injected inputs (from `INPUTS`) the effect called `name` declares, e.g. ("matte",) (KeyError like `get`)."""
    return _entry(name).inputs


def validate(name: str, values: Mapping[str, object]) -> EffectParams | None:
    """Check `values` (parameter name -> value) for the effect called `name`: its model, or ValueError naming the
    effect, each field, the constraint and the value. TRANSITIONAL: for a legacy effect only the names are checked
    against its signature, and the result is None."""
    entry = _entry(name)
    if isinstance(entry.function, ModelledEffect):
        return entry.function.validate(values)
    try:  # TRANSITIONAL: the legacy check, by binding the names to the function's signature
        inspect.signature(entry.function).bind(None, **values)
    except TypeError as error:
        raise ValueError(f"effect {name!r}: {error}") from None
    return None


def apply(name: str, rgb: np.ndarray, **params: Any) -> np.ndarray:
    """Run the effect called `name` with `params` (and any inputs it declares, by keyword); unknown names and invalid
    parameters raise with the valid choices."""
    return get(name)(rgb, **params)


def describe() -> dict[str, str]:
    """Effect name -> its documentation (see `documentation`), sorted by name."""
    return {name: documentation(name) for name in sorted(_EFFECTS)}


def documentation(name: str) -> str:
    """The documentation of the effect called `name`: its docstring, then a "Parameters" section with every field's
    type, unit, range, default and description (none for a TRANSITIONAL legacy effect). KeyError like `get`."""
    entry = _entry(name)
    doc = inspect.cleandoc(entry.function.__doc__ or "")
    return doc if entry.model is None else f"{doc}\n\n{entry.model.documentation()}"


def summary(name: str) -> str:
    """The first sentence of the documentation of the effect called `name`, on one line."""
    text = " ".join((get(name).__doc__ or "").split())
    end = _SENTENCE_END.search(text)
    return text[: end.end()] if end else text


def _entry(name: str) -> _Entry:
    try:
        return _EFFECTS[name]
    except KeyError:
        raise KeyError(f"unknown effect {name!r}; available: {', '.join(sorted(_EFFECTS))}") from None


def _register(fn: Any, category: str | None, params: Any) -> Any:
    name = fn.__name__
    if category not in CATEGORIES:
        given = "has no category" if category is None else f"has the unknown category {category!r}"
        raise ValueError(f"effect {name!r} {given}: write @effect(category=...) with one of: {', '.join(CATEGORIES)}")
    if isinstance(fn, ModelledEffect):  # registered again (a module reloaded, a test restoring the registry)
        if params is not None and params is not fn.params_model:
            raise ValueError(f"effect {name!r} has the model {fn.params_model.__name__}, not {params.__name__}")
        fn.category = category
        entry = _Entry(fn, category, fn.params_model, fn.inputs)
    elif params is None:  # TRANSITIONAL: the legacy keyword-argument style
        signature = inspect.signature(fn)
        _check_legacy_parameters(name, signature)
        entry = _Entry(fn, category, None, tuple(i for i in INPUTS if i in signature.parameters))
    else:
        found = _check_shape(name, fn, params)
        entry = _Entry(ModelledEffect(fn, params, category, found), category, params, found)
    existing = _EFFECTS.get(name)
    if existing is not None and _origin(existing.function) != _origin(entry.function):
        raise ValueError(
            f"two effects are called {name!r}: one in {existing.function.__module__}, another in "
            f"{entry.function.__module__}; rename one"
        )
    _EFFECTS[name] = entry
    return entry.function


def _check_shape(name: str, fn: Callable, params: Any) -> tuple[str, ...]:
    """Check a modelled effect's function and model; returns the inputs it declares."""
    if not (isinstance(params, type) and issubclass(params, EffectParams)):
        raise ValueError(f"effect {name!r}: params must be an EffectParams subclass, got {params!r}")
    shape = (
        f"write it as {name}(rgb, params, *, matte=None): the image, the model, then keyword-only inputs from {INPUTS}"
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
    clash = sorted(set(params.model_fields) & set(INPUTS))
    if clash:
        raise ValueError(f"effect {name!r}: `{clash[0]}` is supplied by the recipe, so it cannot be a parameter")
    return tuple(declared)


def _check_legacy_parameters(name: str, signature: inspect.Signature) -> None:
    """TRANSITIONAL: the rules of a legacy keyword-argument effect."""
    positional = (inspect.Parameter.POSITIONAL_ONLY, inspect.Parameter.POSITIONAL_OR_KEYWORD)
    parameters = list(signature.parameters.values())
    if not parameters or parameters[0].kind not in positional:
        raise ValueError(f"effect {name!r} must take the image as its first parameter")
    for parameter in parameters[1:]:
        if parameter.kind in (inspect.Parameter.VAR_POSITIONAL, inspect.Parameter.VAR_KEYWORD):
            # recipes and `fx` check parameter names against the signature; a catch-all would let typos through
            raise ValueError(f"effect {name!r}: `{parameter.name}` takes any arguments; name every parameter instead")
        if parameter.default is inspect.Parameter.empty:
            raise ValueError(
                f"effect {name!r}: parameter `{parameter.name}` has no default; every parameter after the image "
                f"needs one, so that {name}(image) works"
            )


def _origin(fn: Effect) -> tuple[str, str]:
    """Where a function was defined. Reloading a module makes new function objects with the same origin."""
    return fn.__module__, fn.__qualname__
