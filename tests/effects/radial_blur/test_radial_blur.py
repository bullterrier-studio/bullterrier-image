"""Behaviour of the `radial_blur` effect: the picture averaged along arcs around a centre (spin) or along rays through
it (zoom)."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "radial_blur"
MODES = ["spin", "zoom"]
MANY = 200  # samples: enough that the average is the continuous one to within 1e-4


def grid(size: int = 101) -> tuple[np.ndarray, np.ndarray]:
    """Row and column of every pixel of a square picture, measured from its middle pixel."""
    rows, cols = np.mgrid[0:size, 0:size].astype(np.float64) - size // 2
    return rows, cols


def grey(values: np.ndarray) -> np.ndarray:
    """A picture with `values` (height, width) in all three channels."""
    return np.repeat(values.astype(np.float32)[..., None], 3, axis=2)


@pytest.mark.parametrize("mode", MODES)
def test_amount_zero_leaves_the_picture_unchanged(mode: str, photo: np.ndarray) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo, mode=mode, amount=0.0), photo)


@pytest.mark.parametrize("mode", MODES)
def test_a_single_sample_is_the_picture_itself(mode: str, photo: np.ndarray) -> None:
    np.testing.assert_allclose(registry.apply(NAME, photo, mode=mode, amount=0.5, samples=1), photo, atol=2e-6)


@pytest.mark.parametrize("mode", MODES)
@pytest.mark.parametrize("edge", ["reflect", "mirror", "nearest", "wrap"])
def test_a_constant_picture_stays_as_it_is(mode: str, edge: str) -> None:
    flat = np.full((31, 47, 3), 0.4, np.float32)
    out = registry.apply(NAME, flat, mode=mode, amount=0.8, x=0.2, y=0.9, edge=edge)
    np.testing.assert_allclose(out, 0.4, atol=1e-6)


@pytest.mark.parametrize("mode", MODES)
@pytest.mark.parametrize(
    ("x", "y", "pixel"),
    [(0.5, 0.5, (30, 40)), (0.0, 0.0, (0, 0)), (1.0, 0.5, (30, 80)), (0.25, 0.75, (45, 20))],
    ids=["the middle", "the top left pixel", "the right edge", "off centre"],
)
def test_the_centre_pixel_is_not_blurred(mode: str, x: float, y: float, pixel: tuple[int, int]) -> None:
    """The centre is at fraction x of the way across from the first pixel column to the last (y: down the rows)."""
    noise = np.random.default_rng(2).random((61, 81, 3), dtype=np.float32)
    out = registry.apply(NAME, noise, mode=mode, amount=0.9, x=x, y=y)
    np.testing.assert_allclose(out[pixel], noise[pixel], atol=2e-6)
    assert np.abs(out - noise).max() > 0.05  # and the rest of the picture is blurred


def test_zoom_averages_along_the_rays_through_the_centre() -> None:
    """On a row through the centre, a picture (x / 100)^2 with scales spread evenly over 1 +- amount / 2 (mean 1,
    variance amount^2 / 12) averages to (x^2 + (x - centre)^2 amount^2 / 12) / 100^2, worked out from the definition."""
    ramp = np.tile((np.arange(101) / 100.0) ** 2, (5, 1))
    out = registry.apply(NAME, grey(ramp), mode="zoom", amount=0.6, samples=MANY, y=0.5)[2, :, 0]
    x = np.arange(101.0)
    expected = (x**2 + (x - 50) ** 2 * 0.6**2 / 12) / 100**2
    np.testing.assert_allclose(out[20:81], expected[20:81], atol=3e-4)


def test_spin_shrinks_a_linear_picture_toward_the_centre_by_the_mean_cosine_of_the_arc() -> None:
    """A linear picture's average over rotations by +-amount x 180 degrees about the centre is the picture at the
    centre plus the pixel's offset times sin(w) / w, where w is half the arc in radians; the sines cancel."""
    amount = 0.2
    rows, cols = grid()
    ramp = (cols + 2 * rows + 200) / 400.0
    out = registry.apply(NAME, grey(ramp), mode="spin", amount=amount, samples=MANY)[..., 0]
    half_arc = np.pi * amount
    shrink = np.sin(half_arc) / half_arc
    expected = (shrink * (cols + 2 * rows) + 200) / 400.0
    inside = np.hypot(rows, cols) <= 40  # the arcs stay within the picture there
    np.testing.assert_allclose(out[inside], expected[inside], atol=3e-4)


def test_spin_leaves_a_picture_made_of_rings_alone_and_zoom_leaves_one_made_of_rays_alone() -> None:
    rows, cols = grid()
    radius, angle = np.hypot(rows, cols), np.arctan2(rows, cols)
    rings = grey(0.5 + 0.4 * np.cos(0.15 * radius))
    rays = grey(0.5 + 0.4 * np.cos(2 * angle))
    ring_zone = radius <= 40
    ray_zone = (radius >= 20) & (radius <= 40)  # away from the centre, where the rays meet; inside for 1 +- 0.1
    spun = registry.apply(NAME, rings, mode="spin", amount=0.3, samples=MANY)[..., 0]
    zoomed = registry.apply(NAME, rays, mode="zoom", amount=0.2, samples=MANY)[..., 0]
    np.testing.assert_allclose(spun[ring_zone], rings[..., 0][ring_zone], atol=3e-3)
    np.testing.assert_allclose(zoomed[ray_zone], rays[..., 0][ray_zone], atol=3e-3)
    # and each blurs the other kind of pattern, so the two checks above are sensitive
    fine_rings = grey(0.5 + 0.4 * np.cos(0.5 * radius))
    zoomed_rings = registry.apply(NAME, fine_rings, mode="zoom", amount=0.2, samples=MANY)[..., 0]
    spun_rays = registry.apply(NAME, rays, mode="spin", amount=0.3, samples=MANY)[..., 0]
    assert np.abs(zoomed_rings - fine_rings[..., 0])[ray_zone].max() > 0.1
    assert np.abs(spun_rays - rays[..., 0])[ray_zone].max() > 0.1


@pytest.mark.parametrize(
    ("edge", "expected"),
    [("nearest", 0.03125), ("mirror", 0.0625), ("reflect", 0.0577), ("wrap", 0.4848)],
    ids=["nearest", "mirror", "reflect", "wrap"],
)
def test_edge_sets_what_the_blur_sees_beyond_the_picture(edge: str, expected: float) -> None:
    """A ramp x / 100 over 101 columns, zoomed by 0.5 about the middle: the first pixel samples the positions 50 u
    for u spread evenly over +-0.25, that is -12.5 to 12.5, half of them left of the picture. Averaging what each
    edge rule puts there, worked out by hand (p = position, q = -p):
    nearest repeats the first pixel (0), so only p > 0 counts: E[max(0, p)] / 100 = 0.03125;
    mirror reads the picture at |p|: E|p| / 100 = 0.0625;
    reflect (about the edge of the first pixel) reads it at q - 1 once q > 1 and the flat 0 within a pixel of it:
    (E[(q - 1)+ | p < 0] / 2 + E[p | p > 0] / 2) / 100 = (2.645 + 3.125) / 100 = 0.0577;
    wrap brings in the ramp's far end, 1 at -1 falling to 0 at the first pixel (q within 1) and (101 - q) / 100 beyond:
    0.9071 / 2 + 0.03125 = 0.4848."""
    ramp = np.tile(np.arange(101) / 100.0, (3, 1))
    out = registry.apply(NAME, grey(ramp), mode="zoom", amount=0.5, samples=256, edge=edge, y=0.5)[1, 0, 0]
    assert out == pytest.approx(expected, abs=2e-3)


def test_samples_sets_how_smoothly_the_smear_is_filled_in() -> None:
    """One white pixel 20 px right of the centre, zoomed by 0.4: two samples leave two ghosts, many a streak."""
    dark = np.zeros((101, 101, 3), np.float32)
    dark[50, 70] = 1.0
    ghosts = registry.apply(NAME, dark, mode="zoom", amount=0.4, samples=2)[50, :, 0]
    streak = registry.apply(NAME, dark, mode="zoom", amount=0.4, samples=64)[50, :, 0]
    assert (ghosts > 0.02).sum() <= 6
    assert (streak > 0.02).sum() >= 8


def test_each_colour_channel_is_blurred_on_its_own() -> None:
    red_dot = np.zeros((101, 101, 3), np.float32)
    red_dot[50, 70, 0] = 1.0
    out = registry.apply(NAME, red_dot, mode="spin", amount=0.2, samples=32)
    assert out[..., 0].max() > 0
    assert out[..., 1:].max() == 0.0


@pytest.mark.parametrize("mode", MODES)
def test_the_result_never_leaves_the_range_of_the_picture(mode: str, photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, mode=mode, amount=0.7, samples=24, x=0.3)
    assert out.min() >= photo.min() - 1e-6
    assert out.max() <= photo.max() + 1e-6
