"""Seeded noise: white noise clumped into specks."""

from typing import Literal

import numpy as np

from bullterrier_image.ops.blur import gaussian_blur


def clumped_noise(
    shape: tuple[int, int],
    *,
    size: float,
    seed: int,
    truncate: float,
    edge: Literal["reflect", "nearest", "mirror", "wrap"],
    distribution: Literal["gaussian", "uniform"] = "gaussian",
) -> np.ndarray:
    """Seeded white noise of `shape` (height, width), float32, clumped into specks about `size` px wide.

    Up to a `size` of 1 every pixel is an independent value of zero mean and unit standard deviation, standard-normal
    for the 'gaussian' `distribution`, evenly spread over +-sqrt(3) for 'uniform'. Above 1 the noise is blurred by a
    Gaussian of sigma `size / 2` (cut off at `truncate` sigmas, `edge` deciding what lies beyond the border, as in
    `ops.blur.gaussian_blur`) and scaled back to unit standard deviation, so its strength does not depend on `size`.
    A pure function of its arguments.
    """
    rng = np.random.default_rng(seed)
    if distribution == "gaussian":
        noise = rng.standard_normal(shape).astype(np.float32)
    else:  # uniform on -b..b has standard deviation b / sqrt(3), so b = sqrt(3) gives 1
        noise = ((rng.random(shape) * 2 - 1) * np.sqrt(3)).astype(np.float32)  # constant: the uniform's deviation
    if size > 1.0:  # a speck of 1 px or less is a single pixel: nothing to clump
        noise = gaussian_blur(noise, size / 2.0, truncate=truncate, edge=edge)
        noise /= max(float(noise.std()), 1e-6)  # constant: guard against dividing by zero on a 1-pixel picture
    return noise
