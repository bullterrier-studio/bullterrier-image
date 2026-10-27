"""The exceptions bullterrier-image raises: catch `BullterrierImageError` for every failure the library reports on
purpose.

Each subclass also inherits the builtin exception it replaces, so code that already catches `ValueError`, `KeyError`,
`RuntimeError`, `NotImplementedError` or `OSError` keeps working. Library code raises one of these, never a bare
builtin, for anything a caller, a recipe or a photo can get wrong; the message says what was wrong and what is valid.

Dependency direction: the bottom layer, below `ops`: it imports nothing from bullterrier_image, so every layer may raise
them.
"""

__all__ = [
    "BackendUnavailableError",
    "BullterrierImageError",
    "DepthNotFoundError",
    "ImageReadError",
    "LutError",
    "MapNotFoundError",
    "NonFiniteImageError",
    "ProjectError",
    "RecipeError",
    "SubjectNotFoundError",
    "UnknownEffectError",
]  # fmt: skip


class BullterrierImageError(Exception):
    """The base of every exception bullterrier-image raises on purpose."""


class RecipeError(BullterrierImageError, ValueError):
    """A recipe, a step or an effect's parameters are invalid: a misspelt or out-of-range value, an unsafe
    recipe name, a broken or unsupported recipe file. Raised when the value is made or the file is read, wherever it
    comes from (a TOML file, `fx`, a direct call), naming what is wrong and what is valid."""


class ProjectError(BullterrierImageError, ValueError):
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


class MapNotFoundError(BullterrierImageError, RuntimeError):
    """A map of the photo (see `bullterrier_image.core.registry.MAP_INPUTS`: the subject, the depth, or one a recipe
    declares with a `given` region) is needed but not given, not found, or not usable: no map and no finder given for
    its name, a finder that found nothing, a map of another size than the photo, or values outside 0..1."""


class SubjectNotFoundError(MapNotFoundError):
    """The subject of a photo was needed (a masked step, an effect that takes the subject) but could not be found or
    used: no `subject` map and no finder given, a finder that found nothing, or a map of another size than the photo."""


class DepthNotFoundError(MapNotFoundError):
    """A depth map is needed but not given, not found, or not usable: no `depth` map and no finder given, a finder that
    found nothing, or a map of another size than the photo."""


class NonFiniteImageError(BullterrierImageError, ValueError):
    """A picture to write holds NaN or an infinity, which is no colour: the message names the first such pixel."""


class BackendUnavailableError(BullterrierImageError, NotImplementedError):
    """A capability needs a backend this machine does not have, e.g. subject finding without ONNX Runtime."""


class ImageReadError(BullterrierImageError, OSError):
    """A file could not be read as an image: missing, not an image, truncated, or too large to decode safely."""


class LutError(BullterrierImageError, ValueError):
    """A colour lookup table is unusable: a `.cube` file that is missing, not UTF-8, 1D, malformed (the message names
    the file and the line) or out of the supported size, or a table array of the wrong shape or with values that are
    no numbers."""
