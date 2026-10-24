"""Tests of `ops/place.py`: a picture fitted to a photo, as RGBA at the photo's size.

The oracles are hand-made: with `resample="nearest"` and whole-number scale factors a placed picture is `np.repeat` of
the picture, so every expectation is built with numpy's own repeat, tile, roll and slicing.
"""

from typing import Any

import numpy as np
import pytest

from bullterrier_image.ops.place import place

RED, BLUE = (1.0, 0.0, 0.0, 1.0), (0.0, 0.0, 1.0, 1.0)


def picture(*rows) -> np.ndarray:
    """A picture from rows of RGBA pixels."""
    return np.array(rows, np.float32)


def box(placed: np.ndarray) -> tuple[int, int, int, int]:
    """(top, left, height, width) of the part of `placed` that is not transparent."""
    rows, columns = np.nonzero(placed[..., 3] > 0)
    return int(rows.min()), int(columns.min()), int(np.ptp(rows)) + 1, int(np.ptp(columns)) + 1


def fit(pic, shape, **settings: Any) -> np.ndarray:
    chosen: dict[str, Any] = {"fit": "contain", "at": "center", "scale": 1.0, "margin": 0.0, "resample": "nearest"}
    return place(pic, shape, **(chosen | settings))


SQUARE = np.ones((5, 5, 4), np.float32)


@pytest.mark.parametrize(
    ("at", "top", "left"),
    [
        ("top_left", 0, 0),
        ("top", 0, 5),
        ("top_right", 0, 10),
        ("left", 5, 0),
        ("center", 5, 5),
        ("right", 5, 10),
        ("bottom_left", 10, 0),
        ("bottom", 10, 5),
        ("bottom_right", 10, 10),
    ],
)
def test_a_picture_sits_at_one_of_nine_anchors_in_the_free_space(at, top, left):
    # a 5 x 5 picture contained in 20 x 20 is 20 x 20; at half scale it is 10 x 10 with 10 px of free space each way
    assert box(fit(SQUARE, (20, 20), at=at, scale=0.5)) == (top, left, 10, 10)


def test_the_margin_is_a_share_of_the_shorter_side_kept_clear_of_the_picture():
    placed = fit(SQUARE, (40, 60), margin=0.1)  # 4 px of 40; the area is 32 x 52, so a square picture is 32 x 32
    assert box(placed) == (4, 14, 32, 32)
    assert box(fit(SQUARE, (40, 60), margin=0.1, at="top_left")) == (4, 4, 32, 32)


def test_contain_keeps_the_aspect_and_leaves_the_rest_transparent():
    wide = np.ones((2, 4, 4), np.float32)
    placed = fit(wide, (12, 12), fit="contain", at="top_left")
    assert box(placed) == (0, 0, 6, 12)
    assert placed[6:].max() == 0


def test_width_sets_the_pictures_width_to_the_photos_times_scale_whatever_its_aspect():
    tall = np.ones((8, 2, 4), np.float32)
    placed = fit(tall, (80, 20), fit="width", scale=0.5, at="top_left")  # 10 px wide, so 40 px tall
    assert box(placed) == (0, 0, 40, 10)
    assert box(fit(tall, (80, 20), fit="width", scale=0.5, at="top_left", margin=0.1))[2:] == (
        40,
        10,
    )  # not the margin's


def test_cover_fills_the_photo_and_crops_what_hangs_over_at_the_anchor():
    two_colours = picture([RED, RED, BLUE, BLUE], [RED, RED, BLUE, BLUE])  # 2 rows, 4 columns
    placed = fit(two_colours, (4, 4), fit="cover")  # doubled: 4 x 8, of which the middle 4 columns show
    np.testing.assert_array_equal(placed[0], picture([RED, RED, BLUE, BLUE])[0])
    assert placed[..., 3].min() == 1
    left = fit(two_colours, (4, 4), fit="cover", at="left")
    np.testing.assert_array_equal(left[0, :, :3], np.array([RED, RED, RED, RED], np.float32)[:, :3])


def test_stretch_fills_the_area_whatever_the_aspect():
    quarters = picture([RED, BLUE], [BLUE, RED])
    expected = np.repeat(np.repeat(quarters, 2, axis=0), 3, axis=1)
    np.testing.assert_array_equal(fit(quarters, (4, 6), fit="stretch"), expected)


def test_tile_repeats_the_picture_with_one_tile_at_the_anchor():
    quarters = picture([RED, BLUE], [BLUE, RED])
    one_tile = np.repeat(np.repeat(quarters, 2, axis=0), 2, axis=1)  # 4 x 4: half the photo's width, so scale 0.5
    corner = np.tile(one_tile, (2, 2, 1))
    np.testing.assert_array_equal(fit(quarters, (8, 8), fit="tile", scale=0.5, at="top_left"), corner)
    np.testing.assert_array_equal(fit(quarters, (8, 8), fit="tile", scale=0.5), np.roll(corner, (2, 2), axis=(0, 1)))


def test_transparent_pixels_lend_the_resampled_edge_no_colour():
    half = picture([RED, (0.0, 0.0, 0.0, 0.0)])  # an opaque red pixel beside a clear one, whose colour is black
    placed = fit(half, (8, 16), fit="stretch", resample="lanczos")
    seen = placed[placed[..., 3] > 0.01]
    np.testing.assert_allclose(seen[:, :3], [[1, 0, 0]] * len(seen), atol=1e-5)
    assert ((placed[..., 3] > 0) & (placed[..., 3] < 1)).any()  # the edge between them is soft: resampled, not cut


@pytest.mark.parametrize("resample", ["nearest", "bilinear", "bicubic", "lanczos"])
@pytest.mark.parametrize("shape", [(1, 1), (1, 9), (31, 47)])
def test_every_resampling_gives_a_float_rgba_picture_of_the_photos_size_in_range(shape, resample):
    source = np.random.default_rng(0).random((6, 9, 4), dtype=np.float32)
    before = source.copy()
    placed = fit(source, shape, fit="cover", margin=0.45, resample=resample)
    assert placed.shape == (*shape, 4)
    assert placed.dtype == np.float32
    assert placed.min() >= 0
    assert placed.max() <= 1
    np.testing.assert_array_equal(source, before)
