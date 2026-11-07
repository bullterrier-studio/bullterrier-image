"""The `tone_map` presets, resolved by `fx`. The kernel is tested in `crates/core/tests/it/tone_map.rs`."""

import numpy as np

from bullterrier_image import Recipe, fx, render


def flat(colour: float, shape: tuple[int, int]) -> np.ndarray:
    return np.broadcast_to(np.array(colour, np.float32), (*shape, 3)).copy()


def two_blocks(dark: float, bright: float, shape: tuple[int, int]) -> np.ndarray:
    """Left half `dark`, right half `bright`."""
    picture = flat(bright, shape)
    picture[:, : shape[1] // 2] = dark
    return picture


def test_the_shadows_highlights_preset_lifts_dark_areas_and_lowers_bright_ones() -> None:
    """Far from their edge, a dark half (0.05) comes out brighter and a bright half (0.9) darker."""
    out = render(
        two_blocks(0.05, 0.9, (16, 128)),
        Recipe(name="look", size=128, steps=[fx.tone_map(preset="shadows_highlights")]),
    )
    assert out[8, 5, 0] > 0.05 * 1.3
    assert out[8, 122, 0] < 0.9 * 0.95
