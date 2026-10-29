"""The image every effect works on, float32 (h, w, 3) RGB in 0..1, and the one check of a picture handed in."""

import numpy as np

from bullterrier_image.errors import InvalidImageError, NonFiniteImageError

_AXES = 3  # constant: rows, columns, channels
_RGB = 3  # constant: the channels of RGB
_RGBA = 4  # constant: the channels of RGB with straight alpha last


def as_image(picture: object, *, alpha: bool = False) -> np.ndarray:
    """`picture`, a numpy array a caller hands in, as the image every effect takes: float32, C-contiguous, (h, w, 3)
    RGB in 0..1; with `alpha`, (h, w, 4) with straight alpha last is taken too. The one check of a picture where it
    enters (`render`, `trace`, `render_all`, `Step.apply`, `imageio.save`).

    Unsigned integer levels are divided by their type's largest value (uint8 by 255, uint16 by 65535); floats are kept
    as they are. InvalidImageError, naming what was found and the fix, for anything that is not such an array (a Pillow
    image, a list), another shape, `bool` or signed integer levels, and float values below 0 or above 1 (they are not
    clipped); NonFiniteImageError, naming the first such pixel, for NaN or an infinity."""
    if not isinstance(picture, np.ndarray):
        raise InvalidImageError(
            f"a picture is a numpy array, got {type(picture).__name__}: pass np.asarray(picture) (a Pillow image "
            "gives its 8-bit levels)"
        )
    channels = (_RGB, _RGBA) if alpha else (_RGB,)
    if picture.ndim != _AXES or picture.shape[2] not in channels or 0 in picture.shape:
        held = " or (h, w, 4) RGBA, straight alpha last," if alpha else ""
        drop = "" if alpha else "; drop an alpha channel with picture[..., :3]"
        raise InvalidImageError(
            f"a picture is (h, w, 3) RGB{held} of at least one pixel, got the shape {picture.shape}: make greyscale "
            f"(h, w) RGB with np.dstack([grey] * 3){drop}"
        )
    kind = picture.dtype.kind
    if kind == "u":
        return (picture / float(np.iinfo(picture.dtype).max)).astype(np.float32)
    if kind != "f":
        raise InvalidImageError(
            f"a picture's levels are unsigned integers (uint8 0..255, uint16 0..65535) or floats 0..1, got "
            f"{picture.dtype}: convert it, e.g. picture.astype(np.uint8) or picture.astype(np.float32) / largest"
        )
    image = np.ascontiguousarray(picture, dtype=np.float32)
    finite = np.isfinite(image)
    if not finite.all():
        row, column = np.argwhere(~finite)[0][:2]
        raise NonFiniteImageError(
            f"a picture's levels must be numbers, but it holds NaN or an infinity: the first is at row {row}, "
            f"column {column}"
        )
    low, high = float(image.min()), float(image.max())
    if low < 0 or high > 1:
        # the values in full (`repr`): a level a hair out of range must not print as the bound it misses
        beyond = [f"{high!r} is {high - 1!r} above 1"] * (high > 1) + [f"{low!r} is {-low!r} below 0"] * (low < 0)
        raise InvalidImageError(
            f"a picture's float levels are 0..1, got min {low!r}, max {high!r}: {'; '.join(beyond)}; scale it into "
            "0..1 (8-bit levels as uint8 are divided by 255 for you)"
        )
    return image
