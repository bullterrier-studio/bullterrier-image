"""Behaviour of the `jpeg_crush` effect: the picture put through JPEG's lossy steps at a low quality, then `hits`
coefficients damaged at random, as a corrupted file shows it."""

import numpy as np
import pytest

from bullterrier_image.core import registry
from bullterrier_image.ops.jpeg import decode, encode

NAME = "jpeg_crush"


def grey(levels: np.ndarray) -> np.ndarray:
    """A picture whose red, green and blue are all the 8-bit `levels` (height, width)."""
    return np.repeat(levels[..., None], 3, axis=-1).astype(np.float32) / 255


def blocks_of_flat_grey(levels: list[list[int]]) -> np.ndarray:
    """A picture made of flat 8 x 8 blocks of the given 8-bit grey levels, one per entry."""
    return grey(np.kron(np.array(levels), np.ones((8, 8), dtype=int)))


def cosine_block(vertical: int, horizontal: int, coefficient: float) -> np.ndarray:
    """The 8 x 8 block of samples whose orthonormal DCT is `coefficient` at one place and zero elsewhere, by the
    definition: coefficient * a(v) * a(u) * cos((2y + 1) v pi / 16) * cos((2x + 1) u pi / 16), with a(0) = sqrt(1/8) and
    a(k) = sqrt(2/8) = 1/2 for the rest."""
    y, x = np.mgrid[0:8, 0:8]

    def scale(k: int) -> float:
        return np.sqrt(1 / 8) if k == 0 else 0.5

    return (
        coefficient
        * scale(vertical)
        * scale(horizontal)
        * np.cos((2 * y + 1) * vertical * np.pi / 16)
        * np.cos((2 * x + 1) * horizontal * np.pi / 16)
    )


def mean_error(before: np.ndarray, after: np.ndarray) -> float:
    return float(np.abs(before - after).mean())


@pytest.mark.parametrize("quality", [20, 50, 75])
def test_flat_blocks_survive_exactly_when_their_dc_value_is_a_whole_number_of_steps(quality) -> None:
    """A flat block of 8-bit value v has DC 8 * (v - 128), nothing else. The DC steps at these qualities are 40, 16 and
    8, so offsets from 128 that are multiples of 5 make every DC a whole number of steps: no rounding, no change."""
    picture = blocks_of_flat_grey([[128, 138], [188, 78]])
    np.testing.assert_allclose(registry.apply(NAME, picture, quality=quality), picture, atol=1e-5)


def test_flat_picture_of_a_size_that_does_not_fill_its_blocks_survives_exactly_and_keeps_its_size() -> None:
    picture = grey(np.full((13, 21), 138))
    out = registry.apply(NAME, picture, quality=50)
    assert out.shape == picture.shape
    np.testing.assert_allclose(out, picture, atol=1e-5)


@pytest.mark.parametrize(
    ("vertical", "horizontal", "coefficient", "kept"),
    [
        pytest.param(0, 1, 96, 99, id="(0, 1): step 11, 96 / 11 = 8.7 rounds to 9 steps = 99"),
        pytest.param(1, 0, 50, 48, id="(1, 0): step 12, 50 / 12 = 4.2 rounds to 4 steps = 48"),
        pytest.param(1, 1, 130, 132, id="(1, 1): step 12, 130 / 12 = 10.8 rounds to 11 steps = 132"),
        pytest.param(3, 2, 200, 198, id="(3, 2): step 22, 200 / 22 = 9.1 rounds to 9 steps = 198"),
    ],
)
def test_one_cosine_pattern_comes_back_with_its_coefficient_rounded_to_the_annex_k_step(
    vertical, horizontal, coefficient, kept
) -> None:
    """At quality 50 the steps are table K.1 itself. A block of one cosine has one coefficient; the output block is the
    cosine of the coefficient rounded to a whole number of steps (`kept`), and the blocks beside it stay flat grey."""
    levels = np.full((16, 16), 128.0)
    levels[:8, :8] += cosine_block(vertical, horizontal, coefficient)
    expected = levels.copy()
    expected[:8, :8] = 128 + cosine_block(vertical, horizontal, kept)
    out = registry.apply(NAME, grey(levels), quality=50)
    np.testing.assert_allclose(out, grey(expected), atol=1e-4)


def test_a_slow_ramp_becomes_steps_that_change_only_on_the_8_px_block_grid() -> None:
    """Quality 10 has a DC step of 80, so a block's mean moves in jumps of 10 grey levels (80 / 8). Two blocks at 132
    and 134 round to 0 and 1 steps: 128 and 138, with the jump exactly between column 7 and column 8."""
    picture = blocks_of_flat_grey([[132, 134]])
    out = registry.apply(NAME, picture, quality=10)
    np.testing.assert_allclose(out[:, :8], 128 / 255, atol=1e-5)
    np.testing.assert_allclose(out[:, 8:], 138 / 255, atol=1e-5)


def test_at_quality_100_a_smooth_picture_comes_back_within_a_grey_level_or_two() -> None:
    y, x = np.mgrid[0:48, 0:64].astype(np.float32)
    smooth = np.stack([x / 63, y / 47, (x + y) / 110], axis=-1) * 0.8 + 0.1
    assert mean_error(smooth, registry.apply(NAME, smooth, quality=100)) < 1 / 255
    assert np.abs(smooth - registry.apply(NAME, smooth, quality=100)).max() < 3 / 255


def test_the_lower_the_quality_the_further_the_picture_is_from_the_original(photo) -> None:
    errors = [mean_error(photo, registry.apply(NAME, photo, quality=q)) for q in (95, 75, 50, 25, 5)]
    assert errors == sorted(set(errors))  # strictly rising


HEIGHT, WIDTH = 64, 128
SEEDS = range(6)


@pytest.fixture
def smooth() -> np.ndarray:
    """A gentle colour gradient well inside 0..1, so that damage never runs into the ends of the range and hides."""
    y, x = np.mgrid[0:HEIGHT, 0:WIDTH].astype(np.float32)
    return 0.3 + 0.4 * np.stack([x / WIDTH, y / HEIGHT, (x + y) / (WIDTH + HEIGHT)], axis=-1)


def differing(before: np.ndarray, after: np.ndarray) -> np.ndarray:
    return (before != after).any(axis=-1)


@pytest.mark.parametrize(
    "damage",
    [
        pytest.param({"seed": 7}, id="seed"),
        pytest.param({"dc_share": 1.0}, id="dc_share"),
        pytest.param({"magnitude": 100}, id="magnitude"),
    ],
)
def test_without_hits_the_damage_settings_change_nothing(smooth, damage) -> None:
    np.testing.assert_array_equal(
        registry.apply(NAME, smooth, quality=30, **damage), registry.apply(NAME, smooth, quality=30)
    )


@pytest.mark.parametrize("seed", SEEDS)
def test_a_dc_hit_carries_on_to_every_later_block_of_its_row_and_to_no_other_row(smooth, seed) -> None:
    """DC values are coded as differences from the block before, so damage to one moves all that follow in the row:
    the changed pixels run, from the first changed column on, to the picture's right edge, within one band of rows (a
    luma block is 8 rows tall, a chroma block, covering 2 x 2 luma blocks, 16)."""
    clean = registry.apply(NAME, smooth, quality=30)
    out = registry.apply(NAME, smooth, quality=30, hits=1, dc_share=1.0, seed=seed)
    changed = differing(clean, out)
    columns, rows = np.flatnonzero(changed.any(axis=0)), np.flatnonzero(changed.any(axis=1))
    assert np.array_equal(columns, np.arange(columns[0], WIDTH))  # from the first changed column to the last
    assert rows[0] // 16 == rows[-1] // 16  # inside one aligned band of 16 rows


@pytest.mark.parametrize("seed", SEEDS)
def test_an_ac_hit_stays_inside_one_block_of_the_8_px_grid(smooth, seed) -> None:
    """A hit on any other coefficient changes only its own block: 8 x 8 pixels of luma, or 16 x 16 for the colour
    planes, whose blocks cover four luma blocks. Either way the changed pixels fit in one aligned 16 x 16 cell."""
    clean = registry.apply(NAME, smooth, quality=30)
    out = registry.apply(NAME, smooth, quality=30, hits=1, dc_share=0.0, seed=seed)
    rows, columns = np.nonzero(differing(clean, out))
    assert len(rows) > 0
    assert rows.min() // 16 == rows.max() // 16
    assert columns.min() // 16 == columns.max() // 16


def test_more_hits_damage_more_of_the_picture(smooth) -> None:
    clean = registry.apply(NAME, smooth, quality=30)
    few = differing(clean, registry.apply(NAME, smooth, quality=30, hits=3, seed=1)).sum()
    many = differing(clean, registry.apply(NAME, smooth, quality=30, hits=60, seed=1)).sum()
    assert 0 < few < many


def test_a_larger_magnitude_changes_the_pixels_further(smooth) -> None:
    clean = registry.apply(NAME, smooth, quality=30)
    small = np.abs(clean - registry.apply(NAME, smooth, quality=30, hits=1, magnitude=1, dc_share=1.0, seed=2))
    large = np.abs(clean - registry.apply(NAME, smooth, quality=30, hits=1, magnitude=40, dc_share=1.0, seed=2))
    assert 0 < small.max() < large.max()


def test_detail_hits_never_move_the_average_of_a_block_and_dc_hits_do_in_both_directions(smooth) -> None:
    """The mean of the picture is the DC values' business alone: hits on the other 63 coefficients of each block change
    its pattern and leave its mean, so the picture's mean, where nothing is clamped (small steps, a mid-range picture).
    A DC hit shifts it up or down, by the sign of the hit."""
    clean = registry.apply(NAME, smooth, quality=90)
    detail = registry.apply(NAME, smooth, quality=90, hits=300, magnitude=1, dc_share=0.0, seed=1)
    assert not np.array_equal(detail, clean)
    np.testing.assert_allclose((detail - clean).mean(), 0, atol=1e-6)
    moves = [
        (registry.apply(NAME, smooth, quality=90, hits=1, dc_share=1.0, seed=seed) - clean).sum() for seed in range(12)
    ]
    assert min(moves) < 0 < max(moves)


RATIO = 4


@pytest.fixture
def textured() -> np.ndarray:
    """A mid-range picture of 128 x 256 with fine detail (noise a pixel wide) over a gentle gradient, so damage at
    design size neither clips nor hides."""
    rng = np.random.default_rng(4)
    y, x = np.mgrid[0:128, 0:256].astype(np.float32)
    ramp = 0.4 + 0.2 * np.stack([x / 256, y / 128, (x + y) / 384], axis=-1)
    return (ramp + 0.04 * rng.standard_normal((128, 256, 3))).astype(np.float32)


def averaged(picture: np.ndarray, ratio: int) -> np.ndarray:
    """`picture` shrunk by `ratio` along each side by the mean of each ratio x ratio block, by the definition."""
    height, width = picture.shape[:2]
    return picture.reshape(height // ratio, ratio, width // ratio, ratio, 3).mean(axis=(1, 3)).astype(np.float32)


def repeated(damage: np.ndarray, ratio: int) -> np.ndarray:
    """`damage` with every pixel repeated over a ratio x ratio block."""
    return np.kron(damage, np.ones((ratio, ratio, 1), np.float32))


DAMAGES = [
    pytest.param({"quality": 6}, id="plain"),
    pytest.param({"quality": 20, "hits": 30, "seed": 5}, id="hits"),
    pytest.param({"quality": 20, "hits": 30, "seed": 5, "dc_share": 1.0, "magnitude": 30}, id="dc hits"),
]


@pytest.mark.parametrize("settings", DAMAGES)
def test_at_a_size_ratio_of_1_the_picture_is_just_crushed(smooth, settings) -> None:
    crushed = registry.apply(NAME, smooth, **settings)
    np.testing.assert_array_equal(registry.apply(NAME, smooth, size_ratio=1.0, **settings), crushed)


def test_at_a_size_ratio_of_1_the_result_is_the_codec_round_trip(textured) -> None:
    expected = decode(encode(textured, quality=6))
    np.testing.assert_array_equal(registry.apply(NAME, textured, quality=6, size_ratio=1.0), expected)


@pytest.mark.parametrize("settings", DAMAGES)
def test_at_a_size_ratio_of_4_the_damage_is_constant_over_each_4_x_4_block_of_pixels(textured, settings) -> None:
    out = registry.apply(NAME, textured, size_ratio=RATIO, **settings)
    damage = out - textured
    blocks = damage.reshape(128 // RATIO, RATIO, 256 // RATIO, RATIO, 3)
    # a block with a pixel at an end of the range is clipped there and breaks the pattern: leave those out
    inside = ((out > 0) & (out < 1)).reshape(blocks.shape).all(axis=(1, 3), keepdims=True)
    inside = np.broadcast_to(inside, blocks.shape)
    assert inside.mean() > 0.25  # most blocks are still compared
    np.testing.assert_allclose(blocks[inside], np.broadcast_to(blocks[:, :1, :, :1], blocks.shape)[inside], atol=1e-5)


@pytest.mark.parametrize("settings", DAMAGES)
def test_at_a_size_ratio_of_4_the_damage_is_the_design_size_damage_repeated(textured, settings) -> None:
    """The picture averaged to design size and crushed, with the same hits, differs from it by the damage; the result
    is the picture plus that damage with each pixel repeated over the 4 x 4 pixels it covers, held to 0..1."""
    design = averaged(textured, RATIO)
    damage = registry.apply(NAME, design, size_ratio=1.0, **settings) - design
    out = registry.apply(NAME, textured, size_ratio=RATIO, **settings)
    np.testing.assert_allclose(out, np.clip(textured + repeated(damage, RATIO), 0, 1), atol=1e-5)


def test_at_a_size_ratio_of_4_the_blocks_are_32_px_wide_and_the_fine_detail_stays(textured) -> None:
    """Block edges lie on the 32 px grid (8 design px), and the picture's pixel-wide detail is not smoothed away: the
    mean distance of a pixel from the mean of its 4 x 4 block is what the picture has."""
    out = registry.apply(NAME, textured, quality=6, size_ratio=RATIO)

    def fine_detail(picture: np.ndarray) -> float:
        return float(np.abs(picture - repeated(averaged(picture, RATIO), RATIO)).mean())

    assert fine_detail(out) == pytest.approx(fine_detail(textured), rel=0.05)


def test_at_a_size_ratio_of_4_the_codec_blocks_are_32_px_wide() -> None:
    """The ratio 1 ramp test at four times the size: two flat blocks of 32 px at 132 and 134 round to 128 and 138,
    with the jump exactly between column 31 and column 32."""
    picture = grey(np.kron(np.array([[132, 134]]), np.ones((32, 32), dtype=int)))
    out = registry.apply(NAME, picture, quality=10, size_ratio=RATIO)
    np.testing.assert_allclose(out[:, :32], 128 / 255, atol=1e-5)
    np.testing.assert_allclose(out[:, 32:], 138 / 255, atol=1e-5)


@pytest.mark.parametrize("ratio", [1.5, 2.5])
def test_a_ratio_that_is_not_whole_gives_a_picture_of_the_same_size_in_range(textured, ratio) -> None:
    picture = textured[:100, :170]
    out = registry.apply(NAME, picture, quality=6, hits=10, size_ratio=ratio)
    assert out.shape == picture.shape
    assert out.min() >= 0
    assert out.max() <= 1
