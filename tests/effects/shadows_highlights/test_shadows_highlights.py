"""Behaviour of the `shadows_highlights` effect."""

from itertools import pairwise

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "shadows_highlights"


def flat(value: float, shape: tuple[int, int] = (24, 24)) -> np.ndarray:
    return np.full((*shape, 3), value, np.float32)


def two_halves(dark: float = 0.1, bright: float = 0.9, size: tuple[int, int] = (16, 64)) -> np.ndarray:
    """Left half `dark`, right half `bright`, with a sharp edge between them."""
    picture = flat(bright, size)
    picture[:, : size[1] // 2] = dark
    return picture


def test_with_neither_slider_raised_the_picture_is_unchanged(photo: np.ndarray) -> None:
    np.testing.assert_allclose(registry.apply(NAME, photo, shadows=0.0, highlights=0.0), photo, atol=1e-6)


# The base layer of a flat picture is the picture, so a value v becomes v^e with e = (1 + highlights v^2) /
# (1 + shadows (1 - v)^2): worked out by hand (calculator) per row.
FLAT_ROWS = {
    "shadows 0.5 lifts 0.2: e = 1 / 1.32": (0.2, 0.5, 0.0, 0.2954454),
    "highlights 0.5 pulls 0.8 down: e = 1.32": (0.8, 0.0, 0.5, 0.7448671),
    "shadows 1 on 0.1: e = 1 / 1.81": (0.1, 1.0, 0.0, 0.2802295),
    "shadows 4 on 0.1: e = 1 / 4.24": (0.1, 4.0, 0.0, 0.5809663),
    "mid grey with both sliders at 1: e = 1.25 / 1.25 = 1": (0.5, 1.0, 1.0, 0.5),
    "highlights 1 on 0.95: e = 1.9025": (0.95, 0.0, 1.0, 0.9070248),
    "black stays black": (0.0, 4.0, 0.0, 0.0),
    "white stays white": (1.0, 0.0, 4.0, 1.0),
    "shadows barely touch a bright tone: e = 1 / 1.01": (0.9, 1.0, 0.0, 0.9009393),
}


@pytest.mark.parametrize(("value", "shadows", "highlights", "expected"), FLAT_ROWS.values(), ids=list(FLAT_ROWS))
def test_a_flat_picture_is_recovered_by_the_hand_worked_amount(value, shadows, highlights, expected) -> None:
    out = registry.apply(NAME, flat(value), shadows=shadows, highlights=highlights)
    np.testing.assert_allclose(out, expected, atol=1e-5)


def test_every_channel_of_a_pixel_follows_the_same_curve() -> None:
    """One exponent for the pixel, so a lift keeps every channel's order and a channel at 0 or 1 stays there."""
    pixel = np.broadcast_to(np.array([0.0, 0.1, 0.4], np.float32), (12, 12, 3)).copy()
    out = registry.apply(NAME, pixel, shadows=0.8, highlights=0.0)[0, 0]
    exponent = np.log(out[1]) / np.log(0.1)
    assert exponent < 0.8
    np.testing.assert_allclose(out, [0.0, 0.1**exponent, 0.4**exponent], atol=1e-5)


def test_raising_shadows_never_darkens_and_raising_highlights_never_brightens(photo: np.ndarray) -> None:
    lifts = [registry.apply(NAME, photo, shadows=s, highlights=0.0, radius=10) for s in (0.0, 0.5, 1.0, 4.0)]
    cuts = [registry.apply(NAME, photo, shadows=0.0, highlights=h, radius=10) for h in (0.0, 0.5, 1.0, 4.0)]
    for lower, higher in pairwise(lifts):
        assert (higher >= lower - 1e-6).all()
    for lower, higher in pairwise(cuts):
        assert (higher <= lower + 1e-6).all()
    assert (lifts[-1] - lifts[0]).max() > 0.1  # and the sliders do something
    assert (cuts[0] - cuts[-1]).max() > 0.1


def test_the_base_layer_is_local_so_the_same_dark_dot_is_lifted_less_in_a_bright_area() -> None:
    dot_in_dark, dot_in_bright = flat(0.3, (41, 41)), flat(0.9, (41, 41))
    dot_in_dark[20, 20] = dot_in_bright[20, 20] = 0.1  # a darker pixel in its surroundings
    kwargs = {"shadows": 1.0, "highlights": 0.0, "radius": 8, "edge_threshold": 1.0}
    lifted_in_dark = registry.apply(NAME, dot_in_dark, **kwargs)[20, 20, 0] - 0.1
    lifted_in_bright = registry.apply(NAME, dot_in_bright, **kwargs)[20, 20, 0] - 0.1
    assert lifted_in_bright < lifted_in_dark * 0.5


def test_with_radius_zero_each_pixel_is_judged_by_its_own_brightness() -> None:
    """The base is the pixel itself: a dark dot in a bright field is lifted as a dark pixel, not as a bright area."""
    dot = flat(0.9, (41, 41))
    dot[20, 20] = 0.1
    out = registry.apply(NAME, dot, shadows=1.0, highlights=0.0, radius=0.0, edge_threshold=1.0)
    np.testing.assert_allclose(out[20, 20, 0], 0.2802295, atol=1e-5)  # 0.1^(1 / 1.81), as a flat 0.1 picture
    np.testing.assert_allclose(out[0, 0, 0], 0.9009393, atol=1e-5)  # 0.9^(1 / 1.01), as a flat 0.9 picture


def test_radius_sets_how_wide_an_area_counts_as_the_surroundings() -> None:
    """The middle of a dark block 41 px wide has only dark surroundings within 10 px but bright ones within 30 px."""
    picture = flat(0.9, (41, 121))
    picture[:, 40:81] = 0.1
    centre = 20, 60

    def lift(radius: float) -> float:
        out = registry.apply(NAME, picture, shadows=1.0, highlights=0.0, radius=radius, edge_threshold=1.0)
        return float(out[centre][0]) - 0.1

    assert lift(10) > 0.17  # nearly the full lift of a dark pixel, 0.2802295 - 0.1
    assert lift(60) < lift(10) * 0.5


def test_edge_threshold_decides_whether_an_edge_keeps_its_own_side_of_the_base() -> None:
    """Right beside a sharp edge the dark side keeps its full lift when the edge is above the threshold, and is lifted
    less (the bright side leaks into the base) when the threshold is larger than the edge."""
    picture = two_halves()
    kwargs = {"shadows": 1.0, "highlights": 0.0, "radius": 4}
    sharp = registry.apply(NAME, picture, edge_threshold=0.01, **kwargs)[:, :, 0]
    soft = registry.apply(NAME, picture, edge_threshold=1.0, **kwargs)[:, :, 0]
    far_lift = sharp[8, 0] - 0.1  # the same in both: no edge within 4 px
    assert sharp[8, 31] - 0.1 > far_lift * 0.97
    assert soft[8, 31] - 0.1 < far_lift * 0.8
    np.testing.assert_allclose(soft[8, 0], sharp[8, 0], atol=1e-5)


def test_a_colours_brightness_is_its_rec_709_luma() -> None:
    """A red-ish pixel has luma 0.2126 * 0.9 + 0.7152 * 0.1 + 0.0722 * 0.1 = 0.27008, so the base is that, e = 1 /
    (1 + 0.72992^2) = 0.652 and a channel c becomes c^e, not by its channel mean (0.3667) that would give e = 0.714."""
    pixel = np.broadcast_to(np.array([0.9, 0.1, 0.1], np.float32), (12, 12, 3)).copy()
    out = registry.apply(NAME, pixel, shadows=1.0, highlights=0.0)[0, 0]
    np.testing.assert_allclose(out, [0.9335712, 0.2226343, 0.2226343], atol=1e-5)


def test_more_surroundings_are_seen_as_the_radius_grows() -> None:
    """A dark field whose bright half starts 12 px to the right of the pixel probed: a 4 px radius sees nothing of it,
    8 px sees its edge at the far reach of the filter, 16 px sees it plainly, so each step lifts the pixel less."""
    picture = flat(0.1, (9, 81))
    picture[:, 52:] = 0.9
    levels = [
        float(registry.apply(NAME, picture, shadows=1.0, highlights=0.0, radius=r, edge_threshold=1.0)[4, 40, 0])
        for r in (4, 8, 16)
    ]
    assert levels[0] == pytest.approx(0.2802295, abs=1e-5)  # a dark pixel's full lift, 0.1^(1 / 1.81)
    assert levels[0] - levels[1] > 0.01
    assert levels[1] - levels[2] > 0.01


def reference_base(luma: np.ndarray, radius: int, edge_threshold: float) -> np.ndarray:
    """The guided filter of He, Sun and Tang (2010) with the picture as its own guide, written naively: every window
    mean is a slice and a mean, with windows cut off at the picture's edge."""
    height, width = luma.shape

    def mean(values: np.ndarray) -> np.ndarray:
        return np.array(
            [
                [
                    values[max(y - radius, 0) : y + radius + 1, max(x - radius, 0) : x + radius + 1].mean()
                    for x in range(width)
                ]
                for y in range(height)
            ]
        )

    luma = luma.astype(np.float64)
    mean_luma = mean(luma)
    variance = mean(luma * luma) - mean_luma**2
    slope = variance / (variance + edge_threshold**2)  # covariance of the picture with itself is its variance
    offset = mean_luma - slope * mean_luma
    return np.clip(mean(slope) * luma + mean(offset), 0, 1)


@pytest.mark.parametrize(("radius", "edge_threshold"), [(0, 0.1), (1, 0.05), (3, 0.3), (6, 0.1), (40, 0.2)])
def test_the_base_layer_is_the_guided_filter_of_the_luma(radius: int, edge_threshold: float) -> None:
    """With shadows = 1 a pixel's red goes to r^e with e = 1 / (1 + (1 - B)^2), which gives back the base layer B."""
    rng = np.random.default_rng(7)
    picture = rng.uniform(0.05, 0.9, (13, 17, 3)).astype(np.float32)
    out = registry.apply(NAME, picture, shadows=1.0, highlights=0.0, radius=radius, edge_threshold=edge_threshold)
    exponent = np.log(out[..., 0]) / np.log(picture[..., 0])
    base = 1.0 - np.sqrt(1.0 / exponent - 1.0)
    luma = picture @ np.array([0.2126, 0.7152, 0.0722], np.float32)
    np.testing.assert_allclose(base, reference_base(luma, radius, edge_threshold), atol=2e-4)
