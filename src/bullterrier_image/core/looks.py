"""Looks: recipes of several steps that ship with the core, one file each in `crates/core/data/looks/<name>.toml`.

A look file is an ordinary recipe file (see `bullterrier_image.loader`) with a top-level `description`, one sentence
saying what the look is. `read` lists the looks by name, in name order, each with its description and file; the
loader reads the file when a look is asked for by name, the registry names the looks when it explains an unknown effect.
The folder is a parameter only so that a test can read a throwaway one; none reads the looks the core ships.

Dependency direction: imports `native` and `errors` only; the registry builds on it.
"""

import tomllib
from functools import cache
from pathlib import Path
from typing import NamedTuple

from bullterrier_image.errors import RecipeError

from . import native


class Look(NamedTuple):
    """One look: what it looks like in a sentence, and the text of the recipe file that makes it."""

    description: str
    text: str


@cache
def read(folder: Path | None = None) -> dict[str, Look]:
    """The looks of `folder` by name (the file's name without `.toml`), sorted by name; empty when it has none.
    RecipeError when a file lacks its `description`."""
    if folder is None:
        files = dict(native.shipped_looks())
    else:
        files = {file.stem: file.read_text(encoding="utf-8") for file in sorted(folder.glob("*.toml"))}
    looks = {}
    for name, text in files.items():
        description = tomllib.loads(text).get("description", "")
        if type(description) is not str or not description.strip():
            raise RecipeError(
                f"look '{name}.toml' needs a top-level `description`: one sentence saying what it looks like"
            )
        looks[name] = Look(description, text)
    return looks
