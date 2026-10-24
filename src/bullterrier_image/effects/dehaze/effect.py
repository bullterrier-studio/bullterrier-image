"""The `dehaze` effect: remove haze and fog with the dark channel prior (He, Sun and Tang 2009)."""

from typing import Literal

import numpy as np
from scipy import ndimage

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.color import luminance
from bullterrier_image.ops.guided import guided_filter

# constant: the smallest airlight channel used as a divisor, so a black picture (airlight 0) cannot divide by zero; it
# changes no picture with a visible sky (a channel below 1/1000 is black in any 8-bit file)
_MIN_AIRLIGHT = 1e-3


class DehazeParams(EffectParams):
    """Haze removal by the dark channel prior: how much haze, how it is searched for and how smoothly it is removed."""

    amount: float = param(
        0.9,
        unit="fraction",
        min=0,
        max=1,
        doc="How much of the haze is taken out (He, Sun and Tang's omega): 0 = off, 1 = all of it that the dark "
        "channel finds, which also flattens real distance cues; a little below 1 keeps the picture natural.",
    )
    patch: int = param(
        15,
        unit="px",
        min=1,
        max=201,
        doc="Side of the square patch in which the darkest value is taken to find the haze, in px at the recipe's "
        "size (an even size is centred half a pixel off). Larger patches estimate the haze more reliably but "
        "blur it over bigger areas and miss thin haze; 1 judges every pixel by its own darkest channel.",
    )
    radius: float = param(
        30.0,
        unit="px",
        min=0,
        max=500,
        doc="Radius of the guided filter that refines the haze estimate to follow the picture's edges, in px at "
        "the recipe's size (rounded to whole px). Larger gives a smoother haze with fewer halos far from edges; 0 "
        "keeps the blocky patch estimate.",
    )
    edge_threshold: float = param(
        0.03,
        unit="fraction",
        min=0.001,
        max=1,
        doc="The brightness step (0..1, Rec. 709 luma) that counts as an edge the refined haze estimate must not "
        "blur across: steps above it keep the haze of their own side, steps below it are smoothed. Raising it "
        "gives a smoother estimate and halos around edges; lowering it follows ever smaller edges.",
        advanced=True,
    )
    transmission_floor: float = param(
        0.1,
        unit="fraction",
        min=0.01,
        max=1,
        doc="The lowest transmission (He, Sun and Tang's t0), the share of a pixel's light that reaches the camera: "
        "colours are stretched by at most 1 / transmission_floor, so a low floor clears thick haze but amplifies "
        "noise and clips; 1 allows no stretch at all.",
        advanced=True,
    )
    airlight_fraction: float = param(
        0.001,
        unit="fraction",
        min=0.0001,
        max=0.1,
        doc="The share of the picture's pixels, those with the highest dark channel (the haziest), among which the "
        "brightest one is taken as the colour of the haze (He, Sun and Tang use 0.1 percent = 0.001; at least one "
        "pixel is always used). Raising it makes the estimate less sensitive to a single odd pixel but can pick "
        "a bright object instead of the sky.",
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "nearest",
        doc="What the darkest-value search sees beyond the picture's edge, which sets the haze estimate along the "
        "border: 'nearest' (the edge pixel repeated) and 'reflect' (mirrored, edge pixel repeated) look only at "
        "pixels of the patch inside the picture and give the same result; 'mirror' (mirrored about the edge pixel) "
        "also looks at pixels up to half a patch from the border; 'wrap' (the opposite edge) lets the far side's "
        "darkest values set the haze at the border.",
        advanced=True,
    )


@effect(category="light", params=DehazeParams)
def dehaze(rgb: np.ndarray, params: DehazeParams) -> np.ndarray:
    """Remove haze and fog by inverting the atmospheric scattering model I = J t + A (1 - t) (He, Sun, Tang 2009).

    The dark channel of a haze-free outdoor picture is nearly 0: in any patch of `patch` x `patch` px some pixel has a
    channel near black. Haze lifts it, so the dark channel of I / A measures 1 - t, the haze at that place. The
    steps are
    - the airlight A: the `airlight_fraction` of pixels with the highest dark channel (the haziest) are looked at,
      and the colour of the brightest of them (largest red + green + blue) is A, a per channel colour;
    - the transmission t = 1 - `amount` x dark channel of I / A, refined by a guided filter (`radius`,
      `edge_threshold`), guided by the picture's luma, so it follows the edges of the picture, then held between
      `transmission_floor` and 1;
    - the scene J = (I - A) / t + A, held to 0..1.
    A picture without haze (a dark channel of 0) comes out as it went in. `edge` says what the patch search sees beyond
    the picture's border. Colours are those of the sRGB-encoded picture.
    """
    channel_min = rgb.min(axis=-1)
    patch = params.patch
    dark = ndimage.minimum_filter(channel_min, size=patch, mode=params.edge)

    # Airlight: the brightest pixel among the haziest ones.
    count = max(1, int(params.airlight_fraction * dark.size))
    haziest = np.argpartition(dark.ravel(), dark.size - count)[dark.size - count :]
    pixels = rgb.reshape(dark.size, rgb.shape[-1])[haziest]
    airlight = np.maximum(pixels[np.argmax(pixels.sum(axis=-1))], _MIN_AIRLIGHT)

    # Transmission: from the dark channel of the picture divided by the airlight, refined along the picture's edges.
    dark_normalised = ndimage.minimum_filter((rgb / airlight).min(axis=-1), size=patch, mode=params.edge)
    rough = 1.0 - params.amount * dark_normalised
    refined = guided_filter(luminance(rgb), rough, radius=round(params.radius), epsilon=params.edge_threshold**2)
    transmission = np.clip(refined, params.transmission_floor, 1.0)

    return np.clip((rgb - airlight) / transmission[..., None] + airlight, 0.0, 1.0).astype(np.float32)
