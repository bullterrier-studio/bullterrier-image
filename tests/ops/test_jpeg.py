"""Tests of `ops/jpeg.py`: JPEG's lossy core as maths on a float picture, checked against values worked out by hand from
ITU-T T.81 (the Annex K tables, the level shift, the 8x8 blocks) and the IJG quality rule."""

import numpy as np
import pytest

from bullterrier_image.ops.colorspace import rgb_to_ycbcr
from bullterrier_image.ops.jpeg import decode, encode

# The quantisation step of table entry [row, column] after the IJG quality rule (scale = 5000 / quality below 50, else
# 200 - 2 * quality; step = (table * scale + 50) // 100, held to 1..255), worked out by hand from T.81 tables K.1, K.2.
STEPS = [
    pytest.param(50, 0, (0, 0), 16, id="quality 50 is table K.1 itself: 16 at the corner"),
    pytest.param(50, 0, (0, 1), 11, id="quality 50: 11 beside it"),
    pytest.param(50, 1, (0, 0), 17, id="quality 50 chroma is table K.2: 17 at the corner"),
    pytest.param(50, 1, (7, 7), 99, id="quality 50 chroma: 99 at the far corner"),
    pytest.param(100, 0, (7, 7), 1, id="quality 100 divides by 1 everywhere: (99 * 0 + 50) // 100 is 0, held to 1"),
    pytest.param(90, 0, (0, 0), 3, id="quality 90: scale 20, (16 * 20 + 50) // 100 = 3"),
    pytest.param(10, 0, (0, 0), 80, id="quality 10: scale 500, (16 * 500 + 50) // 100 = 80"),
    pytest.param(1, 0, (7, 7), 255, id="quality 1: scale 5000, held to 255"),
    pytest.param(25, 1, (0, 1), 36, id="quality 25: scale 200, 18 * 2 = 36"),
    pytest.param(90, 0, (1, 3), 4, id="quality 90: scale 20, (19 * 20 + 50) // 100 = 4, the + 50 rounds to nearest"),
]

# Tables K.1 and K.2 of ITU-T T.81 as printed in the standard: at quality 50 the steps are the tables themselves.
TABLE_K1 = [
    [16, 11, 10, 16, 24, 40, 51, 61],
    [12, 12, 14, 19, 26, 58, 60, 55],
    [14, 13, 16, 24, 40, 57, 69, 56],
    [14, 17, 22, 29, 51, 87, 80, 62],
    [18, 22, 37, 56, 68, 109, 103, 77],
    [24, 35, 55, 64, 81, 104, 113, 92],
    [49, 64, 78, 87, 103, 121, 120, 101],
    [72, 92, 95, 98, 112, 100, 103, 99],
]
TABLE_K2 = [
    [17, 18, 24, 47, 99, 99, 99, 99],
    [18, 21, 26, 66, 99, 99, 99, 99],
    [24, 26, 56, 99, 99, 99, 99, 99],
    [47, 66, 99, 99, 99, 99, 99, 99],
    *[[99] * 8] * 4,
]


@pytest.mark.parametrize(("quality", "plane", "entry", "step"), STEPS)
def test_the_quantisation_steps_follow_the_annex_k_tables_and_the_ijg_quality_rule(quality, plane, entry, step) -> None:
    coded = encode(np.zeros((8, 8, 3), np.float32), quality=quality)
    assert coded.steps[plane][entry] == step


def test_at_quality_50_the_steps_are_the_two_tables_of_annex_k() -> None:
    luma, cb, cr = encode(np.zeros((8, 8, 3), np.float32), quality=50).steps
    np.testing.assert_array_equal(luma, TABLE_K1)
    np.testing.assert_array_equal(cb, TABLE_K2)
    np.testing.assert_array_equal(cr, TABLE_K2)


def test_a_flat_grey_picture_has_only_dc_values_worked_out_by_hand() -> None:
    """Level shift 128, orthonormal 8x8 DCT: a flat block of 8-bit value v has DC 8 * (v - 128) and nothing else; at
    quality 50 the DC step is 16, so v = 148 gives 8 * 20 / 16 = 10. Grey has no chroma: its Cb and Cr blocks are 0."""
    coded = encode(np.full((20, 30, 3), 148 / 255, np.float32), quality=50)
    luma, cb, cr = coded.planes
    assert luma.shape == (4, 4, 8, 8)  # 20 x 30 is padded to 32 x 32, four 8 x 8 blocks each way
    assert cb.shape == cr.shape == (2, 2, 8, 8)  # chroma is halved each way: 16 x 16
    np.testing.assert_array_equal(luma[..., 0, 0], 10)
    assert not luma[..., 1:, :].any()
    assert not luma[..., 0, 1:].any()
    assert not cb.any()
    assert not cr.any()


def test_decoding_gives_the_picture_back_at_its_own_size_inside_0_to_1() -> None:
    coded = encode(np.full((13, 21, 3), 0.9, np.float32), quality=50)
    coded.planes[0][..., 0, 0] += 500  # far brighter than white can be
    out = decode(coded)
    assert out.shape == (13, 21, 3)
    assert out.dtype == np.float32
    assert out.max() == 1.0


def test_chroma_is_halved_each_way_so_a_one_pixel_colour_detail_is_averaged_while_luma_stays() -> None:
    """Columns alternating between two colours: 4:2:0 gives every 2 x 2 patch the mean chroma of the two, and luma,
    which is kept at full resolution, still tells the columns apart. Quality 100: the quantiser adds almost nothing."""
    a, b = np.array([1.0, 0.2, 0.1], np.float32), np.array([0.1, 0.3, 0.9], np.float32)
    image = np.tile(np.stack([a, b])[None], (16, 8, 1))  # 16 x 16, columns a, b, a, b...
    out = rgb_to_ycbcr(decode(encode(image, quality=100)))
    ycbcr_a, ycbcr_b = rgb_to_ycbcr(a), rgb_to_ycbcr(b)
    np.testing.assert_allclose(out[..., 1:], np.broadcast_to((ycbcr_a[1:] + ycbcr_b[1:]) / 2, (16, 16, 2)), atol=0.01)
    np.testing.assert_allclose(out[:, 0::2, 0], np.full((16, 8), ycbcr_a[0]), atol=0.01)
    np.testing.assert_allclose(out[:, 1::2, 0], np.full((16, 8), ycbcr_b[0]), atol=0.01)
