"""The effect library: one folder per effect, found automatically.

An effect is a function `(rgb, params) -> rgb`: it takes the image representation described in
`imageio` and its validated parameter model (an `EffectParams` subclass, see `bullterrier_image.core.params`) and
returns a new image of the same shape. Effects know nothing about files or recipes; where and how strongly one applies
is decided by the recipe that calls it.

An effect's code is one folder of this package, named like the effect, holding code only: `effect.py` (its parameter
model and its function, registered with `@effect(category=..., params=...)` from `bullterrier_image.core.registry`; the
only module of the folder that registers anything) and `__init__.py` (imports `effect.py`). Its tests, example recipe
and gallery picture live outside the package under the same name (`tests/effects/<name>/`,
`examples/effects/<name>.toml`, `docs/gallery/<name>.jpg`; see `bullterrier_image.gallery.effect_files`). `uv run poe
new-effect <name> --category <category>` writes all of them from the template in `_template/`. Nothing else needs
editing: importing this package imports every folder whose name does not start with `_`, in sorted order. A folder that
registers no effect, or another number, or an effect named otherwise or registered outside `effect.py`, a folder without
`__init__.py`, and a loose module (`effects/<name>.py`) are each a `ValueError` at import, naming the folder or file
(the registry reports it as a `BullterrierImageError` naming this package's entry point).
Effects never import each other: shared code lives in `bullterrier_image.ops`. This package only finds the effects; the
registry imports it on its first query, through bullterrier-image's entry point in the group `bullterrier_image.effects`
(`pyproject.toml`), exactly as it imports a third party's effect package. Look effects up in
`bullterrier_image.core.registry` (`get`, `apply`, `describe`...).
"""

import importlib
import pkgutil
from pathlib import Path

from bullterrier_image.core import registry as _registry

# The one module of an effect folder that registers the effect.
_EFFECT_MODULE = "effect"


def _discover() -> None:
    """Import every effect folder of this package, then check the folder rules."""
    here = Path(__file__).parent
    folders = sorted(
        info.name for info in pkgutil.iter_modules(__path__) if info.ispkg and not info.name.startswith("_")
    )
    for entry in sorted(here.iterdir()):
        if entry.name.startswith(("_", ".")) or entry.name in folders:
            continue
        if entry.suffix == ".py":  # an effect left here as a module, or hidden by a folder, would never be found
            raise ValueError(
                f"effects/{entry.name} is a loose module: an effect lives in a folder, effects/{entry.stem}/"
            )
        # A folder left holding only `__pycache__` (after a move or a branch switch) is residue, not a missing effect.
        if entry.is_dir() and any(child.name != "__pycache__" for child in entry.iterdir()):
            raise ValueError(f"effects/{entry.name}/ has no __init__.py, so its effect would never be found: add one")
    for name in folders:
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


_discover()
