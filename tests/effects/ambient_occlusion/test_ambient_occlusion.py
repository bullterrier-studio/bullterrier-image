"""Behaviour of the `ambient_occlusion` effect: the picture darkened where its depth says it is sheltered."""

import itertools

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "ambient_occlusion"
SIZE = 160  # test pictures are SIZE x SIZE: room for an interior beyond a 40 px kernel's reach
MIDDLE = SIZE // 2


def picture(seed: int = 0) -> np.ndarray:
    """A random colour picture with no zero channel, so output / input is defined everywhere."""
    return np.random.default_rng(seed).uniform(0.05, 1, (SIZE, SIZE, 3)).astype(np.float32)


def ramp(direction: float) -> np.ndarray:
    """A linear depth: 0.5 at the middle, rising by 0.003 per px towards `direction` (degrees clockwise from up)."""
    y, x = np.mgrid[0:SIZE, 0:SIZE].astype(np.float64)
    a = np.radians(direction)
    return (0.5 + 0.003 * ((x - MIDDLE) * np.sin(a) - (y - MIDDLE) * np.cos(a))).astype(np.float32)


def disc(inside: float, outside: float, radius: float = 8) -> np.ndarray:
    """A depth of `inside` within `radius` px of the middle and `outside` elsewhere: a pit if it is lower, a bump if
    it is higher (1 = nearest)."""
    y, x = np.mgrid[0:SIZE, 0:SIZE]
    return np.where(np.hypot(x - MIDDLE, y - MIDDLE) <= radius, inside, outside).astype(np.float32)


def bowl(width: float = 8) -> np.ndarray:
    """A smooth pit: a plane at depth 0.8 dipping to 0.3 at the middle, a Gaussian dip of `width` px sigma."""
    y, x = np.mgrid[0:SIZE, 0:SIZE]
    return (0.8 - 0.5 * np.exp(-((x - MIDDLE) ** 2 + (y - MIDDLE) ** 2) / (2 * width**2))).astype(np.float32)


def shelter(depth: np.ndarray, y: int, x: int, sigma: float, truncate: float) -> float:
    """The cavity at (y, x) by brute force: the Gaussian-weighted mean of the depth over the square window of
    half-width int(truncate * sigma + 0.5) (scipy's separable kernel), minus the depth there. The window must lie
    inside the picture."""
    reach = int(truncate * sigma + 0.5)
    offsets = np.arange(-reach, reach + 1)
    g = np.exp(-0.5 * (offsets / sigma) ** 2)
    weights = np.outer(g, g)
    window = depth[y - reach : y + reach + 1, x - reach : x + reach + 1].astype(np.float64)
    return float((weights * window).sum() / weights.sum() - depth[y, x])


def darkening(rgb: np.ndarray, out: np.ndarray) -> np.ndarray:
    """1 - output / input of the first channel: 0 = unchanged, 1 = black."""
    return np.asarray(1 - out[..., 0] / rgb[..., 0])


@pytest.mark.parametrize(
    ("depth", "settings", "region"),
    [
        pytest.param(np.zeros((SIZE, SIZE), np.float32), {"gain": 100}, np.s_[:, :], id="flat far depth"),
        pytest.param(np.full((SIZE, SIZE), 0.4, np.float32), {"radius": 200}, np.s_[:, :], id="flat, widest radius"),
        pytest.param(np.ones((SIZE, SIZE), np.float32), {"edge": "wrap"}, np.s_[:, :], id="flat near depth"),
        pytest.param(ramp(30), {"radius": 5}, np.s_[20:-20, 20:-20], id="a Gaussian keeps a ramp: small kernel"),
        pytest.param(ramp(200), {"radius": 10}, np.s_[40:-40, 40:-40], id="a Gaussian keeps a ramp: 40 px kernel"),
        pytest.param(
            ramp(90), {"radius": 8, "truncate": 2, "edge": "wrap"}, np.s_[16:-16, 16:-16], id="ramp, wrap, short cut"
        ),
    ],
)
def test_a_depth_without_hollows_leaves_the_picture_unchanged(
    depth: np.ndarray, settings: dict[str, object], region: tuple[slice, slice]
) -> None:
    """A flat or linear depth is its own Gaussian average, so nothing is sheltered (away from the edge rule)."""
    rgb = picture()
    out = registry.apply(NAME, rgb, depth=depth, **settings)
    assert out.dtype == np.float32
    np.testing.assert_allclose(out[region], rgb[region], atol=1e-5)


@pytest.mark.parametrize(
    ("seed", "settings"),
    [
        pytest.param(1, {}, id="defaults"),
        pytest.param(2, {"gain": 100, "radius": 1}, id="highest gain, smallest radius"),
        pytest.param(3, {"gain": 100, "radius": 50}, id="highest gain, FFT-sized kernel"),
        pytest.param(4, {"gain": 1, "radius": 200, "edge": "nearest"}, id="lowest gain, widest radius"),
    ],
)
def test_it_never_brightens_and_darkens_every_channel_by_one_grey_factor(
    seed: int, settings: dict[str, object]
) -> None:
    rgb = picture(seed)
    depth = np.random.default_rng(seed).random((SIZE, SIZE), dtype=np.float32)
    out = registry.apply(NAME, rgb, depth=depth, **settings)
    assert (out <= rgb).all()
    factor = out / rgb
    assert factor[..., 0].min() < 0.99  # a random depth has hollows: something is darkened
    np.testing.assert_allclose(factor, factor[..., :1].repeat(3, axis=2), atol=1e-5)


PIT = disc(0.3, 0.8)
BUMP = disc(1.0, 0.4)


@pytest.mark.parametrize(
    ("depth", "point", "radius", "gain", "truncate"),
    [
        pytest.param(PIT, (MIDDLE, MIDDLE), 3, 1, 4, id="pit centre, small kernel"),
        pytest.param(PIT, (MIDDLE, MIDDLE + 6), 3, 4, 4, id="inside the pit near its rim"),
        pytest.param(PIT, (MIDDLE, MIDDLE), 10, 1, 4, id="pit centre, 40 px kernel"),
        pytest.param(BUMP, (MIDDLE, MIDDLE + 10), 3, 2, 4, id="the plane beside a bump"),
        pytest.param(BUMP, (MIDDLE + 12, MIDDLE), 10, 1, 4, id="the plane beside a bump, 40 px kernel"),
        pytest.param(BUMP, (MIDDLE + 9, MIDDLE + 9), 6, 3, 4, id="the plane diagonally beside a bump"),
        pytest.param(BUMP, (MIDDLE, MIDDLE + 11), 6, 3, 1.5, id="a Gaussian cut off at 1.5 sigmas"),
    ],
)
def test_the_darkening_is_the_gain_times_the_brute_force_cavity(
    depth: np.ndarray, point: tuple[int, int], radius: float, gain: float, truncate: float
) -> None:
    """Expected: 1 - clip(gain x (Gaussian mean - depth), 0, 1), the mean summed by hand over the kernel's window."""
    rgb = picture()
    out = registry.apply(NAME, rgb, depth=depth, radius=radius, gain=gain, truncate=truncate)
    cavity = shelter(depth, *point, sigma=radius, truncate=truncate)
    assert 0 < gain * cavity < 1  # every row is a case the clip leaves alone
    assert darkening(rgb, out)[point] == pytest.approx(gain * cavity, abs=1e-5)


@pytest.mark.parametrize("radius", [3.0, 10.0, 24.0])
def test_a_pit_is_darkest_at_its_deepest_point_and_the_plane_far_from_it_is_untouched(radius: float) -> None:
    """A bowl minus its wider Gaussian average peaks at the bowl's centre."""
    rgb = picture()
    dark = darkening(rgb, registry.apply(NAME, rgb, depth=bowl(), radius=radius, gain=1))
    assert dark[MIDDLE, MIDDLE] == dark.max()
    assert dark[MIDDLE, MIDDLE] > 0.05
    assert dark[5, 5] == pytest.approx(0, abs=1e-6)


@pytest.mark.parametrize("radius", [3.0, 10.0, 24.0])
def test_a_bump_keeps_its_top_and_darkens_the_plane_beside_it(radius: float) -> None:
    """A contact shadow: the nearer side of a depth step is unchanged, the farther side darkened."""
    rgb = picture()
    out = registry.apply(NAME, rgb, depth=BUMP, radius=radius)
    bump = BUMP > 0.5
    np.testing.assert_array_equal(out[bump], rgb[bump])
    assert darkening(rgb, out)[MIDDLE, MIDDLE + 10] > 0.05


def test_raising_the_radius_widens_the_darkened_area() -> None:
    rgb = picture()
    areas = [
        int((darkening(rgb, registry.apply(NAME, rgb, depth=BUMP, radius=radius)) > 0.01).sum())
        for radius in (2, 6, 18)
    ]
    assert areas[0] < areas[1] < areas[2]


def test_raising_the_gain_darkens_more_until_the_pit_is_black() -> None:
    rgb = picture()
    centre, rim, far = (MIDDLE, MIDDLE), (MIDDLE, MIDDLE + 7), (5, 5)
    outs = [registry.apply(NAME, rgb, depth=PIT, radius=6, gain=gain) for gain in (1, 2, 3, 4, 8, 100)]
    for point in (centre, rim):
        values = [float(out[point][0]) for out in outs]
        assert values[0] > values[1] > values[2] > values[-1]  # the darkening grows with the gain while it can
        assert all(a > b or a == b == 0 for a, b in itertools.pairwise(values))  # until black stops it
        assert values[-1] == 0  # occlusion clipped at 1: full black, not below
    assert (np.stack(outs) >= 0).all()
    for out in outs:
        np.testing.assert_allclose(out[far], rgb[far], atol=1e-6)


@pytest.mark.parametrize("radius", [5.0, 50.0])
def test_with_the_wrap_edge_a_tileable_picture_darkens_without_a_seam(radius: float) -> None:
    """Shifting picture and depth round the edge shifts the result the same way."""
    rgb = picture()
    depth = np.random.default_rng(5).random((SIZE, SIZE), dtype=np.float32)
    shift = (SIZE // 3, SIZE // 2)
    out = registry.apply(NAME, rgb, depth=depth, radius=radius, edge="wrap")
    moved = registry.apply(
        NAME, np.roll(rgb, shift, axis=(0, 1)), depth=np.roll(depth, shift, axis=(0, 1)), radius=radius, edge="wrap"
    )
    np.testing.assert_allclose(moved, np.roll(out, shift, axis=(0, 1)), atol=1e-5)


def test_without_a_depth_the_picture_is_an_unchanged_copy(photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, radius=200, gain=100)
    np.testing.assert_array_equal(out, photo)
    assert out is not photo
