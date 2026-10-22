"""Behaviour of photo_fx.ops.warp, checked against independent oracles (np.roll, np.pad, hand-computed values)."""

from typing import Literal

import numpy as np
import pytest

from photo_fx.ops.warp import pixel_grid, remap

MODES: list[Literal["clamp", "mirror", "wrap", "fill"]] = ["clamp", "mirror", "wrap", "fill"]


def test_pixel_grid_is_column_and_row_index():
    x, y = pixel_grid((3, 5))
    assert x.shape == y.shape == (3, 5)
    assert x.dtype == y.dtype == np.float32
    assert x.tolist() == [[0, 1, 2, 3, 4]] * 3
    assert y.tolist() == [[0] * 5, [1] * 5, [2] * 5]


def _picture(h=7, w=9, channels=3, seed=0):
    rng = np.random.default_rng(seed)
    shape = (h, w) if channels is None else (h, w, channels)
    return rng.random(shape, dtype=np.float32)


@pytest.mark.parametrize("mode", MODES)
@pytest.mark.parametrize("channels", [None, 3])
def test_zero_displacement_is_the_identity_and_leaves_input_untouched(mode, channels):
    image = _picture(channels=channels)
    before = image.copy()
    zero = np.zeros(image.shape[:2], np.float32)

    out = remap(image, zero, zero, mode=mode, fill=0.5)

    np.testing.assert_array_equal(out, image)
    assert out.dtype == np.float32
    assert out.shape == image.shape
    assert out is not image
    np.testing.assert_array_equal(image, before)


def test_field_of_the_wrong_shape_is_rejected_naming_the_mismatch():
    image = _picture(7, 9)
    good = np.zeros((7, 9), np.float32)
    bad = np.zeros((9, 7), np.float32)
    with pytest.raises(ValueError, match=r"dx.*\(9, 7\).*\(7, 9\)"):
        remap(image, bad, good)
    with pytest.raises(ValueError, match=r"dy.*\(9, 7\).*\(7, 9\)"):
        remap(image, good, bad)


def test_unknown_mode_is_rejected_listing_the_valid_ones():
    zero = np.zeros((7, 9), np.float32)
    with pytest.raises(ValueError, match=r"clamp.*mirror.*wrap.*fill"):
        remap(_picture(7, 9), zero, zero, mode="bounce")  # type: ignore[arg-type]  # deliberately invalid: the runtime check is under test


@pytest.mark.parametrize("bad_value", [np.nan, np.inf, -np.inf])
def test_non_finite_displacement_is_rejected(bad_value):
    zero = np.zeros((7, 9), np.float32)
    bad = zero.copy()
    bad[3, 4] = bad_value
    with pytest.raises(ValueError, match="finite"):
        remap(_picture(7, 9), bad, zero)
    with pytest.raises(ValueError, match="finite"):
        remap(_picture(7, 9), zero, bad)


def test_image_that_is_neither_2d_nor_3d_is_rejected():
    zero = np.zeros((7, 9), np.float32)
    with pytest.raises(ValueError, match="image"):
        remap(np.zeros((7, 9, 3, 2), np.float32), zero, zero)
    with pytest.raises(ValueError, match="image"):
        remap(np.zeros(9, np.float32), zero, zero)


def _shift(image, sx, sy, **kwargs):
    """Warp so that the content moves by (+sx, +sy) pixels: output[r, c] = image[r - sy, c - sx]."""
    h, w = image.shape[:2]
    return remap(image, np.full((h, w), -sx, np.float32), np.full((h, w), -sy, np.float32), **kwargs)


@pytest.mark.parametrize(("sx", "sy"), [(2, 0), (0, 3), (-3, 0), (0, -2), (4, -3), (-5, 6)])
def test_wrap_integer_shift_equals_np_roll(sx, sy):
    image = _picture(7, 9)
    np.testing.assert_array_equal(_shift(image, sx, sy, mode="wrap"), np.roll(image, (sy, sx), axis=(0, 1)))


@pytest.mark.parametrize(("sx", "sy"), [(2, 0), (0, 3), (-3, 0), (0, -2), (4, -3), (-5, 6)])
def test_clamp_integer_shift_equals_a_window_of_the_edge_padded_input(sx, sy):
    image = _picture(7, 9)
    h, w = image.shape[:2]
    padded = np.pad(image, ((6, 6), (6, 6), (0, 0)), mode="edge")
    expected = padded[6 - sy : 6 - sy + h, 6 - sx : 6 - sx + w]
    np.testing.assert_array_equal(_shift(image, sx, sy, mode="clamp"), expected)


@pytest.mark.parametrize(("sx", "sy"), [(2, 0), (0, 3), (-3, 0), (0, -2), (4, -3), (-5, 6)])
def test_mirror_integer_shift_equals_a_window_of_the_symmetric_padded_input(sx, sy):
    image = _picture(7, 9)
    h, w = image.shape[:2]
    padded = np.pad(image, ((6, 6), (6, 6), (0, 0)), mode="symmetric")
    expected = padded[6 - sy : 6 - sy + h, 6 - sx : 6 - sx + w]
    np.testing.assert_array_equal(_shift(image, sx, sy, mode="mirror"), expected)


@pytest.mark.parametrize(("sx", "sy"), [(2, 0), (0, 3), (-3, 0), (0, -2), (4, -3)])
def test_fill_integer_shift_keeps_shifted_input_and_fills_exactly_the_uncovered_border(sx, sy):
    image = _picture(7, 9)
    h, w = image.shape[:2]
    out = _shift(image, sx, sy, mode="fill", fill=0.37)

    expected = np.full_like(image, 0.37)
    src_rows, src_cols = slice(max(0, -sy), h - max(0, sy)), slice(max(0, -sx), w - max(0, sx))
    dst_rows, dst_cols = slice(max(0, sy), h - max(0, -sy)), slice(max(0, sx), w - max(0, -sx))
    expected[dst_rows, dst_cols] = image[src_rows, src_cols]
    np.testing.assert_array_equal(out, expected)


def test_huge_displacements_stay_periodic_in_wrap_and_reflected_in_mirror():
    image = _picture(7, 9)
    h, w = image.shape[:2]
    big_x, big_y = 1_000_003, -2_000_001  # not multiples of the picture size
    wrapped = _shift(image, big_x, big_y, mode="wrap")
    np.testing.assert_array_equal(wrapped, np.roll(image, (big_y % h, big_x % w), axis=(0, 1)))
    # mirror has period 2n, so a shift by exactly 2n pixels is the identity
    np.testing.assert_array_equal(_shift(image, 2 * w * 1000, 2 * h * 1000, mode="mirror"), image)
    far = _shift(image, 10**6, 10**6, mode="clamp")
    np.testing.assert_array_equal(far, np.broadcast_to(image[0, 0], image.shape))


def test_half_pixel_shift_in_x_is_the_mean_of_horizontal_neighbours():
    image = _picture(7, 9)
    half = np.full((7, 9), 0.5, np.float32)
    out = remap(image, half, np.zeros_like(half))
    np.testing.assert_allclose(out[:, :-1], (image[:, :-1] + image[:, 1:]) / 2, atol=1e-6)


def test_quarter_pixel_displacement_on_a_2x2_patch_matches_the_hand_computed_bilinear_values():
    a, b, c, d = 0.0, 1.0, 0.5, 0.25  # [[a, b], [c, d]], all different so any swap shows
    patch = np.array([[a, b], [c, d]], np.float32)
    dx = np.full((2, 2), 0.25, np.float32)
    dy = np.full((2, 2), 0.125, np.float32)  # different from dx: a swapped dx/dy gives another number

    out = remap(patch, dx, dy)  # clamp: beyond the last row/column the edge pixel repeats

    # each pixel samples (x + 0.25, y + 0.125): blend along x within each row, then down the rows
    expected = np.array(
        [
            [(a * 0.75 + b * 0.25) * 0.875 + (c * 0.75 + d * 0.25) * 0.125, b * 0.875 + d * 0.125],
            [c * 0.75 + d * 0.25, d],
        ]
    )
    np.testing.assert_allclose(out, expected, atol=1e-6)
    assert np.abs(remap(patch, dy, dx) - expected).max() > 1e-2  # the swapped field is a different picture


@pytest.mark.parametrize(("pad_mode", "mode"), [("symmetric", "mirror"), ("wrap", "wrap")])
@pytest.mark.parametrize(("sx", "sy"), [(3.25, 0.0), (-2.75, 0.0), (0.0, 4.5), (-3.5, 2.25)])
def test_fractional_shift_across_the_edge_interpolates_with_what_lies_beyond(pad_mode, mode, sx, sy):
    image = _picture(7, 9)
    h, w = image.shape[:2]
    pad = 8
    padded = np.pad(image, ((pad, pad), (pad, pad), (0, 0)), mode=pad_mode)

    # content moves by (+sx, +sy): output[r, c] samples position (c - sx, r - sy) of the padded picture
    fx, fy = np.floor(-sx), np.floor(-sy)
    ax, ay = -sx - fx, -sy - fy  # weights of the far neighbour along each axis

    def window(ox, oy):
        r0, c0 = pad + int(fy) + oy, pad + int(fx) + ox
        return padded[r0 : r0 + h, c0 : c0 + w]

    top = window(0, 0) * (1 - ax) + window(1, 0) * ax
    bottom = window(0, 1) * (1 - ax) + window(1, 1) * ax
    expected = top * (1 - ay) + bottom * ay

    out = _shift(image, sx, sy, mode=mode)

    np.testing.assert_allclose(out, expected, atol=1e-6)


def test_sub_pixel_positions_on_a_very_wide_picture_are_not_rounded_to_float32():
    # In float32, x = 33000 + 0.3 rounds to a multiple of 1/256, so a coordinate built in float32 would be off by up to
    # ~0.002 px, which on this sawtooth (slope 1/64 per pixel) is a visible 3e-5 error in the value.
    w = 40_000
    column = np.arange(w)
    image = ((column % 64) / 64).astype(np.float32)[None, :]
    dx = np.full((1, w), 0.3, np.float32)

    out = remap(image, dx, np.zeros_like(dx))

    inside_tooth = (column % 64) < 63  # the sawtooth drops back to 0 after 63, which is not linear
    expected = (((column % 64) + np.float64(np.float32(0.3))) / 64)[inside_tooth]
    assert np.abs(out[0, inside_tooth] - expected).max() < 3e-6


def test_displacement_in_dx_alone_moves_content_only_along_x():
    image = _picture(7, 9)
    zero = np.zeros((7, 9), np.float32)
    out_x = remap(image, np.full((7, 9), 2.0, np.float32), zero, mode="wrap")
    np.testing.assert_array_equal(out_x, np.roll(image, -2, axis=1))
    out_y = remap(image, zero, np.full((7, 9), 2.0, np.float32), mode="wrap")
    np.testing.assert_array_equal(out_y, np.roll(image, -2, axis=0))


@pytest.mark.parametrize("mode", MODES)
@pytest.mark.parametrize("seed", range(3))
def test_constant_image_stays_constant_under_any_field(mode, seed):
    rng = np.random.default_rng(seed)
    image = np.full((20, 30, 3), 0.6, np.float32)
    dx, dy = (rng.normal(0, 12, (20, 30)).astype(np.float32) for _ in range(2))
    out = remap(image, dx, dy, mode=mode, fill=0.6)
    np.testing.assert_allclose(out, 0.6, atol=1e-6)


@pytest.mark.parametrize("mode", MODES)
@pytest.mark.parametrize("seed", range(3))
def test_output_never_leaves_the_input_range(mode, seed):
    rng = np.random.default_rng(seed)
    image = rng.random((30, 40, 3), dtype=np.float32) * 0.5 + 0.25  # in [0.25, 0.75]
    dx, dy = (rng.normal(0, 8, (30, 40)).astype(np.float32) for _ in range(2))
    out = remap(image, dx, dy, mode=mode, fill=0.5)
    assert out.min() >= image.min() - 1e-6
    assert out.max() <= image.max() + 1e-6


def test_warping_then_unwarping_a_smooth_image_returns_it_in_the_interior():
    h, w = 64, 80
    y, x = np.mgrid[0:h, 0:w].astype(np.float32)
    image = (0.5 + 0.3 * np.sin(x / 40) * np.cos(y / 30))[..., None] * np.ones(3, np.float32)
    dx = (2.0 * np.sin(y / 9)).astype(np.float32)
    dy = (1.5 * np.cos(x / 11)).astype(np.float32)

    # sampling at p + d(p) and then at p - d(p) only inverts exactly for a constant field; with a smooth one the
    # error is the field's gradient times its size, which stays small for a low-frequency picture
    out = remap(remap(image, dx, dy), -dx, -dy)

    interior = (slice(8, -8), slice(8, -8))
    assert np.abs(out[interior] - image[interior]).max() < 1e-2


def test_a_2d_matte_warps_exactly_like_one_channel_of_a_colour_picture():
    rng = np.random.default_rng(5)
    rgb = _picture(20, 30)
    dx, dy = (rng.normal(0, 3, (20, 30)).astype(np.float32) for _ in range(2))
    for mode in MODES:
        warped_rgb = remap(rgb, dx, dy, mode=mode, fill=0.2)
        for ch in range(3):
            np.testing.assert_array_equal(remap(rgb[..., ch], dx, dy, mode=mode, fill=0.2), warped_rgb[..., ch])


def test_single_channel_and_single_pixel_pictures_work():
    rng = np.random.default_rng(6)
    image = _picture(10, 12, channels=None)
    dx, dy = (rng.normal(0, 3, (10, 12)).astype(np.float32) for _ in range(2))
    np.testing.assert_array_equal(
        remap(image[..., None], dx, dy, mode="mirror")[..., 0], remap(image, dx, dy, mode="mirror")
    )

    one = np.array([[0.7]], np.float32)
    shift = np.full((1, 1), 3.3, np.float32)
    for mode in ("clamp", "mirror", "wrap"):
        np.testing.assert_allclose(remap(one, shift, shift, mode=mode), one, atol=1e-6)
    np.testing.assert_allclose(remap(one, shift, shift, mode="fill", fill=0.1), 0.1, atol=1e-6)


def test_float64_and_non_contiguous_inputs_give_the_same_float32_result():
    rng = np.random.default_rng(7)
    big = rng.random((30, 40, 3))  # float64
    view = big[::2, ::2]  # non-contiguous, shape (15, 20, 3)
    dx, dy = (rng.normal(0, 3, (15, 20)).astype(np.float32) for _ in range(2))

    out = remap(view, dx, dy, mode="wrap")
    reference = remap(np.ascontiguousarray(view, dtype=np.float32), dx, dy, mode="wrap")

    assert out.dtype == np.float32
    np.testing.assert_array_equal(out, reference)


def test_fill_blends_smoothly_across_the_picture_edge():
    image = np.full((4, 5), 0.8, np.float32)
    dx = np.full((4, 5), 0.5, np.float32)  # the last column samples half way between the edge pixel and outside
    out = remap(image, dx, np.zeros_like(dx), mode="fill", fill=0.2)
    assert out[:, -1] == pytest.approx(np.full(4, (0.8 + 0.2) / 2), abs=1e-6)
    np.testing.assert_allclose(out[:, :-1], 0.8, atol=1e-6)
