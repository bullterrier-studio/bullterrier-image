"""Maps of a photo: one value per pixel, 0..1."""

import numpy as np


def map_problem(layer: np.ndarray) -> str | None:
    """What is wrong with the values of the map `layer`, with its least and greatest value; None when they are finite
    and within 0..1. The one rule for the values of a map that enters from outside (a map given or found, a piece
    finder's answer, a field array handed to an effect)."""
    finite, low, high = bool(np.isfinite(layer).all()), float(layer.min()), float(layer.max())
    problem = "that are not finite" if not finite else "below 0" if low < 0 else "above 1" if high > 1 else None
    return None if problem is None else f"values {problem} (min {low:g}, max {high:g})"
