"""Write the vulture whitelist of the parameter fields that only the Rust core reads.

A ported effect's Python body hands its whole model to the core (`params.model_dump()`), which reads each field by
name, so vulture finds no Python use of a field no other code shares a name with. The fields of the effects the core
implements are listed here, at gate time (`poe dead-code`), so porting an effect edits no shared file; in Rust,
rustc's dead-code lint checks that the core reads each one. Every other name stays checked.

Interface:
    whitelist(models) -> the whitelist's source: a line naming each field of each model
    main(argv)        -> writes the whitelist of the Rust-backed effects' models to argv[0]; exit code 0
"""

import argparse
import sys
from collections.abc import Iterable
from pathlib import Path

from pydantic import BaseModel


def whitelist(models: Iterable[type[BaseModel]]) -> str:
    """Vulture whitelist source naming every field of `models`: an attribute use of each, which vulture counts."""
    return "".join(
        f"_.{field}  # {model.__name__}, read by the Rust core\n" for model in models for field in model.model_fields
    )


def main(argv: list[str] | None = None) -> int:
    """Write the whitelist of the effects the Rust core implements to the path given."""
    from bullterrier_image._native import _core  # noqa: PLC2701  the core is the one that knows which effects it runs
    from bullterrier_image.core import registry

    parser = argparse.ArgumentParser(description="Write the vulture whitelist of the fields only the Rust core reads.")
    parser.add_argument("path", type=Path)
    path = parser.parse_args(argv).path
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(whitelist(registry.parameters(name) for name in _core.effects()), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
