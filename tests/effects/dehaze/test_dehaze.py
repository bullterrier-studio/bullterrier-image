"""Behaviour of the `dehaze` effect."""

from itertools import pairwise

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "dehaze"
AIRLIGHT = np.array([0.9, 0.8, 0.6], np.float32)


def scene(seed: int = 0, shape: tuple[int, int] = (120, 160)) -> np.ndarray:
    """A haze-free scene whose dark channel is 0 everywhere: random colours, one random channel of each pixel at 0
    (the dark channel prior's assumption, met exactly)."""
    rng = np.random.default_rng(seed)
    colours = rng.uniform(0.2, 1.0, (*shape, 3)).astype(np.float32)
    np.put_along_axis(colours, rng.integers(0, 3, (*shape, 1)), 0.0, axis=-1)
    return colours


def haze(clear: np.ndarray, transmission: np.ndarray, airlight: np.ndarray = AIRLIGHT) -> np.ndarray:
    """The atmospheric scattering model every dehazing method inverts: I = J t + A (1 - t)."""
    return (clear * transmission[..., None] + airlight * (1 - transmission[..., None])).astype(np.float32)


def sloped_transmission(shape: tuple[int, int] = (120, 160)) -> np.ndarray:
    """From 0.05 (dense haze) at the top to 0.95 at the bottom."""
    return np.broadcast_to(np.linspace(0.05, 0.95, shape[0], dtype=np.float32)[:, None], shape).copy()


def mean_error(picture: np.ndarray, truth: np.ndarray, rows: slice = slice(None)) -> float:
    return float(np.abs(picture[rows] - truth[rows]).mean())


def test_a_synthetic_hazy_picture_is_recovered() -> None:
    clear = scene()
    hazy = haze(clear, sloped_transmission())
    out = registry.apply(NAME, hazy, amount=1.0)
    clear_part = slice(60, None)  # rows where the transmission is above 0.5
    assert mean_error(hazy, clear, clear_part) > 0.1  # the haze is real ...
    assert mean_error(out, clear, clear_part) < 0.04  # ... and gone
    assert mean_error(out, clear, clear_part) < 0.25 * mean_error(hazy, clear, clear_part)


def test_a_haze_free_picture_is_left_alone() -> None:
    """A picture with a dark channel of 0 has transmission 1 everywhere: nothing to remove."""
    clear = scene(1)
    np.testing.assert_allclose(registry.apply(NAME, clear, amount=1.0), clear, atol=1e-5)


def test_raising_amount_brings_the_picture_closer_to_the_clear_scene() -> None:
    clear = scene(3)
    hazy = haze(clear, sloped_transmission())
    errors = [mean_error(registry.apply(NAME, hazy, amount=a), clear, slice(60, None)) for a in (0.0, 0.4, 0.8, 1.0)]
    assert all(later < earlier - 0.01 for earlier, later in pairwise(errors))


def test_transmission_floor_limits_how_far_dense_haze_is_stretched() -> None:
    hazy = haze(scene(4), sloped_transmission())
    changes = [
        float(np.abs(registry.apply(NAME, hazy, amount=1.0, transmission_floor=f) - hazy).mean())
        for f in (1.0, 0.5, 0.1)
    ]
    assert changes[0] < 1e-5  # a floor of 1 allows no stretch at all
    assert changes[0] < changes[1] < changes[2]


# A 9 x 9 picture of one colour P with a 3 x 3 sky block I = A (rows and columns 6..8) and one pixel Q with a zero
# channel at (2, 2). The transmission is not smoothed with `radius` 0, so each result is worked out by hand from
# t = 1 - amount * (the least of I_c / A_c over the patch) and J = (I - A) / max(t, floor) + A.
# For P = (0.3, 0.2, 0.1) and the sky A = (0.9, 0.8, 0.6) the least ratio is 0.1 / 0.6 = 1/6, so
# amount 1: t = 5/6, J = (-0.6, -0.6, -0.5) / t + A; amount 0.6: t = 0.9.
P, SKY, Q = (0.3, 0.2, 0.1), tuple(AIRLIGHT), (0.5, 0.5, 0.0)


def patch_picture() -> np.ndarray:
    picture = np.tile(np.array(P, np.float32), (9, 9, 1))
    picture[6:, 6:] = SKY
    picture[2, 2] = Q
    return picture


CLEARED_P = (0.18, 0.08, 0.0)  # t = 5/6
HALF_CLEARED_P = (0.9 - 0.6 / 0.9, 0.8 - 0.6 / 0.9, 0.6 - 0.5 / 0.9)  # t = 0.9: (0.2333, 0.1333, 0.0444)

HAND_ROWS = {
    "amount 1 clears P by t = 5/6": ({"patch": 1}, (2, 4), CLEARED_P),
    "amount 0.6 clears P by t = 0.9": ({"patch": 1, "amount": 0.6}, (2, 4), HALF_CLEARED_P),
    "a floor of 0.9 holds t = 5/6 up to 0.9": ({"patch": 1, "transmission_floor": 0.9}, (2, 4), HALF_CLEARED_P),
    "a floor of 0.5 is below t and changes nothing": ({"patch": 1, "transmission_floor": 0.5}, (2, 4), CLEARED_P),
    "the sky is the airlight and stays": ({"patch": 1}, (7, 7), SKY),
    "the edge of the sky stays too": ({"patch": 1}, (6, 6), SKY),
    "Q, with a zero channel, has transmission 1": ({"patch": 1}, (2, 2), Q),
    "patch 1: Q's neighbour is cleared like P": ({"patch": 1}, (2, 3), CLEARED_P),
    "patch 3: Q's neighbour sees Q in its patch, so t = 1": ({"patch": 3}, (2, 3), P),
    "patch 3: two pixels from Q is outside its patch": ({"patch": 3}, (2, 4), CLEARED_P),
    "patch 3: a diagonal neighbour of Q is in its patch": ({"patch": 3}, (3, 3), P),
    "patch 5: two pixels from Q is inside its patch": ({"patch": 5}, (2, 4), P),
    "patch 3: the corner pixel's patch is cut at the edge, not filled with black": ({"patch": 3}, (0, 0), CLEARED_P),
    "patch 5: the corner pixel's cut patch still reaches Q": ({"patch": 5}, (0, 0), P),
}


@pytest.mark.parametrize(("settings", "where", "expected"), HAND_ROWS.values(), ids=list(HAND_ROWS))
def test_the_hand_worked_pixels_of_a_small_picture(settings, where, expected) -> None:
    out = registry.apply(NAME, patch_picture(), **{"radius": 0, "amount": 1.0, **settings})
    np.testing.assert_allclose(out[where], expected, atol=1e-5)


# What `edge` changes is the haze at the border. A 12 x 12 picture of P with a 4 x 4 sky block (rows and columns
# 5..8) and one pixel Z with a zero channel (transmission 1 wherever its patch reaches). With `patch` 4 the patch of
# a pixel spans two rows before it and one after, and the sky pixel (7, 7) sees only sky whatever lies beyond the
# border, so the airlight is the sky under every choice. The patch of the corner pixel (0, 0) holds Z at (2, 2) only
# if the picture is mirrored about its edge pixel; the patch of (11, 11) holds Z at (0, 0) only if the picture wraps
# round. Where the patch holds Z, t = 1 and P stays; elsewhere t = 5/6 and P is cleared as in the rows above.
def border_picture(zero_at: tuple[int, int] | None = None) -> np.ndarray:
    picture = np.tile(np.array(P, np.float32), (12, 12, 1))
    picture[5:9, 5:9] = SKY
    if zero_at is not None:
        picture[zero_at] = Q
    return picture


EDGE_ROWS = {
    "nearest: the patch is cut at the border, so Z two rows away is not seen": ("nearest", (2, 2), (0, 0), CLEARED_P),
    "reflect repeats the border pixel, so it sees what nearest sees": ("reflect", (2, 2), (0, 0), CLEARED_P),
    "mirror reaches the pixel two rows in: Z is seen, t = 1": ("mirror", (2, 2), (0, 0), P),
    "wrap looks at the far side, not at Z two rows in": ("wrap", (2, 2), (0, 0), CLEARED_P),
    "nearest does not see the opposite corner": ("nearest", (0, 0), (11, 11), CLEARED_P),
    "reflect does not see the opposite corner": ("reflect", (0, 0), (11, 11), CLEARED_P),
    "mirror does not see the opposite corner": ("mirror", (0, 0), (11, 11), CLEARED_P),
    "wrap sees Z at the opposite corner, t = 1": ("wrap", (0, 0), (11, 11), P),
}


@pytest.mark.parametrize(("edge", "zero_at", "where", "expected"), EDGE_ROWS.values(), ids=list(EDGE_ROWS))
def test_edge_sets_what_the_patch_sees_beyond_the_border_of_the_picture(edge, zero_at, where, expected) -> None:
    out = registry.apply(NAME, border_picture(zero_at), patch=4, radius=0, amount=1.0, edge=edge)
    np.testing.assert_allclose(out[where], expected, atol=1e-5)


# The same picture with a corner of haze W = (0.95, 0.85, 0.7) in rows and columns 0..2, hazier than the sky (a
# darkest channel of 0.7 against 0.6). Where the patch is cut or mirrored at the border the corner pixels see only W
# and rank first, so W is the airlight: t = 1 - (0.1 / 0.7) = 6/7 and J = W - (W - P) x 7/6. If the picture wraps
# round, the patch of every corner pixel also holds P from the far side, W ranks below the sky and the airlight is the
# sky, as in the rows above.
CORNER_HAZE = (0.95, 0.85, 0.7)
CLEARED_BY_CORNER_HAZE = (0.95 - 0.65 * 7 / 6, 0.85 - 0.65 * 7 / 6, 0.0)  # (0.1917, 0.0917, 0)
AIRLIGHT_EDGE_ROWS = {
    "nearest: the corner's patch is cut, so W ranks first": ("nearest", CLEARED_BY_CORNER_HAZE),
    "reflect: the corner's patch is cut, so W ranks first": ("reflect", CLEARED_BY_CORNER_HAZE),
    "mirror: the corner's patch holds only W, so W ranks first": ("mirror", CLEARED_BY_CORNER_HAZE),
    "wrap: the corner's patch holds P from the far side, so the sky is the airlight": ("wrap", CLEARED_P),
}


@pytest.mark.parametrize(("edge", "expected"), AIRLIGHT_EDGE_ROWS.values(), ids=list(AIRLIGHT_EDGE_ROWS))
def test_edge_sets_which_pixels_the_airlight_search_ranks_haziest_at_the_border(edge, expected) -> None:
    picture = border_picture()
    picture[:3, :3] = CORNER_HAZE
    out = registry.apply(NAME, picture, patch=4, radius=0, amount=1.0, edge=edge)
    np.testing.assert_allclose(out[6, 2], expected, atol=1e-5)


@pytest.mark.parametrize("edge", ["nearest", "reflect", "mirror", "wrap"])
def test_every_edge_choice_gives_a_picture_of_the_same_shape_within_0_to_1(edge, photo) -> None:
    odd = photo[:51, :67] * 0.5 + 0.3
    out = registry.apply(NAME, odd, patch=9, edge=edge)
    assert out.shape == odd.shape
    assert out.min() >= 0.0
    assert out.max() <= 1.0


def test_an_edge_the_patch_search_cannot_use_is_refused() -> None:
    with pytest.raises(ValueError, match="edge"):
        registry.apply(NAME, scene(), edge="constant")


def candidate_picture(*candidates: tuple[float, float, float]) -> np.ndarray:
    """10 x 10 of P with two pixels in each of the given colours, so that the airlight search sees them first when it
    looks at the 4 or 8 haziest pixels of 100 (their darkest channel, 0.5 or more, is above P's 0.1)."""
    picture = np.tile(np.array(P, np.float32), (10, 10, 1))
    for index, colour in enumerate(candidates):
        picture[index, 0:2] = colour
    return picture


SKY_FOUR = (0.9, 0.8, 0.6), (0.9, 0.8, 0.6)  # the sky, listed twice so 4 pixels are the sky
BRIGHTER = (1.0, 0.9, 0.5)  # a darkest channel of 0.5, below the sky's 0.6, but brighter overall: 2.4 against 2.3
BRIGHT_GREY, VIVID = (0.8, 0.8, 0.8), (1.0, 0.5, 0.5)  # sum 2.4, largest channel 0.8 / sum 2.0, largest channel 1.0
AIRLIGHT_ROWS = {
    "4 of 100: the sky is the haziest; P = (0.18, 0.08, 0)": ((*SKY_FOUR, BRIGHTER, BRIGHTER), 0.045, CLEARED_P),
    "5 of 100: the first W is searched too and is the airlight; t = 0.8, J = (0.125, 0.025, 0)": (
        (*SKY_FOUR, BRIGHTER, BRIGHTER),
        0.055,
        (0.125, 0.025, 0.0),
    ),
    "8 of 100: the brighter W is searched too and is the airlight; t = 0.8, J = (0.125, 0.025, 0)": (
        (*SKY_FOUR, BRIGHTER, BRIGHTER),
        0.085,
        (0.125, 0.025, 0.0),
    ),
    "the brightest by red + green + blue wins over the one with the largest channel; t = 0.875": (
        (BRIGHT_GREY, BRIGHT_GREY, VIVID, VIVID),
        0.085,
        (0.8 - 0.5 / 0.875, 0.8 - 0.6 / 0.875, 0.0),  # J = (P - A) / t + A with A = (0.8, 0.8, 0.8)
    ),
}


@pytest.mark.parametrize(("candidates", "fraction", "expected"), AIRLIGHT_ROWS.values(), ids=list(AIRLIGHT_ROWS))
def test_airlight_is_the_brightest_of_the_haziest_pixels_and_fraction_sets_how_many_are_searched(
    candidates, fraction, expected
) -> None:
    """The pixels are ranked by dark channel and the airlight is the brightest of the top `airlight_fraction`."""
    out = registry.apply(
        NAME, candidate_picture(*candidates), patch=1, radius=0, amount=1.0, airlight_fraction=fraction
    )
    np.testing.assert_allclose(out[9, 9], expected, atol=1e-5)


# A dim airlight (0.3, 0.25, 0.2) and the pixel (0.15, 0.1, 0.05): ratios (0.5, 0.4, 0.25), t = 0.75, J = A - 0.2.
# A nearly black airlight channel (0.002) is divided by as it is, not rounded up: P' = (0.15, 0.001, 0.1) has ratios
# 0.5 everywhere, t = 0.5 and J = 0.
DIM_ROWS = {
    "a dim airlight": ((0.3, 0.25, 0.2), (0.15, 0.1, 0.05), (0.1, 0.05, 0.0)),
    "a nearly black airlight channel": ((0.3, 0.002, 0.2), (0.15, 0.001, 0.1), (0.0, 0.0, 0.0)),
}


@pytest.mark.parametrize(("sky", "pixel", "expected"), DIM_ROWS.values(), ids=list(DIM_ROWS))
def test_a_dim_airlight_is_used_as_it_is(sky, pixel, expected) -> None:
    picture = np.tile(np.array(pixel, np.float32), (10, 10, 1))
    picture[0, 0:2] = picture[1, 0:2] = sky
    out = registry.apply(NAME, picture, patch=1, radius=0, amount=1.0, airlight_fraction=0.045)
    np.testing.assert_allclose(out[9, 9], expected, atol=1e-5)


def test_a_larger_radius_mixes_in_the_haze_of_more_distant_pixels() -> None:
    """Pixel (4, 4) of the small picture is 2 px from the sky block, where the transmission is 0. With a radius of 0
    it is cleared as P is; with a radius that reaches the sky, and an edge_threshold above every edge so the filter
    averages across them, each step of the radius mixes in more of the sky's haze."""
    results = [
        registry.apply(NAME, patch_picture(), patch=1, amount=1.0, radius=r, edge_threshold=1.0)[4, 4, 0]
        for r in (0, 1, 2, 4)
    ]
    assert results[0] == pytest.approx(CLEARED_P[0], abs=1e-5)
    assert all(later < earlier - 0.005 for earlier, later in pairwise(results))


def test_edge_threshold_keeps_the_haze_estimate_from_blurring_across_an_edge_in_the_picture() -> None:
    """The sky block is an edge of 0.1 or more in luma. Below that the pixel 2 px from it keeps its own haze, above
    it the sky's haze is averaged in."""
    sharp, smooth = (
        registry.apply(NAME, patch_picture(), patch=1, amount=1.0, radius=2, edge_threshold=e)[4, 4, 0]
        for e in (0.001, 1.0)
    )
    assert sharp == pytest.approx(CLEARED_P[0], abs=1e-3)
    assert smooth < sharp - 0.03
    # an edge ten times the threshold or more is still kept
    moderate = registry.apply(NAME, patch_picture(), patch=1, amount=1.0, radius=2, edge_threshold=0.05)[4, 4, 0]
    assert moderate == pytest.approx(CLEARED_P[0], abs=3e-3)


@pytest.mark.parametrize(("radius", "edge_threshold"), [(1, 0.001), (1, 0.03), (3, 0.001), (3, 0.3)])
def test_dehazing_only_ever_darkens_a_picture_whose_airlight_is_its_brightest_grey(radius, edge_threshold) -> None:
    """Channels of 0.05 or 0.95 only: the haziest pixels are the white ones, so the airlight is 0.95 and the scene is
    J = A + (I - A) / t with t at most 1, which is never brighter than I. A transmission above 1 would brighten."""
    mixed = np.random.default_rng(8).random((40, 50, 3)) > 0.5
    picture = np.where(mixed, 0.95, 0.05).astype(np.float32)
    out = registry.apply(NAME, picture, patch=1, amount=1.0, radius=radius, edge_threshold=edge_threshold)
    assert (out <= picture + 1e-6).all()
    assert (out < picture - 0.02).any()  # and it does clear something
