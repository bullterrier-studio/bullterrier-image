"""Behaviour of the `starburst` effect: diffraction spikes drawn through the bright points of a picture."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "starburst"
SIZE = 201
MIDDLE = SIZE // 2
DIAGONAL = float(np.hypot(SIZE, SIZE))


def point(value: float = 1.0, row: int = MIDDLE, col: int = MIDDLE, size: tuple[int, int] = (SIZE, SIZE)) -> np.ndarray:
    """A black picture with one grey pixel `value` at (`row`, `col`)."""
    dark = np.zeros((*size, 3), np.float32)
    dark[row, col] = value
    return dark


def polar(shape: tuple[int, int], row: int, col: int) -> tuple[np.ndarray, np.ndarray]:
    """Distance in px and direction in degrees (0 = right, 90 = up, as seen on the picture) of every pixel from
    (`row`, `col`)."""
    rows, cols = np.mgrid[0 : shape[0], 0 : shape[1]].astype(np.float64)
    return np.hypot(rows - row, cols - col), np.degrees(np.arctan2(row - rows, cols - col)) % 360


def spikes_around(out: np.ndarray, radius: float, *, floor: float = 0.05) -> list[float]:
    """The directions in degrees of the separate lit arcs on the circle of `radius` px round the middle pixel: the
    pixels within half a px of it, sorted by direction, grouped where the lit ones run on (brute force)."""
    distance, direction = polar(out.shape[:2], MIDDLE, MIDDLE)
    ring = np.abs(distance - radius) < 0.5
    order = np.argsort(direction[ring])
    angles, lit = direction[ring][order], out[..., 0][ring][order] > floor
    starts = lit & ~np.roll(lit, 1)  # a lit pixel whose previous one (circularly) is dark begins an arc
    found = []
    for start in np.flatnonzero(starts):
        arc = [start]
        while lit[(arc[-1] + 1) % len(lit)] and (arc[-1] + 1) % len(lit) != start:
            arc.append((arc[-1] + 1) % len(lit))
        arc_angles = np.unwrap(np.radians(angles[arc]))
        found.append(float(np.degrees(arc_angles.mean()) % 360))
    return sorted(found)


@pytest.mark.parametrize(
    ("rays", "angle"),
    [(1, 0.0), (2, 0.0), (3, 90.0), (4, 0.0), (4, 45.0), (5, 20.0), (6, 30.0), (8, 0.0)],
    ids=["one spike", "a line", "a triangle pointing up", "a cross", "a diagonal cross", "five", "six", "eight"],
)
def test_a_bright_point_gains_exactly_rays_spikes_pointing_from_angle(rays: int, angle: float) -> None:
    out = registry.apply(NAME, point(), rays=rays, angle=angle, length=0.2, amount=1.0, threshold=0.5)
    expected = sorted((angle + 360.0 * k / rays) % 360 for k in range(rays))
    found = spikes_around(out, radius=30)
    assert len(found) == rays
    # compare directions on the circle: a spike at 359.9 degrees is next to one at 0
    off = np.abs((np.array(found) - np.array(expected) + 180) % 360 - 180)
    assert off.max() < 3.0


def test_the_spikes_are_a_thin_line_and_light_nothing_else() -> None:
    """Brute force: every pixel the effect changes lies within one px of a spike's line, ahead of the point and no
    farther than length x the diagonal; the lines themselves are lit."""
    rays, angle, length = 3, 10.0, 0.25
    out = registry.apply(NAME, point(), rays=rays, angle=angle, length=length, amount=1.0, threshold=0.5)[..., 0]
    distance, direction = polar(out.shape, MIDDLE, MIDDLE)
    reach = length * DIAGONAL
    # how far each pixel is from the nearest spike's line, measured across it; spikes only run outward
    across = np.full(out.shape, np.inf)
    for k in range(rays):
        theta = np.radians(direction - (angle + 360.0 * k / rays))
        ahead = np.cos(theta) >= 0
        across = np.where(ahead, np.minimum(across, distance * np.abs(np.sin(theta))), across)
    changed = out > 1e-4
    assert (across[changed] <= 1.0).all()
    assert (distance[changed] <= reach).all()
    on_the_line = (across < 0.2) & (distance > 3) & (distance < 0.9 * reach)
    assert on_the_line.any()
    assert (out[on_the_line] > 0.05).all()


@pytest.mark.parametrize("length", [0.1, 0.3])
def test_length_is_how_far_a_spike_reaches_as_a_fraction_of_the_diagonal(length: float) -> None:
    out = registry.apply(NAME, point(), rays=4, angle=0.0, length=length, amount=1.0, threshold=0.5)[..., 0]
    distance, _ = polar(out.shape, MIDDLE, MIDDLE)
    farthest = distance[out > 1e-3].max()
    assert length * DIAGONAL - 2.0 <= farthest <= length * DIAGONAL


def test_a_spike_fades_linearly_from_the_highlight_to_nothing_at_its_full_length() -> None:
    """A pixel of 0.6 over a threshold of 0.5 holds 0.2 of highlight ((0.6 - 0.5) / (1 - 0.5)); `amount` 0.5 of it
    lies at the point, and the spike along the row to the right fades to zero at length x the diagonal."""
    length, amount = 0.2, 0.5
    out = registry.apply(NAME, point(0.6), rays=4, angle=0.0, length=length, amount=amount, threshold=0.5)[MIDDLE, :, 0]
    reach = length * DIAGONAL
    for steps in (1, 10, 25, 40):
        expected = 0.2 * amount * (1 - steps / reach)
        assert out[MIDDLE + steps] == pytest.approx(expected, abs=2e-5)


def test_amount_scales_the_spikes() -> None:
    weak = registry.apply(NAME, point(0.6), rays=4, length=0.2, amount=0.5, threshold=0.5)
    strong = registry.apply(NAME, point(0.6), rays=4, length=0.2, amount=1.5, threshold=0.5)
    row = (MIDDLE, slice(MIDDLE + 5, MIDDLE + 40), 0)
    np.testing.assert_allclose(strong[row], 3 * weak[row], rtol=1e-3)


@pytest.mark.parametrize(
    ("value", "threshold", "changes"),
    [(0.7, 0.8, False), (0.7, 0.6, True), (0.95, 0.9, True), (0.3, 0.0, True)],
    ids=["below the threshold", "above it", "a bright point", "threshold 0 lights everything above black"],
)
def test_only_what_is_brighter_than_threshold_makes_spikes(value: float, threshold: float, changes: bool) -> None:
    dark = point(value)
    out = registry.apply(NAME, dark, length=0.2, amount=1.0, threshold=threshold)
    assert bool((np.abs(out - dark) > 1e-4).any()) is changes


def test_black_and_a_picture_without_highlights_stay_as_they_are() -> None:
    black = np.zeros((40, 60, 3), np.float32)
    dim = np.full((40, 60, 3), 0.4, np.float32)
    np.testing.assert_array_equal(registry.apply(NAME, black), black)
    np.testing.assert_allclose(registry.apply(NAME, dim, threshold=0.5), dim, atol=1e-6)  # float32 rounding of screen


@pytest.mark.parametrize("rays", [1, 4, 7])
def test_the_picture_only_ever_gets_brighter(rays: int, photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, rays=rays, threshold=0.5, length=0.3, amount=2.0)
    assert (out >= photo - 1e-6).all()
    assert (out > photo + 0.01).any()  # and the settings do light something


def test_each_colour_channel_makes_spikes_of_its_own() -> None:
    red = np.zeros((SIZE, SIZE, 3), np.float32)
    red[MIDDLE, MIDDLE, 0] = 1.0
    out = registry.apply(NAME, red, length=0.2, amount=1.0, threshold=0.5)
    assert out[..., 0].max() > 0.5
    assert out[..., 1:].max() < 1e-4


def test_no_light_enters_from_beyond_the_picture() -> None:
    """A spike that points out of the picture from its first column must not come back in from the other side, nor
    be mirrored: everywhere but at the point itself the picture is unchanged."""
    dark = point(0.6, row=MIDDLE, col=0)
    out = registry.apply(NAME, dark, rays=1, angle=180.0, length=0.9, amount=1.0, threshold=0.5)
    elsewhere = np.ones(dark.shape[:2], bool)
    elsewhere[MIDDLE, 0] = False
    assert np.abs(out - dark)[elsewhere].max() < 1e-4


def test_a_spike_runs_on_to_the_picture_edge_when_it_is_longer_than_the_picture() -> None:
    out = registry.apply(NAME, point(), rays=1, angle=0.0, length=1.0, amount=1.0, threshold=0.5)
    assert out[MIDDLE, SIZE - 1, 0] > 0.3


def test_a_point_on_the_picture_edge_makes_the_same_spike_as_one_in_the_middle() -> None:
    """Nothing beyond the edge adds light: the spike of a point in the first column, pointing into the picture, is the
    one the same point makes 30 columns in."""
    settings = {"rays": 1, "angle": 0.0, "length": 0.3, "amount": 1.0, "threshold": 0.5}
    at_the_edge = registry.apply(NAME, point(0.8, col=0), **settings)[MIDDLE, :40, 0]
    inside = registry.apply(NAME, point(0.8, col=30), **settings)[MIDDLE, 30:70, 0]
    np.testing.assert_allclose(at_the_edge, inside, atol=2e-5)


@pytest.mark.parametrize("rays", [1, 4, 9])
def test_the_point_itself_is_lit_once_however_many_spikes_cross_it(rays: int) -> None:
    """The spikes share the point: 0.6 over a threshold of 0.5 is 0.2 of highlight, `amount` 0.5 of it is light 0.1,
    screened over 0.6: 1 - (1 - 0.6)(1 - 0.1) = 0.64."""
    out = registry.apply(NAME, point(0.6), rays=rays, amount=0.5, threshold=0.5)
    assert out[MIDDLE, MIDDLE, 0] == pytest.approx(0.64, abs=1e-5)


@pytest.mark.parametrize(
    ("color", "expected"),
    [("#ff0000", (1.0, 0.0, 0.0)), ("#fff", (1.0, 1.0, 1.0)), ((0.5, 0.25, 0.0), (0.5, 0.25, 0.0))],
    ids=["red spikes", "white spikes", "numbers 0..1 are a colour too"],
)
def test_color_tints_the_spikes(color: str | tuple[float, ...], expected: tuple[float, float, float]) -> None:
    out = registry.apply(NAME, point(), color=color, amount=1.0, length=0.2, threshold=0.5)
    # 10 px right of the point a spike of a white point has light 1 - 10 / reach in every channel
    reach = 0.2 * DIAGONAL
    np.testing.assert_allclose(out[MIDDLE, MIDDLE + 10], np.array(expected) * (1 - 10 / reach), atol=2e-5)


def test_limit_keeps_a_large_bright_area_from_flooding_while_the_brightest_point_still_shines() -> None:
    """A block of 0.9 (16% of the pixels) and one white point. At threshold 0.5 every block pixel radiates and
    the area is wrapped in light; with `limit` of 1% only the white point radiates."""
    picture = np.zeros((SIZE, SIZE, 3), np.float32)
    picture[60:140, 60:140] = 0.9
    picture[30, 170] = 1.0
    beside_the_block = (100, slice(145, 155), 0)  # right of the block, where its pixels' spikes run to the right
    beside_the_point = (30, slice(175, 185), 0)
    settings = {"rays": 4, "length": 0.1, "amount": 1.0, "threshold": 0.5}
    flooded = registry.apply(NAME, picture, **settings)
    capped = registry.apply(NAME, picture, limit=0.01, **settings)
    assert flooded[beside_the_block].min() > 0.05
    np.testing.assert_allclose(capped[beside_the_block], 0.0, atol=1e-6)
    assert capped[beside_the_point].min() > 0.05


@pytest.mark.parametrize(
    ("limit", "shining"),
    [(0.0, []), (0.00003, [0]), (0.00006, [0, 1]), (1.0, [0, 1, 2])],
    ids=["none", "one pixel of three", "two pixels of three", "no cap"],
)
def test_limit_lets_only_that_share_of_the_pixels_radiate_the_brightest_first(limit: float, shining: list[int]) -> None:
    """Three points of different brightness in a picture of 40401 pixels: a limit of 0.00003 allows one pixel."""
    rows = [30, 100, 170]  # brightest first
    picture = np.zeros((SIZE, SIZE, 3), np.float32)
    for row, value in zip(rows, (1.0, 0.9, 0.8), strict=True):
        picture[row, 30] = value
    out = registry.apply(NAME, picture, rays=4, length=0.1, amount=1.0, threshold=0.0, limit=limit)
    for index, row in enumerate(rows):
        spike = out[row, 35:45, 0]  # to the right of the point
        assert bool(spike.min() > 0.02) is (index in shining)
