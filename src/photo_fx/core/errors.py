"""The exceptions photo-fx raises: catch `PhotoFxError` for every failure the library reports on purpose.

Each subclass also inherits the builtin exception it replaces, so code that already catches `ValueError`, `KeyError`,
`RuntimeError`, `NotImplementedError` or `OSError` keeps working. Library code raises one of these, never a bare
builtin, for anything a caller, a recipe or a photo can get wrong; the message says what was wrong and what is valid.

Dependency direction: imports nothing, so every layer may raise them.
"""

__all__ = [
    "BackendUnavailableError", "ImageReadError", "PhotoFxError", "ProjectError", "RecipeError", "SubjectNotFoundError",
    "UnknownEffectError",
]  # fmt: skip


class PhotoFxError(Exception):
    """The base of every exception photo-fx raises on purpose."""


class RecipeError(PhotoFxError, ValueError):
    """A recipe, a step, an animation or an effect's parameters are invalid: a misspelt or out-of-range value, an unsafe
    recipe name, a broken or unsupported recipe file. Raised when the value is made or the file is read, wherever it
    comes from (a TOML file, `fx`, a direct call), naming what is wrong and what is valid."""


class ProjectError(PhotoFxError, ValueError):
    """A pipeline project, or the source checkout the developer commands work on, is invalid: a project file that is
    not TOML or has unknown keys, globs that match no photo, two photos with one file name, an out-of-range export or
    check setting, a photo whose name would put its files outside the output folder, or (developer commands) a
    checkout without the template or an example recipe, an effect name that is taken or cannot be one. Raised when
    the project is made or read (or, for that photo, when it is processed), naming the file (if any), the setting and
    the value."""


class UnknownEffectError(RecipeError, KeyError):
    """No effect is registered under the given name; the message lists the known ones."""

    def __str__(self) -> str:
        """The message as written (a plain `KeyError` would show it quoted, as its repr)."""
        return str(self.args[0]) if self.args else ""


class SubjectNotFoundError(PhotoFxError, RuntimeError):
    """The subject of a photo was needed (a masked step, an effect that takes the matte) but could not be found or
    used: no `matte` and no finder given, a finder that found nothing, or a matte of another size than the photo."""


class BackendUnavailableError(PhotoFxError, NotImplementedError):
    """A capability needs a backend this machine does not have, e.g. subject finding without Apple Vision."""


class ImageReadError(PhotoFxError, OSError):
    """A file could not be read as an image: missing, not an image, truncated, or too large to decode safely."""
