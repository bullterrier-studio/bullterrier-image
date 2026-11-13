"""Finding the subject of a picture: the person, pet or object that stands out.

One question, one answer: `subject_matte(rgb)` returns how much each pixel belongs to the subject (1) or to the
background (0), at the picture's own size; `subject_finder()` is the same as a `Finder` that the renderers take
(`finders={"subject": ...}`), named by its backend and model. Three backends can answer it:

- `"vision"`, the default on macOS 14 and later: Apple Vision's subject segmentation (the "lift subject" feature). Runs
  on the Neural Engine, needs no download. Its cut is cleaner than the portable model's, and differs from it, so a look
  made on a Mac cuts a little differently elsewhere.
- `"onnx"`, the default everywhere else: a portable model, IS-Net (general use, Apache-2.0), run by ONNX Runtime on
  macOS, Linux and Windows. The model (about 180 MB) is not part of the package: the first use downloads it into the
  user's cache folder, checks its SHA-256 and keeps it there; nothing is downloaded at import time. Offline, download
  the file by hand (the error says from where and to where) or set `BT_IMAGE_SUBJECT_MODEL` to a local copy, which is
  then used as it is and never downloaded.

- `"sam2"`, never a default: SAM 2.1 Hiera Large (Apache-2.0 weights and code) run by ONNX Runtime, which finds what
  you point at instead of what stands out: `points=[(x, y, positive), ...]`, clicks as shares of the picture
  (`(0.5, 0.4, True)` is a click at the middle, a bit above the centre, on the thing wanted; `False` is a click on
  something to leave out), and optionally `box=(left, top, right, bottom)`. With neither it is refused: it does not
  guess. The models (about 910 MB) download once into the cache folder like the portable one, SHA-256 checked. A
  find takes seconds on a laptop CPU and about 2.6 GB of memory.

The backend is chosen once, by, in order: the `segmenter` argument; the `BT_IMAGE_SEGMENTER` environment variable
(`onnx` or `vision`); otherwise by what the platform has, never by trying one and falling back. The work is the core's.

No model at all is needed when the caller already has the subject's matte: every renderer takes it as
`maps={"subject": ...}` (load a black-and-white or transparent PNG with `imageio.load_map`, or pass
`--map subject=cutout.png` on the command line).
"""

import functools
from collections.abc import Sequence

import numpy as np

from bullterrier_image.core import native
from bullterrier_image.core.finder import Finder


def subject_matte(
    rgb: np.ndarray,
    *,
    shrink: float = 1.5,
    feather: float = 1.0,
    segmenter: str | None = None,
    points: Sequence[tuple[float, float, bool]] = (),
    box: tuple[float, float, float, float] | None = None,
) -> np.ndarray:
    """Alpha matte (height, width, float32 0..1) for the main subject of `rgb`, same size as `rgb`.

    The segmenter's edge is soft and slightly generous, which shows as a coloured halo once the background is
    replaced. `shrink` pulls the edge in by that many pixels and `feather` blurs it by that many pixels,
    both measured in `rgb`'s own pixels; the defaults give a clean, slightly soft cut for a 1000-1600 px photo.
    `segmenter` is `"vision"`, `"onnx"` or `"sam2"`; None leaves the choice to `BT_IMAGE_SEGMENTER`, then to the
    platform. `points` (x, y, positive: shares of the picture from 0 to 1, across and down, and whether the click is on
    what is wanted) and `box` (left, top, right, bottom, shares) say what `"sam2"` is to find; it needs one of them, and
    no other segmenter takes them.

    Raises SubjectNotFoundError if nothing stands out (for `"sam2"`: nothing at what was pointed at),
    BackendUnavailableError for a segmenter name that is unknown, or if the chosen backend cannot run here (the message
    says what to do instead), and ValueError, naming `points` or `box`, for `"sam2"` with neither, for a share outside
    0 to 1, or for them given to another segmenter.
    """
    return native.find_subject(
        rgb,
        segmenter=segmenter,
        shrink=shrink,
        feather=feather,
        allow_download=True,
        points=points,
        box=box,
    )


def subject_finder(
    segmenter: str | None = None,
    *,
    points: Sequence[tuple[float, float, bool]] = (),
    box: tuple[float, float, float, float] | None = None,
) -> Finder:
    """`subject_matte` with its default edge clean-up, as a `Finder`: the segmenter is chosen now and kept.

    `segmenter`, `points` and `box` are as for `subject_matte`. The identity names the backend and its model, so it
    changes exactly when the answers could: for the portable model a hash of the model (the pinned one when the
    standard model is meant, it is not downloaded to find out; else of the file `BT_IMAGE_SUBJECT_MODEL` names, so the
    same model at another path is the same finder); for `"sam2"` the hashes of its four model files and the points and
    box exactly, so another click is another identity; for Apple Vision the macOS version, whose release brings the
    model. Raises BackendUnavailableError for an unknown segmenter name (naming the choices) or a backend that cannot
    run here, naming what to do.
    """
    identity = native.subject_finder_identity(segmenter=segmenter, points=points, box=box)
    chosen = identity.split(":", 1)[0]  # "onnx", "sam2" or "vision": what the identity was made for
    return Finder(identity, functools.partial(subject_matte, segmenter=chosen, points=points, box=box))


def refine_edge(alpha: np.ndarray, shrink: float, feather: float) -> np.ndarray:
    """Clean up the edge of a matte (height, width, 0..1): returns a new float32 matte of the same size, in 0..1.

    `shrink` (px) pulls the edge in: a pixel keeps the lowest value within a disc of that radius around it, so a halo
    up to `shrink` px wide is cut away; the picture's own border does not count as an edge. Then `feather` (px) blurs
    the edge with a Gaussian of that sigma, which softens it without changing the matte's total. 0 turns either off,
    and so does a negative value (it is not an error).
    `subject_matte` applies this to the model's answer; use it on a matte from anywhere else for the same clean cut.
    """
    return native.refine_edge(alpha, shrink, feather)
