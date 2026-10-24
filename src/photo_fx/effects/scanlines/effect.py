"""The `scanlines` effect: dark horizontal lines laid over the picture at a regular spacing, like a CRT screen."""

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect


class ScanlinesParams(EffectParams):
    """Scanlines: horizontal dark lines repeating every `spacing` px, each `thickness` of that spacing thick, darkened
    by `darkness`."""

    spacing: float = param(
        3.0,
        unit="px",
        min=1,
        max=64,
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
        max=64,
        doc="How far down in px the first line starts from the top of the picture. Offsets of more than `spacing` "
        "repeat the pattern: the lines are at offset + k x spacing.",
    )


@effect(category="texture", params=ScanlinesParams)
def scanlines(rgb: np.ndarray, params: ScanlinesParams) -> np.ndarray:
    """Dark horizontal lines every `spacing` px, each `thickness` x `spacing` thick, darkening by `darkness`.

    The lines start at row `offset` and repeat every `spacing` px. A pixel row is multiplied by 1 - `darkness` x the
    share of the row a line covers, so whole rows under a line are exactly 1 - `darkness` times the picture and rows
    no line touches are unchanged; the same gain applies to every column and channel.
    """
    rows = np.arange(rgb.shape[0] + 1) - params.offset  # the top edge of each row, and the bottom of the last
    line = params.thickness * params.spacing
    # Length of line above each position, counting from the first line: whole periods, plus the part of this period.
    above = np.floor(rows / params.spacing) * line + np.minimum(np.mod(rows, params.spacing), line)
    covered = np.diff(above)  # per row, 0..1: the share of it a line covers
    gain = (1.0 - params.darkness * covered).astype(np.float32)
    return rgb * gain[:, None, None]
