"""Looks: recipes of several steps that ship with the package, one file each in `looks/<name>.toml`.

A look file is an ordinary recipe file (see `bullterrier_image.loader`) with a top-level `description`, one sentence
saying what the look is. `read` lists the looks by name, in name order, each with its description and file; the
loader reads the file when a look is asked for by name, the registry names the looks when it explains an unknown effect.
The folder is a parameter only so that a test can read a throwaway one.

Dependency direction: imports `errors` only; the registry builds on it.
"""

import tomllib
from functools import cache
from pathlib import Path
from typing import NamedTuple

from bullterrier_image.errors import RecipeError

SHIPPED = Path(__file__).parent.parent / "looks"


class Look(NamedTuple):
    """One look: what it looks like in a sentence, and the recipe file that makes it."""

    description: str
    file: Path


@cache
def read(folder: Path = SHIPPED) -> dict[str, Look]:
    """The looks of `folder` by name (the file's name without `.toml`), sorted by name; empty when it has none.
    RecipeError when a file lacks its `description`."""
    looks = {}
    for file in sorted(folder.glob("*.toml")):
        description = tomllib.loads(file.read_text(encoding="utf-8")).get("description", "")
        if type(description) is not str or not description.strip():
            raise RecipeError(
                f"look {file.name!r} needs a top-level `description`: one sentence saying what it looks like"
            )
        looks[file.stem] = Look(description, file)
    return looks
