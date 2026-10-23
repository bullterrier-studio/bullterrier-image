"""Behaviour of the `lens_blur` effect: the picture averaged over a disc, as an out-of-focus lens draws it."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "lens_blur"
REC_709_RED = 0.2126  # the perceived brightness of pure red, from the Rec. 709 standard


def impulse(size: int = 81, value: tuple[float, float, float] = (1.0, 1.0, 1.0)) -> np.ndarray:
    """A black picture with one pixel of colour `value` in the middle."""
    dark = np.zeros((size, size, 3), np.float32)
    dark[size // 2, size // 2] = value
    return dark


@pytest.mark.parametrize("radius", [3.0, 6.0, 10.0, 20.0])
def test_an_impulse_becomes_a_disc_of_the_area_pi_r_squared(radius: float) -> None:
    """A white pixel spreads into a disc whose pixels all weigh the same, so one pixel's weight is 1 / area."""
    out = registry.apply(NAME, impulse(81), radius=radius)[..., 0].astype(np.float64)
    centre = 40
    assert out.sum() == pytest.approx(1.0, abs=1e-5)
    area = 1.0 / out[centre, centre]
    assert area == pytest.approx(np.pi * radius**2, rel=0.02)


@pytest.mark.parametrize("radius", [4.0, 6.8, 9.0, 9.7, 14.5])
def test_the_disc_is_flat_inside_round_and_empty_outside(radius: float) -> None:
    out = registry.apply(NAME, impulse(81), radius=radius)[..., 0]
    rows, cols = np.mgrid[0:81, 0:81] - 40
    distance = np.hypot(rows, cols)
    middle = out[40, 40]
    np.testing.assert_allclose(out[distance <= radius - 0.5], middle, rtol=1e-4)  # pixels wholly inside: full weight
    assert out[distance >= radius + 0.5].max() < 1e-6  # pixels wholly outside: none (FFT rounding only)
    assert (out[(distance > radius - 0.5) & (distance < radius + 0.5)] < middle * (1 + 1e-4)).all()  # the rim: a share
    # the rim along a row: a pixel's weight falls from 1 to 0 over the pixel-wide band centred on the radius
    for steps in range(1, 17):
        assert out[40, 40 + steps] / middle == pytest.approx(np.clip(radius + 0.5 - steps, 0, 1), abs=1e-4)
    # round, not square: pixels at the same distance weigh the same, in any direction
    assert out[40 + 3, 40 + 4] == pytest.approx(out[40, 40 + 5], abs=1e-6)
    assert out[40 + 3, 40 + 4] == pytest.approx(out[40 - 5, 40], abs=1e-6)


def test_a_larger_radius_spreads_the_light_further_and_thinner() -> None:
    small, large = (registry.apply(NAME, impulse(81), radius=r)[..., 0] for r in (4.0, 12.0))
    assert large[40, 40 + 8] > 1e-3  # 8 px out is inside a 12 px disc
    assert small[40, 40 + 8] < 1e-6  # and outside a 4 px one
    assert large[40, 40] < small[40, 40]


@pytest.mark.parametrize("edge", ["reflect", "mirror", "nearest", "wrap"])
@pytest.mark.parametrize("highlights", [0.0, 3.0])
def test_a_constant_picture_stays_as_it_is(edge: str, highlights: float) -> None:
    grey = np.full((21, 34, 3), 0.4, np.float32)
    out = registry.apply(NAME, grey, radius=6.0, highlights=highlights, edge=edge)
    np.testing.assert_allclose(out, 0.4, atol=1e-6)


def test_the_blur_keeps_the_mean_when_the_picture_wraps_around_and_there_are_no_highlights(photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, radius=5.0, highlights=0.0, edge="wrap")
    np.testing.assert_allclose(out.mean(axis=(0, 1)), photo.mean(axis=(0, 1)), atol=1e-5)
    assert out.std() < photo.std()


@pytest.mark.parametrize("highlights", [0.0, 4.0])
def test_radius_zero_leaves_the_picture_unchanged(photo: np.ndarray, highlights: float) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo, radius=0.0, highlights=highlights), photo)


@pytest.mark.parametrize("highlights", [0.0, 5.0])
def test_the_result_never_leaves_the_range_of_the_picture(photo: np.ndarray, highlights: float) -> None:
    out = registry.apply(NAME, photo, radius=7.0, highlights=highlights)
    assert out.min() >= photo.min()
    assert out.max() <= photo.max()


def test_each_colour_channel_is_blurred_on_its_own() -> None:
    out = registry.apply(NAME, impulse(81, (1.0, 0.0, 0.0)), radius=6.0)
    assert out[40, 44, 0] > 0
    assert out[..., 1:].max() == 0.0


@pytest.mark.parametrize(
    ("highlights", "value", "luma"),
    [
        (0.0, (1.0, 1.0, 1.0), 1.0),
        (2.0, (1.0, 1.0, 1.0), 1.0),
        (9.0, (1.0, 1.0, 1.0), 1.0),
        (5.0, (1.0, 0.0, 0.0), REC_709_RED),
    ],
    ids=["no gain", "gain 2 on white", "gain 9 on white", "gain 5 on red, a fifth as bright"],
)
def test_highlights_make_brighter_pixels_count_more_in_the_average(
    highlights: float, value: tuple[float, float, float], luma: float
) -> None:
    """A lone light pixel on black, `highlights` g, counts 1 + g x luma times a black pixel in every average that
    contains it. At a pixel inside the disc, with plain-disc share k of that light: the light reaches
    k (1 + g luma) of a total weight of 1 + k g luma (every other neighbour weighs 1)."""
    plain = registry.apply(NAME, impulse(81, value), radius=6.0, highlights=0.0)
    out = registry.apply(NAME, impulse(81, value), radius=6.0, highlights=highlights)
    probe = (40, 42)
    share = float(plain[probe][0])  # the plain disc's share k at 2 px from the light (the light's red is 1)
    expected = share * (1 + highlights * luma) / (1 + share * highlights * luma)
    assert float(out[probe][0]) == pytest.approx(expected, rel=1e-4)


def test_highlights_brighten_the_glow_around_a_light_without_adding_light_elsewhere() -> None:
    low, high = (registry.apply(NAME, impulse(81), radius=6.0, highlights=g)[..., 0] for g in (0.0, 8.0))
    assert high[40, 43] > low[40, 43]
    assert max(high[40, 40 + 9], low[40, 40 + 9]) < 1e-6  # beyond the disc nothing was ever reached


def test_edge_sets_what_the_blur_sees_beyond_the_edge() -> None:
    """One bright pixel in the first column of a dark picture, radius 2: 'nearest' repeats it beyond the edge so the
    pixel itself is brighter than 'wrap' makes it (the opposite edge is 30 px away)."""
    dark = np.zeros((31, 31, 3), np.float32)
    dark[15, 0] = 1.0
    nearest = registry.apply(NAME, dark, radius=2.0, edge="nearest")[15, 0, 0]
    wrapped = registry.apply(NAME, dark, radius=2.0, edge="wrap")[15, 0, 0]
    assert nearest > wrapped * 1.5


@pytest.mark.parametrize("edge", ["reflect", "mirror", "nearest", "wrap"])
def test_highlights_weigh_the_light_beyond_the_edge_as_well(edge: str) -> None:
    """With a share s of the average from the lone light (what the plain blur says at that pixel with this edge rule),
    the light counts 1 + g times a black pixel and so reaches s (1 + g) / (1 + s g) once the light beyond the
    edge, which each edge rule shows differently, is weighed in the total too."""
    dark = np.zeros((31, 31, 3), np.float32)
    dark[15, 0] = 1.0
    share = registry.apply(NAME, dark, radius=3.0, highlights=0.0, edge=edge)[15, 0, 0]
    out = registry.apply(NAME, dark, radius=3.0, highlights=6.0, edge=edge)[15, 0, 0]
    assert out == pytest.approx(share * 7.0 / (1.0 + share * 6.0), rel=1e-4)


@pytest.mark.parametrize("value", [0.7, 1.0])
def test_a_flat_picture_is_not_pushed_past_its_own_level_by_rounding(value: float) -> None:
    flat = np.full((40, 40, 3), value, np.float32)
    assert (registry.apply(NAME, flat, radius=9.0, highlights=50.0) <= flat).all()
