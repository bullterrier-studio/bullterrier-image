"""Behaviour of the `dust_scratches` effect: light and dark specks, hairs and scratches drawn on the picture."""

import itertools

import numpy as np
import pytest
from scipy import ndimage

from bullterrier_image.core import registry

NAME = "dust_scratches"
BARE = {"density": 0.0, "scratches": 0}  # no dust, no scratches
WHITE = "#ffffff"


def grey(size: int = 160) -> np.ndarray:
    return np.full((size, size, 3), 0.5, np.float32)


def changed(settings: dict, size: int = 160) -> np.ndarray:
    """Which pixels of a mid-grey picture the marks touch: (h, w) bool."""
    return (registry.apply(NAME, grey(size), **settings) != 0.5).any(axis=-1)


def coverage(settings: dict, size: int = 160) -> np.ndarray:
    """How far each pixel of a mid-grey picture is covered by marks, 0..1, when both colours are white and fully
    opaque: grey 0.5 mixed with white by the coverage c is 0.5 + 0.5 c."""
    out = registry.apply(NAME, grey(size), light=WHITE, dark=WHITE, opacity=1.0, **settings)
    return (out[..., 0] - 0.5) * 2.0


@pytest.mark.parametrize("light", [WHITE, "#102030"])
def test_no_dust_and_no_scratches_leave_the_picture_unchanged(light: str, photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, light=light, **BARE)
    np.testing.assert_array_equal(out, photo)
    assert not np.shares_memory(out, photo)


def test_more_dust_touches_more_of_the_picture_and_keeps_the_marks_it_had() -> None:
    fractions, touched = [], []
    for density in (0.1, 0.3, 1.0, 3.0):
        mask = changed({"density": density, "scratches": 0, "seed": 4})
        fractions.append(mask.mean())
        touched.append(mask)
    assert fractions == sorted(fractions)
    assert len(set(fractions)) == len(fractions)  # strictly more each time
    for fewer, more in itertools.pairwise(touched):
        assert not (fewer & ~more).any()  # a mark at a lower density is still there at a higher one


def test_the_number_of_marks_is_the_density_per_1000_square_pixels() -> None:
    """Hairs of size 1 are separate tiny marks: density 0.5 on 300 x 300 px = 90 000 px^2 is 45 marks, density 1.5
    is 135 (a few merge where they touch)."""
    for density, marks in ((0.5, 45), (1.5, 135)):
        mask = changed({"density": density, "scratches": 0, "hairs": 1.0, "size": 1.0, "seed": 6}, size=300)
        assert ndimage.label(mask, structure=np.ones((3, 3)))[1] == pytest.approx(marks, rel=0.1)


def test_a_hair_is_a_pixel_wide_line_of_up_to_size_long() -> None:
    """Hairs run from half of `size` to `size` long (mean 0.75 size, 9 px at size 12) and a pixel wide: each covers its
    length plus the two half-discs of its rounded ends (pi / 4 px^2): 45 hairs cover 45 x (9 + 0.785) = 440 px^2."""
    cover = coverage({"density": 0.5, "scratches": 0, "hairs": 1.0, "size": 12.0, "seed": 6}, size=300).sum()
    assert cover == pytest.approx(45 * (9 + np.pi / 4), rel=0.1)


def test_a_speck_is_a_disc_of_random_radius_up_to_half_the_size() -> None:
    """The radius is size / 2 x u^2 for u evenly random (many tiny specks, few big): the mean disc area is
    pi (size / 2)^2 E[u^4] = pi x 36 / 5 = 22.6 px^2 at size 12, so 270 specks (density 3) cover 6100 px^2."""
    cover = coverage({"density": 3.0, "scratches": 0, "hairs": 0.0, "size": 12.0, "seed": 6}, size=300).sum()
    assert cover == pytest.approx(270 * np.pi * 36 / 5, rel=0.2)


@pytest.mark.parametrize("size", [2.0, 6.0, 12.0])
def test_larger_specks_cover_more_of_the_picture(size: float) -> None:
    smaller = coverage({"density": 0.5, "scratches": 0, "size": size, "seed": 1}).sum()
    larger = coverage({"density": 0.5, "scratches": 0, "size": size * 2, "seed": 1}).sum()
    assert larger > 1.5 * smaller


@pytest.mark.parametrize("width", [1.0, 2.5, 4.0])
def test_a_straight_scratch_covers_its_width_in_every_row(width: float) -> None:
    """A vertical scratch of width w covers, summed across a row, exactly w pixels (the antialiased line is a box of
    width w seen through the one-pixel pixel box, and sampling that at whole pixels keeps its area)."""
    cover = coverage({"density": 0.0, "scratches": 1, "angle": 0.0, "width": width, "seed": 3})
    row_sums = cover.sum(axis=1)
    marked = row_sums[row_sums > 0]
    assert len(marked) >= 80  # runs over at least half of the 160 rows
    caps = int(width) + 2  # the rounded ends are not a full width
    np.testing.assert_allclose(marked[caps:-caps], width, atol=1e-3)


def one_scratch(seed: int, angle: float) -> tuple[np.ndarray, np.ndarray]:
    """The rows a lone scratch covers and its sideways position (coverage-weighted column) in each."""
    cover = coverage({"density": 0.0, "scratches": 1, "angle": angle, "width": 1.0, "seed": seed})
    rows = np.flatnonzero(cover.sum(axis=1) > 0)
    return rows, (cover * np.arange(160)).sum(axis=1)[rows] / cover.sum(axis=1)[rows]


def test_scratches_lean_to_either_side_by_up_to_the_angle() -> None:
    """Each scratch leans by a random angle up to 40 degrees off vertical, i.e. moves sideways at most tan(40) = 0.84
    px per row, to the left or to the right; across 16 scratches the slopes spread over nearly that whole range."""
    slopes = np.array([np.polyfit(*one_scratch(seed, 40.0), 1)[0] for seed in range(16)])
    limit = np.tan(np.radians(40))
    assert np.abs(slopes).max() <= limit + 0.005
    assert slopes.min() < -0.5 * limit
    assert slopes.max() > 0.85 * limit


def test_scratches_run_over_half_to_all_of_the_height_at_places_that_differ() -> None:
    spans = [(rows[0], rows[-1]) for rows, _ in (one_scratch(seed, 0.0) for seed in range(16))]
    lengths = np.array([last - first + 1 for first, last in spans])
    assert lengths.min() >= 80  # a half of the 160 rows, and the rounded ends
    assert lengths.max() <= 160
    assert lengths.max() > 140  # some run nearly all the way
    assert min(first for first, _ in spans) < 10 < max(first for first, _ in spans)  # starting at various heights
    assert min(last for _, last in spans) < 150  # and not all reaching the bottom


def test_more_scratches_cover_more() -> None:
    few = coverage({"density": 0.0, "scratches": 2, "seed": 1}).sum()
    many = coverage({"density": 0.0, "scratches": 8, "seed": 1}).sum()
    assert many > 2 * few


def test_light_marks_take_the_light_colour_and_dark_marks_the_dark_colour_at_the_opacity() -> None:
    settings = {"density": 1.0, "size": 20.0, "scratches": 0, "seed": 1, "light": "#ff0000", "dark": "#0000ff"}
    solid = registry.apply(NAME, grey(), opacity=1.0, **settings)
    assert (solid == np.array([1, 0, 0], np.float32)).all(axis=-1).any()  # a fully covered light pixel
    assert (solid == np.array([0, 0, 1], np.float32)).all(axis=-1).any()  # a fully covered dark pixel
    half = registry.apply(NAME, grey(), opacity=0.5, **settings)
    assert (half == np.array([0.75, 0.25, 0.25], np.float32)).all(axis=-1).any()  # half red over 0.5 grey
    assert (half == np.array([0.25, 0.25, 0.75], np.float32)).all(axis=-1).any()


def test_hairs_are_thin_and_specks_are_blobs() -> None:
    """With every mark a hair (one pixel wide) no 3 x 3 block is fully covered; with specks up to 20 px, one is."""

    def has_covered_block(mask: np.ndarray) -> bool:
        sums = np.cumsum(np.cumsum(mask.astype(int), axis=0), axis=1)
        blocks = sums[3:, 3:] - sums[:-3, 3:] - sums[3:, :-3] + sums[:-3, :-3]
        return bool((blocks == 9).any())

    base = {"density": 0.3, "size": 20.0, "scratches": 0, "seed": 2}
    assert has_covered_block(coverage({**base, "hairs": 0.0}) >= 0.99)
    hairs = coverage({**base, "hairs": 1.0})
    assert hairs.sum() > 0
    assert not has_covered_block(hairs >= 0.99)


def test_the_same_seed_gives_the_same_marks_and_another_seed_other_marks(photo: np.ndarray) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo, seed=1), registry.apply(NAME, photo, seed=1))
    assert (registry.apply(NAME, photo, seed=1) != registry.apply(NAME, photo, seed=2)).any()
    for settings in ({"density": 0.0, "scratches": 3}, {"density": 0.5, "scratches": 0}):
        assert (changed({**settings, "seed": 1}) != changed({**settings, "seed": 2})).any()
