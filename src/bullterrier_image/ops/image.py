"""The image every effect works on, float32 (h, w, 3) RGB in 0..1, and the one check of a picture handed in."""

import numpy as np

from bullterrier_image.errors import InvalidImageError, NonFiniteImageError

_AXES = 3  # constant: rows, columns, channels
_GREY_AXES = 2  # constant: rows and columns only
_BATCH_AXES = 4  # constant: a picture per first index
_CHANNEL_AXIS = 2  # constant: the last axis of (h, w, channels)
_GREY = 1  # constant: the channels of a grey picture
_RGB = 3  # constant: the channels of RGB
_RGBA = 4  # constant: the channels of RGB with straight alpha last


def as_image(picture: object, *, alpha: bool = False) -> np.ndarray:
    """`picture`, a numpy array a caller hands in, as the image every effect takes: float32, C-contiguous, (h, w, 3)
    RGB in 0..1; with `alpha`, (h, w, 4) with straight alpha last is taken too. The one check of a picture where it
    enters (`render`, `trace`, `render_all`, `Step.apply`, `imageio.save`).

    Unsigned integer levels are divided by their type's largest value (uint8 by 255, uint16 by 65535); floats are kept
    as they are. InvalidImageError, naming what was found and the fix, for anything that is not such an array (a Pillow
    image, a list), another shape, `bool`, signed or wider unsigned integer levels, and float values below 0 or above 1
    (they are not clipped); NonFiniteImageError, naming the first such pixel, for NaN or an infinity."""
    if not isinstance(picture, np.ndarray):
        raise InvalidImageError(
            f"a picture is a numpy array, got {type(picture).__name__}: pass np.asarray(picture) (a Pillow image "
            "gives its 8-bit levels)"
        )
    channels = (_RGB, _RGBA) if alpha else (_RGB,)
    if picture.ndim != _AXES or picture.shape[2] not in channels or 0 in picture.shape:
        held = " or (h, w, 4) RGBA, straight alpha last," if alpha else ""
        raise InvalidImageError(
            f"a picture is (h, w, 3) RGB{held} of at least one pixel, got the shape {picture.shape}: "
            f"{_shape_fix(picture.shape, alpha=alpha)}"
        )
    if picture.dtype in (np.uint8, np.uint16):
        return (picture / float(np.iinfo(picture.dtype).max)).astype(np.float32)
    if picture.dtype.kind != "f":
        raise InvalidImageError(
            f"a picture's levels are uint8 (0..255), uint16 (0..65535) or floats 0..1, got {picture.dtype}: convert "
            "it, e.g. picture.astype(np.float32) / largest, where largest is the value that means white"
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


def _shape_fix(shape: tuple[int, ...], *, alpha: bool) -> str:
    """What to do about a picture of this `shape` that is no RGB picture, by what the shape looks like."""
    if 0 in shape:
        return "it has no pixels"
    if len(shape) == _GREY_AXES:
        return "greyscale (h, w): make it RGB with np.dstack([grey] * 3)"
    if len(shape) == _BATCH_AXES:
        return "a batch of pictures: pass one, picture[0]"
    if len(shape) != _AXES:
        return "pass one picture, an array of (h, w, 3)"
    channels = shape[_CHANNEL_AXIS]
    if channels == _GREY:
        return "one channel: repeat it into RGB with np.repeat(picture, 3, axis=2)"
    if channels == _RGBA and not alpha:
        return "RGBA: drop the alpha channel with picture[..., :3]"
    return f"{channels} channels: RGB has three"
