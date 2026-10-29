"""Build recipes in Python: `fx.<effect>(...)` makes a validated step.

    from bullterrier_image import Recipe, fx, render

    look = Recipe([
        fx.gradient_map(stops=["#12002b", "#ff2bd6", "#00f0ff"]),
        fx.gradient(colors=["#000000"], mask="background"),                  # only where the photo's background is
        fx.slice_shift(preset="glitch", bands=8, seed=4),
        fx.rgb_split(angle=0, amount=5.6, edge="wrap"),
        fx.bloom(radius=12.2, amount=0.65, opacity=0.8),           # mixed in at 80%
        fx.gradient(blend_mode="screen", opacity=0.5),          # a gradient screened over the picture
    ], name="neon-glitch", size=1024)
    image = render(photo, look)

    fx.bloom(radius=12).apply(photo)                                   # one effect, no recipe
    fx.slice_shift.presets["glitch"]                                     # an effect's presets: {parameter: value}

Every effect listed by `bt-image effects` (and in `fx.__all__`) is here under its own name, with its own parameters and
documentation (`help(fx.bloom)`). After them it takes the step keys of a recipe file, with the same meaning: `mask` (a
selection: "subject", "background", "depth", "light" or a region), `opacity` (0..1), `blend_mode` (normal, multiply,
screen, overlay, soft_light, difference, color), `name` (a word that addresses the step's numbers, see
`Recipe.parameters`), for an effect with presets `preset`, and for an effect that draws along the borders of a partition
(`seams`) `partition`. A number the effect's documentation marks "(drivable)" also takes a `Drive`
(`radius=Drive(by="far", start=0, end=12)`): it then varies across the picture by a field. A misspelt parameter or key
fails when the step is made, not when the photo is rendered, with the nearest valid name.

Everything is given by keyword. Across the API, a call takes at most one argument by position, the thing itself
(`Recipe(steps)`, `Step(effect)`, `Each(each)`); the rest are keywords, so a new or reordered field never changes what
a call means. A step is changed by copying it, `step.replace(radius=20, opacity=0.5)`, checked as `fx` checks it.

`mask` may name a region the recipe declares (`Recipe(..., regions={"glass": Shards(...)})`); `fx` itself declares none,
so by itself it knows only the built-in `subject` and `background`, and the recipe that holds the step checks the name,
as it does for the `field` of an effect that works from one.
A step that gives each piece of a region its own look is an `Each` (`bullterrier_image.Each`), not an effect.

`fx.pyi` lets editors complete effect names and parameters. It is written by `uv run poe regenerate`
(`gallery.stub_text`), once at integration rather than on every branch, so it can lag behind a branch's effects.
"""

import inspect
from collections.abc import Callable
from functools import cache
from pathlib import Path
from typing import Any, Literal, Protocol, cast

from .core import registry
from .core.params import signature as parameter_signature
from .errors import RecipeError
from .loader import with_files
from .recipe import Step  # the public home: importing it binds what `Step.apply` renders with


class _Effect(Protocol):
    """What `fx.<effect>` is: called with the effect's parameters and the step keys, all by keyword, it returns the
    `Step`; `presets` is the effect's presets, {preset: {parameter: value}}."""

    __name__: str
    presets: dict[str, dict[str, Any]]

    def __call__(self, **values: Any) -> Step: ...


__all__: list[str]  # every effect's name, as registered now (`__getattr__` answers it): only declared here


def __getattr__(name: str) -> _Effect:
    """The step builder of the effect `name`, made on demand, one per registered effect; `__all__` is every effect's
    name, as registered now."""
    if name == "__all__":
        return __dir__()  # type: ignore[return-value]  # the one attribute of the module that is no effect
    if name.startswith("_"):
        raise AttributeError(name)
    try:
        function = registry.get(name)
    except KeyError as error:  # the registry says where the name lives, if it is a preset or a look
        raise AttributeError(str(error)) from None
    return _builder(name, function)


def __dir__() -> list[str]:
    return sorted(registry.describe())


@cache  # keyed by the function too, so an effect replaced under the same name (a reload) gets a new builder
def _builder(name: str, function: Callable) -> _Effect:
    """The function behind `fx.<name>`: takes the effect's parameters (not the image), then the step keys, and returns
    a Step.

    Every value is given by keyword; the step keys (`mask`, `opacity`, `blend_mode`, `preset`, `partition`, `name`)
    follow the effect's parameters. The Step checks them all, so a misspelt or out-of-range value fails here exactly
    as it would in a recipe file. `preset=` names one of the effect's presets (see `registry.presets`); the values
    given win over the preset's. A picture parameter takes a `Picture`, a colour, a path (read now, relative to the
    current folder) or an array (float, height x width x 3 or 4). A lookup table parameter takes a `Lut`, the path of
    a `.cube` file or an array (float, n x n x n x 3).
    """

    def build(*args: Any, **values: Any) -> Step:
        if args:
            first = next(iter(registry.parameters(name).model_fields), "value")
            raise TypeError(
                f"fx.{name}() takes its values by keyword, not by position (got {len(args)} positional): write "
                f"fx.{name}({first}={args[0]!r})"
            )
        if "effect" in values:  # the one step key fx does not take: the function's name already chose the effect
            raise RecipeError(
                f"`effect`: fx.{name}() always builds the effect {name!r}, so it takes no `effect` "
                f"(got {values['effect']!r}); call the fx function of the effect you want"
            )
        keys = {key: values.pop(key) for key in registry.STEP_KEYS if key in values}  # the rest are the parameters
        return Step(name, params=with_files(name, values, Path()), **keys)

    build.__name__, build.__qualname__ = name, f"fx.{name}"
    build.__doc__ = registry.documentation(name)
    # the effect's presets, `{preset: {parameter: value}}`: a function attribute, which mypy does not model
    build.presets = {preset: dict(found.params) for preset, found in registry.presets(name).items()}  # type: ignore[attr-defined]
    # `inspect.signature` reads this documented hook, which mypy does not model on functions
    build.__signature__ = _signature(name).replace(return_annotation="Step")  # type: ignore[attr-defined]
    return cast("_Effect", build)  # `presets` was given above


def _signature(name: str) -> inspect.Signature:
    """The effect's parameters as a call signature, then the step keys as keywords, typed and defaulted as `Step`
    declares them (`effect` is the builder's own name; `opacity` defaults to the effect's own, `registry.opacity`; a
    key that is also an injected input, `partition`, only for an effect that takes it), and `preset` (one of the
    names) if the effect has presets."""
    signature = parameter_signature(registry.parameters(name))
    step = Step.model_fields
    defaults = {key: step[key].default for key in step} | {"opacity": registry.opacity(name)}
    keys = [
        inspect.Parameter(key, inspect.Parameter.KEYWORD_ONLY, default=defaults[key], annotation=step[key].annotation)
        for key in registry.STEP_KEYS
        if key in step and key != "effect" and (key not in registry.INPUTS or key in registry.inputs(name))
    ]
    names = tuple(registry.presets(name))
    if names:
        keys.append(
            inspect.Parameter("preset", inspect.Parameter.KEYWORD_ONLY, default=None, annotation=Literal[names] | None)
        )
    return signature.replace(parameters=[*signature.parameters.values(), *keys])
