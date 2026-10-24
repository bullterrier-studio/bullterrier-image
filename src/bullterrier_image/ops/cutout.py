"""Working with a subject cut out of its picture: layers that go behind it."""

import numpy as np


def put_behind(rgb: np.ndarray, matte: np.ndarray, alpha: np.ndarray | float, color: np.ndarray) -> np.ndarray:
    """`rgb` with a layer of `color` laid behind the subject: `rgb + (1 - matte) * alpha * (color - rgb)`, in float32.

    `rgb` is a float picture (height, width, 3) in 0..1 whose subject is `matte`, (height, width) in 0..1 (1 = subject,
    soft at its edge). `alpha` is the layer's opacity, one number or (height, width), in 0..1, and `color` its
    colour: three values in 0..1, or a picture like `rgb` for a layer that is not flat. Where the subject is fully
    there (`matte` 1) the picture is exactly as it was, and where `alpha` is 0 it is too, so a layer never touches the
    subject and an empty layer changes nothing; elsewhere each pixel moves from the picture towards `color` by
    `alpha`, and at the subject's soft edge by the share of the pixel the subject does not cover. The input is left
    alone.
    """
    return (rgb + ((1.0 - matte) * alpha)[..., None] * (color - rgb)).astype(np.float32)
