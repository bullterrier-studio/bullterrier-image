"""Finding the depth of a picture: how near each pixel is.

`depth_map(rgb)` answers it at the picture's own size, 1 = nearest, 0 = farthest; `depth_finder()` is the same as a
`Finder` that the renderers take (`finders={"depth": ...}`), named by its model. The model is Depth Anything V2 Small
(Apache-2.0) on ONNX Runtime, the same on macOS, Linux and Windows. It tells relative depth, not distance, and the map
is scaled per photo: the nearest pixel of every photo is 1 and the farthest 0, so one value is a different distance in
every photo. `inverse_depth(rgb)` is the same answer before that scaling, on the model's own scale, which does not
depend on the photo: scale several photos alike with one range of your own and hand the result in as
`maps={"depth": ...}`.

The model (about 99 MB) is not part of the package: the first use downloads it into the user's cache folder, checks its
SHA-256 and keeps it there; nothing is downloaded at import time. Offline, download the file by hand (the error says
from where and to where) or set `BT_IMAGE_DEPTH_MODEL` to a local copy, which is then used as it is and never
downloaded. The work is the core's.

No model at all is needed when the caller already has a depth map: every renderer takes it as `maps={"depth": ...}`
(load a greyscale picture, white = nearest, with `imageio.load_map`, or pass `--map depth=depth.png` on the command
line).
"""

import numpy as np

from bullterrier_image.core import native
from bullterrier_image.core.finder import Finder

from ._runtime import onnxruntime_library


def depth_map(rgb: np.ndarray) -> np.ndarray:
    """Depth map (height, width, float32 0..1) of `rgb`, same size as `rgb`: 1 = nearest, 0 = farthest.

    `inverse_depth(rgb)` scaled so its nearest pixel is 1 and its farthest 0, per photo: the same value in two photos
    is not the same distance. When the model tells nothing apart (every pixel equally far) the map is all 0.

    Raises BackendUnavailableError like `inverse_depth`.
    """
    return native.find_depth(rgb, library=onnxruntime_library(), allow_download=True)


def inverse_depth(rgb: np.ndarray) -> np.ndarray:
    """The model's relative inverse depth of `rgb` (height, width, float32), same size as `rgb`: larger = nearer.

    Not scaled to the photo: the values are on the model's own scale, so the same value in two photos is the same
    nearness and photos can be scaled alike (subtract a shared low, divide by a shared span). Only the order and the
    ratios of differences mean anything: it is neither a distance nor bounded.

    Raises BackendUnavailableError when the model cannot run here: `BT_IMAGE_DEPTH_MODEL` names no file, the download
    fails (the message says how to get the file by hand), or ONNX Runtime is not available.
    """
    return native.find_inverse_depth(rgb, library=onnxruntime_library(), allow_download=True)


def depth_finder() -> Finder:
    """`depth_map` as a `Finder`. Its identity is a hash of the model: the pinned one when the standard model is meant
    (it is not downloaded to find out), else of the file `BT_IMAGE_DEPTH_MODEL` names, so the same model at another
    path is the same finder. Raises BackendUnavailableError when `BT_IMAGE_DEPTH_MODEL` names no file."""
    return Finder(native.depth_finder_identity(), depth_map)
