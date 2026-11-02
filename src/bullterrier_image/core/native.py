"""The Rust extension's entry points, raising the package's exceptions.

The extension cannot import `errors` (it is a leaf beside it), so it raises `_core.CoreError`, a `ValueError` whose
`kind` names the exception it stands for (`ErrorKind` in the Rust core is the one list). `entry` turns that into the
`BullterrierImageError` subclass of the kind; any other exception, such as one a Python effect raised and the core
handed back unchanged, passes through as it is. Call the core through this module, not through `_core`, wherever a
caller catches the package's exceptions.
"""

import functools
from collections.abc import Callable

from bullterrier_image._native import _core
from bullterrier_image.errors import (
    BackendUnavailableError,
    BullterrierImageError,
    DepthNotFoundError,
    ImageReadError,
    ImageWriteError,
    InvalidImageError,
    MapNotFoundError,
    NonFiniteImageError,
    RecipeError,
    SubjectNotFoundError,
    UnknownEffectError,
)

# The exception of each kind the core reports, but `other` (a programming error), which stays the `CoreError` itself.
_RAISED_AS: dict[str, type[BullterrierImageError]] = {
    "recipe": RecipeError,
    "unknown_effect": UnknownEffectError,
    "subject_not_found": SubjectNotFoundError,
    "depth_not_found": DepthNotFoundError,
    "map_not_found": MapNotFoundError,
    "invalid_image": InvalidImageError,
    "non_finite_image": NonFiniteImageError,
    "image_read": ImageReadError,
    "image_write": ImageWriteError,
    "backend_unavailable": BackendUnavailableError,
}


_KIND_OF = {raised: kind for kind, raised in _RAISED_AS.items()}


def found_nothing(error: MapNotFoundError) -> _core.CoreError:
    """`error`, a map the package could not find, as the `CoreError` that tells the core a finder found nothing: of the
    kind of `error` (`subject_not_found`, `depth_not_found` or `map_not_found`)."""
    raised = _core.CoreError(str(error))
    raised.kind = next(_KIND_OF[cls] for cls in type(error).__mro__ if cls in _KIND_OF)
    return raised


def entry[**P, R](function: Callable[P, R]) -> Callable[P, R]:
    """`function`, an entry point of the core, raising the exception of the kind of any error the core reports (the
    `CoreError` is its `__cause__`). A kind the package has no exception for is a `KeyError`: the core and this table
    disagree, which is a bug to see, not to hide."""

    @functools.wraps(function)
    def wrapped(*args: P.args, **kwargs: P.kwargs) -> R:
        try:
            return function(*args, **kwargs)
        except _core.CoreError as error:
            if error.kind == "other":
                raise
            raise _RAISED_AS[error.kind](str(error)) from error

    return wrapped


effects = entry(_core.effects)
params = entry(_core.params)
effect = entry(_core.effect)
register = entry(_core.register)
unregister = entry(_core.unregister)
render = entry(_core.render)
trace = entry(_core.trace)
check_recipe = entry(_core.check_recipe)
needs = entry(_core.needs)
known_maps = entry(_core.known_maps)
