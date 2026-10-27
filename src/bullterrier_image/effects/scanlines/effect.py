"""The `scanlines` effect: dark horizontal lines laid over the picture at a regular spacing, like a CRT screen."""

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect


class ScanlinesParams(EffectParams):
    """Scanlines: horizontal dark lines repeating every `spacing` px, each `thickness` of that spacing thick, darkened
    by `darkness`, with edges as soft as `softness` says."""

    spacing: float = param(
        3.0,
        unit="px",
        min=1,
        max=2000,
        doc="Distance in px from the top of one line to the top of the next. A spacing that is not a whole number "
        "drifts against the pixel rows and the lines are anti-aliased. Larger = fewer, wider-set lines.",
    )
    thickness: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="How thick each line is, as a fraction of `spacing`: 0 = no line (the picture is unchanged), 0.5 = lines "
        "and gaps equally wide, 1 = the lines touch and the whole picture is darkened alike. A line that covers only "
        "part of a pixel row darkens that row in proportion.",
    )
    darkness: float = param(
        0.35,
        unit="fraction",
        min=0,
        max=1,
        doc="How much a line darkens what is under it: 0 = nothing (the picture is unchanged), 0.35 is a visible "
        "screen, 1 = black lines. A line multiplies the picture by 1 - darkness, so colours keep their ratios.",
    )
    offset: float = param(
        0.0,
        unit="px",
        min=0,
        max=2000,
        doc="How far down in px the first line starts from the top of the picture (the middle of its soft edge, when "
        "`softness` is above 0). Offsets of more than `spacing` repeat the pattern: the lines are at offset + k x "
        "spacing.",
    )
    softness: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="How softly a line fades in and out: 0 = hard edges, each row darkened by the share of it a line covers; "
        "1 = as soft as the lines and gaps allow, so with a `thickness` of 0.5 the darkness swells and fades along "
        "sin^2, darkest in the middle of a line and nothing in the middle of a gap, as on a CRT. In between, each "
        "edge fades over `softness` x the narrower of a line and a gap, either side of the hard edge. It keeps the "
        "average darkening of the picture: only its shape changes.",
    )


def _half_area(away: np.ndarray, plateau: float, reach: float) -> np.ndarray:
    """The area under a line's profile (1 up to `plateau`, then a raised-cosine fall over 2 x `reach` to 0) from its
    middle out to `away`, in periods."""
    fall = np.clip(away - plateau, 0, 2 * reach)  # how far into the fall `away` is
    share = np.divide(fall, 2 * reach, out=np.zeros_like(fall), where=reach > 0)  # 0..1 across the fall
    # the fall is 0.5 (1 + cos(pi x share)), whose area from its start is fall / 2 + reach sin(pi x share) / pi
    return np.minimum(away, plateau) + fall / 2 + reach / np.pi * np.sin(np.pi * share)


@effect(category="texture", params=ScanlinesParams)
def scanlines(rgb: np.ndarray, params: ScanlinesParams) -> np.ndarray:
    """Dark horizontal lines every `spacing` px, each `thickness` x `spacing` thick, darkening by `darkness`.

    The lines start at row `offset` and repeat every `spacing` px. A pixel row is multiplied by 1 - `darkness` x the
    share of the row a line covers, so whole rows under a line are exactly 1 - `darkness` times the picture and rows
    no line touches are unchanged; the same gain applies to every column and channel.
    """
    turns = (np.arange(rgb.shape[0] + 1) - params.offset) / params.spacing  # row edges, in periods from the first line
    half = params.thickness / 2  # a line's half width, in periods; its middle is that far after its start
    reach = params.softness * min(half, 0.5 - half)  # each edge fades this far either side of the hard edge
    # Length of line above each position, counting from the first line: whole periods, plus the part of this period,
    # measured from the middle of the nearest line (the profile is the same either side of it).
    periods = np.floor(turns - half + 0.5)
    away = turns - half - periods
    above = periods * params.thickness + half + np.sign(away) * _half_area(np.abs(away), half - reach, reach)
    covered = np.diff(above) * params.spacing  # per row, 0..1: the share of it a line covers
    gain = (1.0 - params.darkness * covered).astype(np.float32)
    return rgb * gain[:, None, None]
