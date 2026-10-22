"""The effect library: one folder per effect, found automatically.

An effect is a function `(rgb, params, *, matte=None) -> rgb`: it takes the image representation described in
`imageio` and its validated parameter model (an `EffectParams` subclass, see `photo_fx.core.params`) and returns a new
image of the same shape. Effects know nothing about files or recipes; where and how strongly one applies is decided by
the recipe that calls it.

An effect's code is one folder of this package, named like the effect, holding code only: `effect.py` (its parameter
model and its function, registered with `@effect(category=..., params=...)` from `photo_fx.core.registry`; the only
module of the folder that registers anything) and `__init__.py` (imports `effect.py`). Its tests, example recipe and
gallery picture live outside the package under the same name (`tests/effects/<name>/`, `examples/effects/<name>.toml`,
`docs/gallery/<name>.jpg`; see `photo_fx.gallery.effect_files`). `uv run photo-fx new-effect <name> --category
<category>` writes all of them from the template in `_template/`. Nothing else needs editing: importing this package
imports every folder (and, TRANSITIONAL, every module not yet moved into a folder) whose name does not start with `_`,
in sorted order. A folder that registers no effect, or another number, or an effect named otherwise or registered
outside `effect.py`, a folder without `__init__.py`, and a folder and a module of the same name are each a `ValueError`
at import, naming the folder.
Effects never import each other: shared code lives in `photo_fx.ops`.

The query names below (`get`, `apply`, `describe`...) are the registry's, re-exported so that `photo_fx.effects.get`
and friends keep working.
"""

import importlib
import pkgutil
from pathlib import Path

from photo_fx.core import registry as _registry

# The one module of an effect folder that registers the effect.
_EFFECT_MODULE = "effect"


def _discover() -> None:
    """Import every folder and (TRANSITIONAL) loose module of this package, then check the folder rules."""
    here = Path(__file__).parent
    found = {info.name: info.ispkg for info in pkgutil.iter_modules(__path__) if not info.name.startswith("_")}
    for entry in sorted(here.iterdir()):
        if not entry.is_dir() or entry.name.startswith(("_", ".")) or found.get(entry.name):
            continue
        # A folder left holding only `__pycache__` (after a move or a branch switch) is residue, not a missing effect.
        if any(child.name != "__pycache__" for child in entry.iterdir()):
            raise ValueError(f"effects/{entry.name}/ has no __init__.py, so its effect would never be found: add one")
    folders = sorted(name for name, is_folder in found.items() if is_folder)
    for name in folders:
        if (here / f"{name}.py").exists():
            raise ValueError(
                f"effects/{name}/ and effects/{name}.py share a name, and the folder would hide the module: rename one"
            )
    for name in sorted(found):
        importlib.import_module(f"{__name__}.{name}")
    for name in folders:
        _check_folder(name)


def _check_folder(folder: str) -> None:
    package = f"{__name__}.{folder}"
    registered = sorted(
        name
        for name, function in _registry.all_effects().items()
        if function.__module__ == package or function.__module__.startswith(f"{package}.")
    )
    if not registered:
        raise ValueError(
            f"effects/{folder}/ registers no effect: its effect.py must define @effect def {folder}(rgb, params), "
            f"and its __init__.py import effect.py"
        )
    if registered != [folder]:
        raise ValueError(
            f"effects/{folder}/ registers {', '.join(registered)}: a folder holds exactly one effect, named like the "
            f"folder ({folder})"
        )
    module = _registry.get(folder).__module__
    if module != f"{package}.{_EFFECT_MODULE}":
        raise ValueError(f"effects/{folder}/: the effect is registered in {module}; register it in effect.py")


# Before the public names below are bound, so that a folder or module named like one of them cannot replace it.
_discover()


# Re-exported for compatibility: these lived in this package before the registry moved to `core` (M1).
from photo_fx.core.registry import (  # noqa: E402  (must follow the discovery)
    CATEGORIES,
    all_effects,
    apply,
    categories,
    category,
    describe,
    get,
    summary,
)

__all__ = ["CATEGORIES", "all_effects", "apply", "categories", "category", "describe", "get", "summary"]
