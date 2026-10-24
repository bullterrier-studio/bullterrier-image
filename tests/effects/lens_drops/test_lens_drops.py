"""Behaviour of the `lens_drops` effect: water drops on the lens that show the scene upside down."""

import itertools
from typing import Any

import numpy as np
import pytest
from scipy import ndimage

from photo_fx.core import registry

NAME = "lens_drops"
SIZE = 200
BARE = {"blur": 0.0, "highlight": 0.0}  # a drop that only refracts
RAMPS = np.stack(
    [
        np.broadcast_to(np.linspace(0, 1, SIZE, dtype=np.float32), (SIZE, SIZE)),  # red grows along the columns
        np.broadcast_to(np.linspace(0, 1, SIZE, dtype=np.float32)[:, None], (SIZE, SIZE)),  # green down the rows
        np.full((SIZE, SIZE), 0.5, np.float32),
    ],
    axis=-1,
)
STEP = 1 / (SIZE - 1)  # how much each ramp rises per pixel


def drop_of(picture: np.ndarray, seed: int, **settings: Any) -> tuple[np.ndarray, np.ndarray, tuple[float, float], int]:
    """The picture with one drop on it, the drop's pixels (h, w) bool, its centre as (row, column) and the seed that
    made it: the first seed from `seed` on whose drop and the part of the scene it shows lie well inside the picture."""
    reach = settings.get("refraction", 1.5) * settings.get("radius", 30.0)
    for candidate in range(seed, seed + 100):
        out = registry.apply(NAME, picture, count=1, seed=candidate, **settings)
        touched = (out != picture).any(axis=-1)
        rows, columns = np.nonzero(touched)
        centre = (rows.mean(), columns.mean())
        if min(centre) - reach - 3 >= 0 and max(centre) + reach + 3 <= SIZE - 1:
            return out, touched, centre, candidate
    raise AssertionError("every one of 100 seeds put the drop near the edge")


def test_no_drops_leave_the_picture_unchanged(photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, count=0)
    np.testing.assert_array_equal(out, photo)
    assert not np.shares_memory(out, photo)


@pytest.mark.parametrize("refraction", [0.5, 1.0, 2.0, 3.0])
def test_a_drop_shows_the_scene_upside_down_refraction_times_as_wide(refraction: float) -> None:
    """Inside a drop the picture is read at the centre minus `refraction` times each pixel's offset from it, so a ramp
    that rises by one step per pixel outside falls by `refraction` steps per pixel inside, along both axes (bilinear
    reading is exact on a linear ramp)."""
    out, touched, *_ = drop_of(RAMPS, 3, refraction=refraction, radius=25.0, interpolation="bilinear", **BARE)
    inside = ndimage.binary_erosion(touched, iterations=2)  # clear of the soft rim
    both = inside[:, 1:] & inside[:, :-1]
    assert both.sum() > 200
    np.testing.assert_allclose(np.diff(out[..., 0], axis=1)[both], -refraction * STEP, atol=2e-5)
    both = inside[1:] & inside[:-1]
    np.testing.assert_allclose(np.diff(out[..., 1], axis=0)[both], -refraction * STEP, atol=2e-5)


@pytest.mark.parametrize("seed", [3, 20, 40, 60])
def test_a_drop_with_a_refraction_of_one_turns_the_scene_about_its_centre(seed: int) -> None:
    """The pixel at offset d from the drop's centre shows the one at offset -d, so a mark one side of the centre
    appears on the other side, the same distance from it (to within the pixel or so that the centre of a drawn disc
    can be told apart: the drop's centre is not on a pixel)."""
    out, touched, (row, column), _ = drop_of(RAMPS, seed, refraction=1.0, radius=25.0, interpolation="bilinear", **BARE)
    # the red ramp reads the column of the centre mirrored: 2 x centre - column, as a value
    probe = round(row)
    shown = np.flatnonzero(touched[probe])[3:-3]  # the drop's pixels on the centre row, clear of its rim
    np.testing.assert_allclose(out[probe, shown, 0], ((2 * column - np.arange(SIZE)) * STEP)[shown], atol=1.2 * STEP)
    # and down the centre column the green ramp reads the row of the centre mirrored
    probe = round(column)
    shown = np.flatnonzero(touched[:, probe])[3:-3]
    np.testing.assert_allclose(out[shown, probe, 1], ((2 * row - np.arange(SIZE)) * STEP)[shown], atol=1.2 * STEP)


def test_the_rim_of_a_drop_is_a_one_pixel_blend_of_what_it_shows_with_the_picture() -> None:
    """On the red ramp a pixel at column offset d from the centre changes by cover x (flipped - original) = cover x
    2 d steps when the refraction is 1, so cover = change / (2 d steps): 1 inside, 0 outside, a blend over the rim."""
    out, touched, (_, column), _ = drop_of(RAMPS, 3, refraction=1.0, radius=40.0, **BARE)
    rows, columns = np.nonzero(touched & (np.abs(np.arange(SIZE) - column) > 8))
    cover = (out[rows, columns, 0] - RAMPS[rows, columns, 0]) / (2 * (column - columns) * STEP)
    assert cover.min() > -0.1
    assert cover.max() < 1.1
    assert ((cover > 0.1) & (cover < 0.9)).sum() >= 10  # the antialiased rim
    assert (cover > 0.9).sum() > 100  # and the solid inside


def test_nearest_reading_shows_only_values_the_picture_has() -> None:
    noise = np.random.default_rng(3).random((SIZE, SIZE, 3), dtype=np.float32)
    out, touched, *_ = drop_of(noise, 3, radius=30.0, refraction=1.5, interpolation="nearest", **BARE)
    inside = ndimage.binary_erosion(touched, iterations=2)
    assert np.isin(out[inside], noise).all()


def test_a_drop_changes_only_the_pixels_inside_its_radius() -> None:
    """Its radius is between half of `radius` and all of it: the changed pixels are one disc of that size."""
    areas = []
    for seed in range(0, 40, 4):
        _, touched, (row, column), _ = drop_of(RAMPS, seed, radius=30.0, **BARE)
        rows, columns = np.nonzero(touched)
        reach = np.hypot(rows - row, columns - column).max()
        areas.append(touched.sum())
        assert 15.0 <= reach <= 31.0
        assert ndimage.label(touched)[1] == 1
    assert max(areas) > 1.5 * min(areas)  # the drops differ in size


@pytest.mark.parametrize("count", [3, 8])
def test_count_drops_are_made_whatever_the_size_of_the_picture(count: int) -> None:
    """Small drops far apart on a noise picture are `count` separate discs."""
    noise = np.random.default_rng(1).random((400, 400, 3), dtype=np.float32)
    out = registry.apply(NAME, noise, count=count, radius=10.0, seed=2, **BARE)
    assert ndimage.label((out != noise).any(axis=-1))[1] == pytest.approx(count, abs=1)


def test_what_a_drop_reads_beyond_the_picture_follows_the_edge_rule() -> None:
    """A big, strongly refracting drop reads from beyond the picture's border: each edge rule gives its own picture."""
    noise = np.random.default_rng(2).random((80, 80, 3), dtype=np.float32)
    rules = ("reflect", "nearest", "mirror", "wrap")
    outs = {
        rule: registry.apply(NAME, noise, count=1, radius=40.0, refraction=4.0, edge=rule, seed=1, **BARE)
        for rule in rules
    }
    for first, second in itertools.combinations(rules, 2):
        assert not np.array_equal(outs[first], outs[second]), f"{first} and {second} give the same picture"


def test_the_scene_inside_a_drop_is_blurred_by_blur_and_outside_it_is_not() -> None:
    noise = np.random.default_rng(3).random((SIZE, SIZE, 3), dtype=np.float32)
    sharp, touched, _, seed = drop_of(noise, 3, radius=40.0, refraction=1.0, blur=0.0, highlight=0.0)
    inside = ndimage.binary_erosion(touched, iterations=3)
    spreads = [sharp[inside].std()]
    for blur in (1.0, 3.0):
        out = registry.apply(NAME, noise, count=1, seed=seed, radius=40.0, refraction=1.0, blur=blur, highlight=0.0)
        far = ~ndimage.binary_dilation(touched, iterations=2)
        np.testing.assert_array_equal(out[far], noise[far])  # the same drop, nothing else touched
        spreads.append(out[inside].std())
    # white noise of spread s blurred by a Gaussian of sigma px has spread s / (2 sqrt(pi) sigma): 0.28 s and 0.09 s
    assert spreads[1] < 0.5 * spreads[0]
    assert spreads[2] < 0.5 * spreads[1]


def test_a_highlight_brightens_the_upper_rim_of_a_drop_and_leaves_its_middle_and_lower_rim() -> None:
    """The light is highlight x (distance from the centre / radius)^2 x (1 - dy / distance) / 2 with dy the pixel's
    row below the centre: full at the top of the rim, half at its sides, none at the bottom and none at the centre."""
    plain, touched, (row, column), seed = drop_of(RAMPS, 3, refraction=1.0, radius=40.0, **BARE)
    lit = registry.apply(NAME, RAMPS, count=1, seed=seed, refraction=1.0, radius=40.0, blur=0.0, highlight=1.0)
    light = np.zeros((SIZE, SIZE))
    light[touched] = ((lit - plain)[touched] / (1 - plain[touched]))[:, 0]  # screen: out = base + light x (1 - base)
    rows, columns = np.mgrid[0:SIZE, 0:SIZE]
    distance = np.hypot(rows - row, columns - column)
    reach = distance[touched].max()
    top = touched & (rows < row - 0.9 * distance) & (distance > 0.85 * reach)
    bottom = touched & (rows > row + 0.9 * distance) & (distance > 0.85 * reach)
    assert light[top].mean() > 0.6
    assert light[bottom].max() < 0.05
    assert light[touched & (distance < 0.25 * reach)].max() < 0.1
    assert (light >= -1e-6).all()
    np.testing.assert_array_equal(lit[~touched], plain[~touched])


def test_a_flat_picture_stays_flat_without_a_highlight() -> None:
    flat = np.full((60, 60, 3), 0.4, np.float32)
    np.testing.assert_allclose(registry.apply(NAME, flat, count=5, seed=1, highlight=0.0), 0.4, atol=1e-6)


def test_the_same_seed_gives_the_same_drops_and_another_seed_other_drops(photo: np.ndarray) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo, seed=1), registry.apply(NAME, photo, seed=1))
    assert (registry.apply(NAME, photo, seed=1) != registry.apply(NAME, photo, seed=2)).any()
