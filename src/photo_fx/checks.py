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
