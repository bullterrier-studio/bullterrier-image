"""The table of effects, the rules for getting into it, and the queries over it.

Effect modules register themselves here with `@effect(category=...)`; recipes, `fx`, the catalogue and the command line
look effects up here. The registry knows nothing about where effects live: the app layer imports the effect library
(`photo_fx.effects`), which registers every effect, before anything asks. Every rule is checked when an effect
registers, so a broken effect fails at import, naming itself, instead of in some recipe much later.
"""

import inspect
import re
from collections.abc import Callable

import numpy as np

Effect = Callable[..., np.ndarray]

# The sections of the effect catalogue (docs/effects.md is laid out in this order). Every effect is in exactly one.
CATEGORIES = (
    "colour", "light", "texture", "stylised", "text", "distortion", "glitch", "atmosphere", "subject", "compositing",
    "backgrounds", "utilities", "animation",
)  # fmt: skip

_EFFECTS: dict[str, Effect] = {}
_CATEGORY: dict[str, str] = {}

# The end of a sentence: a full stop, ! or ? followed by a space or the end. Decimals ("0.5") and ranges ("0..1") are
# never followed by a space; "e.g." and "i.e." are, so they are skipped.
_SENTENCE_END = re.compile(r"(?<!e\.g)(?<!i\.e)[.!?](?=\s|$)")


def effect(fn: Effect | None = None, *, category: str | None = None):
    """Decorator: `@effect(category="colour")` makes `fn(rgb, **params) -> rgb` available under its own name to recipes,
    `fx` and the command line. The function itself is returned unchanged.

    `category` must be one of `CATEGORIES`. Raises `ValueError`, and registers nothing, when the category is missing or
    unknown, when `fn` does not take the image first, when any later parameter has no default (every effect must run as
    `fn(image)`) or collects arbitrary arguments, or when a different function already has the same name. Registering
    the same function again, as reloading its module does, replaces the old one.
    """
    if fn is not None:  # used bare, as `@effect`: there is no category, so this raises and says how to give one
        return _register(fn, category)
    return lambda fn: _register(fn, category)


def unregister(name: str) -> None:
    """Remove the effect called `name` (KeyError if there is none). For tests that register throwaway effects."""
    del _EFFECTS[name]
    del _CATEGORY[name]


def get(name: str) -> Effect:
    """The effect called `name`; KeyError listing the available ones if there is none."""
    try:
        return _EFFECTS[name]
    except KeyError:
        raise KeyError(f"unknown effect {name!r}; available: {', '.join(sorted(_EFFECTS))}") from None


def all_effects() -> dict[str, Effect]:
    """Every registered effect by name (a copy)."""
    return dict(_EFFECTS)


def category(name: str) -> str:
    """The category of the effect called `name` (KeyError like `get`)."""
    get(name)
    return _CATEGORY[name]


def categories() -> dict[str, list[str]]:
    """Every category, in the order of `CATEGORIES`, with the sorted names of its effects (empty if it has none)."""
    return {c: sorted(name for name, of in _CATEGORY.items() if of == c) for c in CATEGORIES}


def apply(name: str, rgb: np.ndarray, **params) -> np.ndarray:
    """Run the effect called `name` with `params`; unknown names and parameters raise with the valid choices."""
    return get(name)(rgb, **params)


def describe() -> dict[str, str]:
    """Effect name -> its documentation, for `photo-fx effects`."""
    return {name: (fn.__doc__ or "").strip() for name, fn in sorted(_EFFECTS.items())}


def summary(name: str) -> str:
    """The first sentence of the documentation of the effect called `name`, on one line."""
    text = " ".join((get(name).__doc__ or "").split())
    end = _SENTENCE_END.search(text)
    return text[: end.end()] if end else text


def _register(fn: Effect, category: str | None) -> Effect:
    name = fn.__name__
    if category not in CATEGORIES:
        given = "has no category" if category is None else f"has the unknown category {category!r}"
        raise ValueError(f"effect {name!r} {given}: write @effect(category=...) with one of: {', '.join(CATEGORIES)}")
    _check_parameters(name, inspect.signature(fn))
    existing = _EFFECTS.get(name)
    if existing is not None and _origin(existing) != _origin(fn):
        raise ValueError(
            f"two effects are called {name!r}: one in {existing.__module__}, another in {fn.__module__}; rename one"
        )
    _EFFECTS[name], _CATEGORY[name] = fn, category
    return fn


def _check_parameters(name: str, signature: inspect.Signature) -> None:
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
