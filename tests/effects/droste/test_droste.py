"""Behaviour of the `droste` effect: the picture shown again, smaller, in its own middle, over and over."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "droste"
HEIGHT, WIDTH = 41, 61  # odd, so the middle is a pixel: row 20, column 30
MIDDLE = (HEIGHT // 2, WIDTH // 2)


@pytest.fixture
def noise() -> np.ndarray:
    return np.random.default_rng(6).random((HEIGHT, WIDTH, 3), dtype=np.float32)


def copies(scale: float, repeats: int) -> np.ndarray:
    """How many times each pixel is inside a smaller copy, worked out by counting: a pixel at `size` (its distance from
    the middle as a share of the picture's half-extent, the picture's edge being 1) is inside a copy while it is closer
    than `scale`, and in the next copy that copy's picture has it `size / scale` out."""
    rows, cols = np.mgrid[0:HEIGHT, 0:WIDTH]
    size = np.maximum(np.abs(rows - MIDDLE[0]) / MIDDLE[0], np.abs(cols - MIDDLE[1]) / MIDDLE[1])
    count = np.zeros(size.shape, dtype=int)
    while True:
        inside = (size < scale) & (count < repeats)
        if not inside.any():
            return count
        size = np.where(inside, size / scale, size)
        count += inside


@pytest.mark.parametrize("repeats", [1, 2, 6])
def test_the_picture_is_shown_scaled_down_in_its_own_middle_as_many_times_as_it_is_asked_to_repeat(
    noise, repeats
) -> None:
    """With `scale` 0.5 and no turn, a pixel inside k copies shows the picture 2^k times farther from the middle than it
    is itself. Pixels whose size is exactly 1/2, 1/4... sit on the border between two copies and are left out."""
    out = registry.apply(NAME, noise, scale=0.5, turn=0, repeats=repeats)
    count = copies(0.5, repeats)
    rows, cols = np.mgrid[0:HEIGHT, 0:WIDTH]
    away = 2.0**count
    source_rows = np.rint(MIDDLE[0] + (rows - MIDDLE[0]) * away).astype(int)
    source_cols = np.rint(MIDDLE[1] + (cols - MIDDLE[1]) * away).astype(int)
    size = np.maximum(np.abs(rows - MIDDLE[0]) / MIDDLE[0], np.abs(cols - MIDDLE[1]) / MIDDLE[1])
    levels = np.log2(np.where(size > 0, size, 1))
    checked = ~np.isclose(levels, np.rint(levels))  # leave out the sizes 1/2, 1/4, ... and the middle pixel's 0
    expected = noise[source_rows.clip(0, HEIGHT - 1), source_cols.clip(0, WIDTH - 1)]
    np.testing.assert_allclose(out[checked], expected[checked], atol=1e-6)
    assert count.max() == repeats  # the middle of the picture is as deep as it is allowed to be
    assert (count == 0).any()  # and the outside of the picture is the picture as it is


@pytest.mark.parametrize(
    ("turn", "repeats", "edge", "pad_mode"),
    [
        (90, 1, "reflect", "symmetric"),
        (90, 1, "nearest", "edge"),
        (90, 1, "wrap", "wrap"),
        (90, 1, "mirror", "reflect"),
        (180, 1, "reflect", "symmetric"),
        (-90, 1, "reflect", "symmetric"),
        (90, 3, "reflect", "symmetric"),
        (60, 2, "reflect", "symmetric"),
    ],
    ids=[
        "90",
        "90, edge repeated",
        "90, wrapping",
        "90, mirrored",
        "180",
        "-90",
        "90 over three copies",
        "60 over two",
    ],
)
def test_each_copy_is_turned_by_turn_degrees_from_the_one_around_it_and_reads_beyond_the_edge_by_the_edge_rule(
    noise, turn, repeats, edge, pad_mode
) -> None:
    """A pixel `d` from the middle, in a picture k copies deep, shows the picture at 2^k d turned by k x `turn` degrees:
    (row, column) offsets (r, c) become (r cos t - c sin t, r sin t + c cos t), t the angle as the picture's rows run
    down. A turned copy's source can lie beyond the picture, where the oracle pads it with numpy's rule for the edge."""
    out = registry.apply(NAME, noise, scale=0.5, turn=turn, repeats=repeats, edge=edge, interpolation="nearest")
    count = copies(0.5, repeats)
    rows, cols = np.mgrid[0:HEIGHT, 0:WIDTH]
    r, c = rows - MIDDLE[0], cols - MIDDLE[1]
    size = np.maximum(np.abs(r) / MIDDLE[0], np.abs(c) / MIDDLE[1])
    levels = np.log2(np.where(size > 0, size, 1))
    checked = ~np.isclose(levels, np.rint(levels))  # leave out the borders between copies
    angle = np.radians(turn) * count
    away = 2.0**count
    source_rows = np.rint(MIDDLE[0] + away * (r * np.cos(angle) - c * np.sin(angle))).astype(int)
    source_cols = np.rint(MIDDLE[1] + away * (r * np.sin(angle) + c * np.cos(angle))).astype(int)
    pad = 30
    padded = np.pad(noise, ((pad, pad), (pad, pad), (0, 0)), mode=pad_mode)
    expected = padded[source_rows.clip(-pad, HEIGHT + pad - 1) + pad, source_cols.clip(-pad, WIDTH + pad - 1) + pad]
    assert (count[checked] > 0).sum() > 100
    np.testing.assert_allclose(out[checked], expected[checked], atol=1e-6)
