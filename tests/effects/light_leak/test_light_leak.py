"""Behaviour of the `light_leak` effect: warm, seeded washes of light bleeding in from one side of the picture."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "light_leak"
HEIGHT, WIDTH = 90, 120


def leak(**settings: object) -> np.ndarray:
    """The leak alone: the effect on a black picture, in white unless `settings` say otherwise, as (height, width)."""
    black = np.zeros((HEIGHT, WIDTH, 3), np.float32)
    return registry.apply(NAME, black, **{"color": "#ffffff", **settings})[..., 0]


def centroid_along(light: np.ndarray, angle: float) -> float:
    """How far the light's centre of mass lies from the picture's centre toward `angle` degrees (counterclockwise as
    seen on the picture, 0 = right), in px: the brute-force weighted mean of every pixel's offset."""
    rows, cols = np.mgrid[0:HEIGHT, 0:WIDTH].astype(np.float64)
    right, up = cols - (WIDTH - 1) / 2, (HEIGHT - 1) / 2 - rows
    toward = right * np.cos(np.radians(angle)) + up * np.sin(np.radians(angle))
    return float((light * toward).sum() / light.sum())


@pytest.mark.parametrize("seed", [0, 1, 2, 7])
@pytest.mark.parametrize("angle", [0.0, 45.0, 90.0, 180.0, 270.0])
def test_the_light_enters_from_the_side_angle_points_to(angle: float, seed: int) -> None:
    assert centroid_along(leak(angle=angle, seed=seed, amount=1.0, size=0.2), angle) > 5.0


def test_the_same_seed_gives_the_same_leak_and_another_seed_another() -> None:
    np.testing.assert_array_equal(leak(seed=3), leak(seed=3))
    assert np.abs(leak(seed=3) - leak(seed=4)).max() > 0.05


def test_it_lights_a_black_picture_and_stays_smooth() -> None:
    light = leak(amount=1.0, size=0.25)
    assert light.max() > 0.3
    assert np.abs(np.diff(light, axis=1)).max() < 0.1  # no hard edges: neighbouring pixels differ little
    assert np.abs(np.diff(light, axis=0)).max() < 0.1


def test_amount_scales_the_light_where_nothing_saturates() -> None:
    weak, strong = leak(amount=0.1, size=0.2), leak(amount=0.3, size=0.2)
    assert weak.max() > 0.01
    np.testing.assert_allclose(strong, 3 * weak, rtol=1e-3, atol=1e-6)


def test_a_bigger_size_spreads_more_light_deeper_into_the_picture() -> None:
    small, large = leak(size=0.05, angle=0.0), leak(size=0.4, angle=0.0)
    assert large.sum() > 1.5 * small.sum()
    assert centroid_along(large, 0.0) < centroid_along(small, 0.0)  # the centre of the light moves in from the edge


def test_color_tints_the_light_pixel_by_pixel() -> None:
    black = np.zeros((HEIGHT, WIDTH, 3), np.float32)
    out = registry.apply(NAME, black, color=(1.0, 0.5, 0.25), amount=1.0, size=0.2)
    lit = out[..., 0] > 0.01
    assert lit.sum() > 100
    np.testing.assert_allclose(out[..., 1][lit] / out[..., 0][lit], 0.5, rtol=1e-4)
    np.testing.assert_allclose(out[..., 2][lit] / out[..., 0][lit], 0.25, rtol=1e-4)


def test_light_is_screened_over_the_picture() -> None:
    """Over a grey of 0.5 each pixel is 1 - (1 - 0.5)(1 - light), light being what it adds to black."""
    light = leak(amount=0.8, size=0.2, seed=5)
    grey = np.full((HEIGHT, WIDTH, 3), 0.5, np.float32)
    out = registry.apply(NAME, grey, color="#ffffff", amount=0.8, size=0.2, seed=5)
    np.testing.assert_allclose(out[..., 0], 1 - 0.5 * (1 - light), atol=1e-5)


@pytest.mark.parametrize("blobs", [1, 3, 16])
def test_blobs_sets_how_many_patches_of_light_there_are(blobs: int) -> None:
    """A single blob is one connected patch; brute-force count of the lit regions (4-connected flood fill) is
    1 for one blob, and never more than the blobs."""
    lit = leak(blobs=blobs, size=0.1, amount=1.0, seed=2) > 0.05
    seen = np.zeros_like(lit)
    regions = 0
    for row, col in zip(*np.nonzero(lit), strict=True):
        if seen[row, col]:
            continue
        regions += 1
        stack = [(row, col)]
        seen[row, col] = True
        while stack:
            r, c = stack.pop()
            for nr, nc in ((r + 1, c), (r - 1, c), (r, c + 1), (r, c - 1)):
                if 0 <= nr < HEIGHT and 0 <= nc < WIDTH and lit[nr, nc] and not seen[nr, nc]:
                    seen[nr, nc] = True
                    stack.append((nr, nc))
    assert 1 <= regions <= blobs


def test_the_picture_only_ever_gets_brighter(photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, amount=1.0, size=0.3, blobs=8)
    assert (out >= photo - 1e-6).all()
    assert (out > photo + 0.01).any()


SEEDS = range(60)


def one_patch(seed: int, angle: float = 0.0) -> dict[str, float]:
    """A single patch's measurements, taken by brute force from the light it leaves on a black picture: where its
    brightest pixel lies (px in from the side `angle` points to, and px along that side, from the middle), how bright
    it is, the area above half its peak, and the spread of that area along the side over across it."""
    light = leak(blobs=1, size=0.2, amount=0.5, angle=angle, seed=seed)  # below 1: no flat top to the patch
    rows, cols = np.mgrid[0:HEIGHT, 0:WIDTH].astype(np.float64)
    right, up = cols - (WIDTH - 1) / 2, (HEIGHT - 1) / 2 - rows
    toward = right * np.cos(np.radians(angle)) + up * np.sin(np.radians(angle))
    along = up * np.cos(np.radians(angle)) - right * np.sin(np.radians(angle))
    row, col = np.unravel_index(light.argmax(), light.shape)
    half = light > light.max() / 2
    return {
        "depth": float(toward.max() - toward[row, col]),
        "along": float(along[row, col]),
        "peak": float(light.max()),
        "area": float(half.sum()),
        "reach_in": float(toward[half].std()),
        "elongation": float(along[half].std() / max(toward[half].std(), 1e-9)),
        "at_the_border": float(row in (0, HEIGHT - 1) or col in (0, WIDTH - 1)),
    }


@pytest.mark.parametrize("angle", [0.0, 90.0, 180.0, 270.0])
def test_a_patch_is_centred_between_the_side_it_comes_from_and_size_x_the_diagonal_in(angle: float) -> None:
    """Over many seeds the depth of the brightest pixel of a lone patch runs from the very edge to `size` x the
    diagonal (0.2 x 150 = 30 px), whichever side it comes in from, and its place along the side from end to end."""
    patches = [one_patch(seed, angle) for seed in SEEDS]
    depths = np.array([patch["depth"] for patch in patches])
    positions = np.array([patch["along"] for patch in patches])
    length = HEIGHT if angle in (0.0, 180.0) else WIDTH  # the side's length in px
    assert depths.min() < 5.0
    assert 20.0 < depths.max() <= 31.0
    assert positions.min() < -0.25 * length
    assert positions.max() > 0.25 * length
    assert (
        sum(patch["at_the_border"] for patch in patches) <= 6
    )  # the patch's middle lies inside the picture (all but a few)


def test_patches_differ_in_brightness_in_size_and_in_how_far_they_stretch_along_the_side() -> None:
    patches = [one_patch(seed) for seed in SEEDS]
    peaks = np.array([patch["peak"] for patch in patches])
    areas = np.array([patch["area"] for patch in patches])
    assert peaks.max() - peaks.min() > 0.1  # weighted by half to all
    assert areas.max() > 1.8 * areas.min()  # widths from half to all of `size`
    # patches clear of the picture's edges, which cut nothing off: how far one reaches in is its width, from half to
    # all of `size`, and how far it reaches along the side is that stretched by one to two
    clear = [p for p in patches if p["depth"] > 20 and abs(p["along"]) < 18]
    reach_in = np.array([patch["reach_in"] for patch in clear])
    stretches = np.array([patch["elongation"] for patch in clear])
    assert len(clear) >= 4
    assert reach_in.max() > 1.4 * reach_in.min()
    assert stretches.min() > 0.9  # never stretched across the side, only along it
    assert stretches.max() > 1.4  # and by up to twice


def test_overlapping_patches_blend_instead_of_clipping_to_a_flat_plateau() -> None:
    light = leak(blobs=8, size=0.3, amount=1.0)
    assert (light > light.max() - 1e-6).mean() < 0.01
