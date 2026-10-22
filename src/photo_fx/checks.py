"""Automatic quality checks: what a person would otherwise have to look at to spot a bad result.

Each check returns a list of plain-English warnings (empty = fine). Checks never raise and never block an export; the
pipeline puts them in its report, so a batch of hundreds of photos only needs a human for the ones that are flagged.
"""

import numpy as np


def matte_warnings(matte: np.ndarray, share_range: tuple[float, float] = (0.02, 0.97)) -> list[str]:
    """Warn if the subject cut-out covers an implausible share of the photo (it missed the subject, or took it all)."""
    low, high = share_range
    share = float((matte >= 0.5).mean())
    if share < low:
        return [f"the cut-out covers only {share:.1%} of the photo: the subject may have been missed"]
    if share > high:
        return [f"the cut-out covers {share:.1%} of the photo: there is almost no background to separate"]
    return []


def still_warnings(image: np.ndarray) -> list[str]:
    """Warn about a result that is broken, empty, blown out or without any detail."""
    if not np.isfinite(image).all():
        return ["the result contains invalid pixel values"]
    if float(image.max()) < 0.02:
        return ["the result is blank (all black)"]
    if float(image.mean()) > 0.97:
        return ["the result is almost entirely white"]
    if float(image.std()) < 0.01:
        return ["the result is a flat colour with no detail"]
    return []


class MotionMeter:
    """Measures how much an animation changes from frame to frame, to catch clips whose motion dies early.

    Feed it every frame with `add`; ask `warnings` at the end. A clip whose last stretch is frozen (nothing moves
    for a good part of its length) looks like it stopped, even though the file is fine.
    """

    def __init__(self) -> None:
        self.energy: list[float] = []  # mean change between each frame and the one before
        self.shape: tuple[int, int] | None = None  # (height, width) of the frames
        self._last: np.ndarray | None = None

    def add(self, frame: np.ndarray) -> None:
        self.shape = self.shape or frame.shape[:2]
        step = max(1, max(frame.shape[:2]) // 160)  # compare a coarse copy: cheap, and plenty for spotting motion
        small = frame[::step, ::step].mean(axis=2)
        if self._last is not None:
            self.energy.append(float(np.abs(small - self._last).mean()))
        self._last = small

    def frozen_share(self) -> float:
        """The share of the clip after its last visible movement (movement = at least 5% of the peak)."""
        energy = np.asarray(self.energy)
        if len(energy) == 0 or energy.max() <= 0:
            return 1.0
        last_moving = int(np.nonzero(energy >= 0.05 * energy.max())[0].max())
        return 1.0 - (last_moving + 1) / len(energy)

    def warnings(self, frozen_tail: float = 0.3) -> list[str]:
        if len(self.energy) < 10:
            return []
        if max(self.energy) <= 1e-6:
            return ["nothing moves in the clip"]
        share = self.frozen_share()
        if share > frozen_tail:
            return [f"the motion stops early: the last {share:.0%} of the clip is frozen"]
        return []
