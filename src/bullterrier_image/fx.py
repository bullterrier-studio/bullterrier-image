"""Build recipes in Python: `fx.<effect>(...)` makes a validated step.

    from bullterrier_image import Recipe, fx, render

    look = Recipe("neon-glitch", size=1024, steps=[
        fx.gradient_map(stops=["#12002b", "#ff2bd6", "#00f0ff"]),
        fx.fill("#000000", mask="background"),                  # only where the photo's background is
        fx.glitch(split=12, bands=8, seed=4),
        fx.glow(radius=26, amount=0.65, opacity=0.8),           # mixed in at 80%
        fx.gradient(blend_mode="screen", opacity=0.5),          # a gradient screened over the picture
    ])
    image = render(photo, look)

Every effect listed by `bt-image effects` is here under its own name, with its own parameters and documentation
(`help(fx.glitch)`). After them it takes the step keys of a recipe file, by keyword and with the same meaning: `mask`
(a selection: "subject", "background", "depth" or a region), `opacity` (0..1), `blend_mode` (normal, multiply, screen,
overlay, soft_light, difference, color), for an effect with presets `preset`, and for an effect that draws along the
borders of a partition (`seams`) `partition`. A misspelt parameter or key fails when the step is made,
not when the photo is rendered.

`mask` may name a region the recipe declares (`Recipe(..., regions={"glass": Shards(...)})`); `fx` itself declares none,
so by itself it knows only the built-in `subject` and `background`, and the recipe that holds the step checks the name.
A step that gives each piece of a region its own look is an `Each` (`bullterrier_image.Each`), not an effect.

`fx.pyi` lets editors complete effect names and parameters. It is written from `stub_text()` by
`uv run poe regenerate`, once at integration rather than on every branch, so it can lag behind a branch's effects.
"""

import inspect
import re
from collections.abc import Callable
from functools import cache
from pathlib import Path
from typing import Any, Literal

from .core import registry
from .core.steps import Step
from .errors import RecipeError
from .loader import with_pictures

# The effect builders are module attributes made on demand (`__getattr__`), one per registered effect.
__all__ = ["stub_text"]


def __getattr__(name: str) -> Callable[..., Step]:
    if name.startswith("_"):
        raise AttributeError(name)
    try:
        function = registry.get(name)
    except KeyError:
        raise AttributeError(
            f"fx has no effect {name!r}; available: {', '.join(sorted(registry.describe()))}"
        ) from None
    return _builder(name, function)


def __dir__() -> list[str]:
    return sorted(registry.describe())


@cache  # keyed by the function too, so an effect replaced under the same name (a reload) gets a new builder
def _builder(name: str, function: Callable) -> Callable[..., Step]:
    """The function behind `fx.<name>`: takes the effect's parameters (not the image), then the step keys, and returns
    a Step.

    Values may be given by position, in the model's field order, or by keyword; the step keys (`mask`, `opacity`,
    `blend_mode`, `preset`, `partition`) only by keyword. The Step checks them all, so a misspelt or out-of-range
    value fails here
    exactly as it would in a recipe file. `preset=` names one of the effect's presets (see `registry.presets`); the
    values given win over the preset's. A picture parameter takes a `Picture`, a colour, a path (read now, relative to
    the current folder) or an array (float, height x width x 3 or 4).
    """
    fields = tuple(registry.parameters(name).model_fields)

    def build(*args: Any, **values: Any) -> Step:
        if len(args) > len(fields):
            raise TypeError(f"fx.{name}() takes at most {len(fields)} positional values ({len(args)} given)")
        positional = dict(zip(fields, args, strict=False))
        for twice in positional.keys() & values.keys():
            raise TypeError(f"fx.{name}() got `{twice}` both by position and by keyword")
        if "effect" in values:  # the one step key fx does not take: the function's name already chose the effect
            raise RecipeError(
                f"`effect`: fx.{name}() always builds the effect {name!r}, so it takes no `effect` "
                f"(got {values['effect']!r}); call the fx function of the effect you want"
            )
        keys = {key: values.pop(key) for key in registry.STEP_KEYS if key in values}  # the rest are the parameters
        return Step(name, with_pictures(name, {**positional, **values}, Path()), **keys)

    build.__name__, build.__qualname__ = name, f"fx.{name}"
    build.__doc__ = registry.documentation(name)
    # `inspect.signature` reads this documented hook, which mypy does not model on functions
    build.__signature__ = _signature(name).replace(return_annotation="Step")  # type: ignore[attr-defined]
    return build


def _signature(name: str) -> inspect.Signature:
    """The effect's parameters as a call signature, then the step keys as keywords, typed and defaulted as `Step`
    declares them (`effect` is the builder's own name; `opacity` defaults to the effect's own, `registry.opacity`; a
    key that is also an injected input, `partition`, only for an effect that takes it), and `preset` (one of the
    names) if the effect has presets."""
    signature = registry.parameters(name).signature()
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


def stub_text() -> str:
    """The text of `fx.pyi`: one typed, documented function per effect, for editors."""
    lines = [
        '"""Generated by `poe regenerate`; do not edit by hand."""',
        "",
        "import collections.abc",
        "import os",
        "from typing import Literal",
        "",
        "import numpy",
        "",
        "from .core.params import Picture",
        "from .recipe import Step",
        "",
    ]
    for name in sorted(registry.describe()):
        # Python before 3.14 writes `X | None` of a Literal as Optional[X]; a stub is written the short way
        signature = re.sub(r"Optional\[(Literal\[[^\]]*\])\]", r"\1 | None", str(_signature(name)))
        signature = signature.replace("bullterrier_image.core.params.Picture", "Picture")
        lines.append(f"def {name}{signature} -> Step:")
        doc = registry.documentation(name).replace("\\", "\\\\").replace('"""', "'''")
        lines += [f'    """{doc}"""', "    ...", ""]
    return "\n".join(lines)
