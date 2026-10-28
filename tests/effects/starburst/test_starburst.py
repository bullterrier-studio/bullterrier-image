"""Behaviour of the `starburst` effect: diffraction spikes drawn through the bright points of a picture."""

import numpy as np
import pytest
from scipy import ndimage

from bullterrier_image import Recipe, fx, render
from bullterrier_image.core import registry

NAME = "starburst"
SIZE = 201
MIDDLE = SIZE // 2


def point(value: float = 1.0, row: int = MIDDLE, col: int = MIDDLE, size: tuple[int, int] = (SIZE, SIZE)) -> np.ndarray:
    """A black picture with one grey pixel `value` at (`row`, `col`)."""
    dark = np.zeros((*size, 3), np.float32)
    dark[row, col] = value
    return dark


def polar(shape: tuple[int, int], row: int, col: int) -> tuple[np.ndarray, np.ndarray]:
    """Distance in px and direction in degrees (clockwise from right, 90 = down, as seen on the picture) of every
    pixel from (`row`, `col`)."""
    rows, cols = np.mgrid[0 : shape[0], 0 : shape[1]].astype(np.float64)
    return np.hypot(rows - row, cols - col), np.degrees(np.arctan2(rows - row, cols - col)) % 360


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
    [(1, 0.0), (1, 90.0), (2, 0.0), (3, 270.0), (4, 0.0), (4, 45.0), (5, 20.0), (6, 30.0), (8, 0.0)],
    ids=[
        "one spike right",
        "one spike down",
        "a line",
        "a triangle pointing up",
        "a cross",
        "a diagonal cross",
        "five",
        "six",
        "eight",
    ],
)
def test_a_bright_point_gains_exactly_rays_spikes_pointing_from_angle(rays: int, angle: float) -> None:
    out = registry.apply(NAME, point(), rays=rays, angle=angle, length=57.0, amount=1.0, threshold=0.5)
    expected = sorted((angle + 360.0 * k / rays) % 360 for k in range(rays))
    found = spikes_around(out, radius=30)
    assert len(found) == rays
    # compare directions on the circle: a spike at 359.9 degrees is next to one at 0
    off = np.abs((np.array(found) - np.array(expected) + 180) % 360 - 180)
    assert off.max() < 3.0


def test_the_spikes_are_a_thin_line_and_light_nothing_else() -> None:
    """Brute force: every pixel the effect changes lies within one px of a spike's line, ahead of the point and no
    farther than `length` px; the lines themselves are lit."""
    rays, angle, reach = 3, 10.0, 71.0
    out = registry.apply(NAME, point(), rays=rays, angle=angle, length=reach, amount=1.0, threshold=0.5)[..., 0]
    distance, direction = polar(out.shape, MIDDLE, MIDDLE)
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


@pytest.mark.parametrize("length", [28.0, 85.0])
def test_length_is_how_far_a_spike_reaches_in_px(length: float) -> None:
    out = registry.apply(NAME, point(), rays=4, angle=0.0, length=length, amount=1.0, threshold=0.5)[..., 0]
    distance, _ = polar(out.shape, MIDDLE, MIDDLE)
    farthest = distance[out > 1e-3].max()
    assert length - 2.0 <= farthest <= length


@pytest.mark.parametrize("width", [1.0, 3.0, 6.0])
def test_width_is_how_many_px_thick_a_spike_is_with_a_soft_edge_one_px_wide(width: float) -> None:
    """One spike to the right, 20 px out: a pixel `d` rows off its line lights by the brute-force profile
    clip(width / 2 + 0.5 - d, 0, 1) times the fade 1 - 20 / length."""
    length = 57.0
    out = registry.apply(NAME, point(), rays=1, angle=0.0, length=length, width=width, amount=1.0, threshold=0.5)
    offsets = np.arange(-5, 6)
    expected = np.clip(width / 2 + 0.5 - np.abs(offsets), 0.0, 1.0) * (1 - 20 / length)
    np.testing.assert_allclose(out[MIDDLE + offsets, MIDDLE + 20, 0], expected, atol=2e-5)


def test_a_spike_fades_linearly_from_the_highlight_to_nothing_at_its_full_length() -> None:
    """A pixel of 0.6 over a threshold of 0.5 holds 0.2 of highlight ((0.6 - 0.5) / (1 - 0.5)); `amount` 0.5 of it
    lies at the point, and the spike along the row to the right fades to zero at `length` px."""
    length, amount = 57.0, 0.5
    out = registry.apply(NAME, point(0.6), rays=4, angle=0.0, length=length, amount=amount, threshold=0.5)[MIDDLE, :, 0]
    for steps in (1, 10, 25, 40):
        expected = 0.2 * amount * (1 - steps / length)
        assert out[MIDDLE + steps] == pytest.approx(expected, abs=2e-5)


def test_amount_scales_the_spikes() -> None:
    weak = registry.apply(NAME, point(0.6), rays=4, length=57.0, amount=0.5, threshold=0.5)
    strong = registry.apply(NAME, point(0.6), rays=4, length=57.0, amount=1.5, threshold=0.5)
    row = (MIDDLE, slice(MIDDLE + 5, MIDDLE + 40), 0)
    np.testing.assert_allclose(strong[row], 3 * weak[row], rtol=1e-3)


@pytest.mark.parametrize(
    ("value", "threshold", "changes"),
    [(0.7, 0.8, False), (0.7, 0.6, True), (0.95, 0.9, True), (0.3, 0.0, True)],
    ids=["below the threshold", "above it", "a bright point", "threshold 0 lights everything above black"],
)
def test_only_what_is_brighter_than_threshold_makes_spikes(value: float, threshold: float, changes: bool) -> None:
    dark = point(value)
    out = registry.apply(NAME, dark, length=57.0, amount=1.0, threshold=threshold)
    assert bool((np.abs(out - dark) > 1e-4).any()) is changes


def test_black_and_a_picture_without_highlights_stay_as_they_are() -> None:
    black = np.zeros((40, 60, 3), np.float32)
    dim = np.full((40, 60, 3), 0.4, np.float32)
    np.testing.assert_array_equal(registry.apply(NAME, black), black)
    np.testing.assert_allclose(registry.apply(NAME, dim, threshold=0.5), dim, atol=1e-6)  # float32 rounding of screen


@pytest.mark.parametrize("rays", [1, 4, 7])
def test_the_picture_only_ever_gets_brighter(rays: int, photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, rays=rays, threshold=0.5, length=85.0, amount=2.0)
    assert (out >= photo - 1e-6).all()
    assert (out > photo + 0.01).any()  # and the settings do light something


def test_each_colour_channel_makes_spikes_of_its_own() -> None:
    red = np.zeros((SIZE, SIZE, 3), np.float32)
    red[MIDDLE, MIDDLE, 0] = 1.0
    out = registry.apply(NAME, red, length=57.0, amount=1.0, threshold=0.5)
    assert out[..., 0].max() > 0.5
    assert out[..., 1:].max() < 1e-4


def test_no_light_enters_from_beyond_the_picture() -> None:
    """A spike that points out of the picture from its first column must not come back in from the other side, nor
    be mirrored: everywhere but at the point itself the picture is unchanged."""
    dark = point(0.6, row=MIDDLE, col=0)
    out = registry.apply(NAME, dark, rays=1, angle=180.0, length=256.0, amount=1.0, threshold=0.5)
    elsewhere = np.ones(dark.shape[:2], bool)
    elsewhere[MIDDLE, 0] = False
    assert np.abs(out - dark)[elsewhere].max() < 1e-4


def test_a_spike_runs_on_to_the_picture_edge_when_it_is_longer_than_the_picture() -> None:
    out = registry.apply(NAME, point(), rays=1, angle=0.0, length=284.0, amount=1.0, threshold=0.5)
    assert out[MIDDLE, SIZE - 1, 0] > 0.3


def test_a_point_on_the_picture_edge_makes_the_same_spike_as_one_in_the_middle() -> None:
    """Nothing beyond the edge adds light: the spike of a point in the first column, pointing into the picture, is the
    one the same point makes 30 columns in."""
    settings = {"rays": 1, "angle": 0.0, "length": 85.0, "amount": 1.0, "threshold": 0.5}
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
    out = registry.apply(NAME, point(), color=color, amount=1.0, length=57.0, threshold=0.5)
    # 10 px right of the point a spike of a white point has light 1 - 10 / reach in every channel
    reach = 57.0
    np.testing.assert_allclose(out[MIDDLE, MIDDLE + 10], np.array(expected) * (1 - 10 / reach), atol=2e-5)


def test_limit_keeps_a_large_bright_area_from_flooding_while_the_brightest_point_still_shines() -> None:
    """A block of 0.9 (16% of the pixels) and one white point. At threshold 0.5 every block pixel radiates and
    the area is wrapped in light; with `limit` of 1% only the white point radiates."""
    picture = np.zeros((SIZE, SIZE, 3), np.float32)
    picture[60:140, 60:140] = 0.9
    picture[30, 170] = 1.0
    beside_the_block = (100, slice(145, 155), 0)  # right of the block, where its pixels' spikes run to the right
    beside_the_point = (30, slice(175, 185), 0)
    settings = {"rays": 4, "length": 28.0, "amount": 1.0, "threshold": 0.5}
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
    out = registry.apply(NAME, picture, rays=4, length=28.0, amount=1.0, threshold=0.0, limit=limit)
    for index, row in enumerate(rows):
        spike = out[row, 35:45, 0]  # to the right of the point
        assert bool(spike.min() > 0.02) is (index in shining)


LATTICE = np.arange(10, SIZE - 10, 9)  # points 9 px apart: spikes of length 8 px never reach the next point
FIELD = {"rays": 4, "length": 8.0, "amount": 0.5, "threshold": 0.5}


def lattice_gains(**settings: float) -> np.ndarray:
    """A lattice of grey 0.6 points; the light each one gets as a share of the full light a lone point gets (0.1,
    see `test_the_point_itself_is_lit_once_however_many_spikes_cross_it`), one entry per point, row by row."""
    picture = np.zeros((SIZE, SIZE, 3), np.float32)
    picture[np.ix_(LATTICE, LATTICE)] = 0.6
    out = registry.apply(NAME, picture, **FIELD, **settings)
    return ((out[np.ix_(LATTICE, LATTICE)][..., 0] - 0.6) / 0.4 / 0.1).ravel()


@pytest.mark.parametrize(
    ("spacing", "share"),
    [(0.0, 1.0), (0.5, 1.0), (1.0, 1.0), (1.5, 4 / 9), (2.0, 0.25), (4.0, 1 / 16)],
    ids=["0 = every point", "below 1 = every point", "1 = every point", "1.5: 1 in 2.25", "2: 1 in 4", "4: 1 in 16"],
)
def test_each_bright_pixel_radiates_with_the_chance_one_over_spacing_squared(spacing: float, share: float) -> None:
    gains = lattice_gains(spacing=spacing, seed=3)
    picked = float((gains > 0.5).mean())  # each point is lit fully (variation 0) or not at all
    assert picked == pytest.approx(share, abs=0.07)  # 441 points: binomial spread under 0.024
    assert set(np.round(gains, 3)) <= {0.0, 1.0}


def test_raising_spacing_only_drops_points_and_the_seed_picks_which() -> None:
    many = lattice_gains(spacing=1.5, seed=3) > 0.5
    few = lattice_gains(spacing=3.0, seed=3) > 0.5
    other = lattice_gains(spacing=3.0, seed=4) > 0.5
    assert (many | ~few).all()  # every point of the sparser pick is in the denser one
    assert many.sum() > few.sum()
    assert (few != other).any()


def count_glints(size: int, seed: int) -> int:
    """The separate lit patches the preset `sparkle` (spacing 70) leaves round a bright block, the middle third of a
    `size` px square picture."""
    block = np.zeros((size, size, 3), np.float32)
    block[size // 3 : 2 * size // 3, size // 3 : 2 * size // 3] = 0.9
    glitter = fx.starburst(preset="sparkle", spacing=70.0, seed=seed, threshold=0.5, variation=0.0)
    lit = (render(block, Recipe(name="glints", steps=[glitter]), size=size) - block).max(axis=-1)
    return int(ndimage.label(lit > 0.02)[1])


def test_the_number_of_glints_does_not_grow_with_the_resolution() -> None:
    """The same recipe on a 240 and on a 720 px render lights about as many glints (the bright area has nine times
    the pixels at 720, and the chance per pixel is nine times smaller); 80 seeds keep the counts within about
    20% of each other."""
    small = sum(count_glints(240, seed) for seed in range(80))
    large = sum(count_glints(720, seed) for seed in range(80))
    assert small > 70  # glints, not a handful by luck
    assert large == pytest.approx(small, rel=0.35)


def square_light(size: int) -> float:
    """The light a small bright square (6 design px wide) adds, summed over the picture rendered at `size` px, per
    design px squared (the sum divided by (size / 480)^2)."""
    side, middle = round(6 * size / 480), size // 2
    picture = np.zeros((size, size, 3), np.float32)
    picture[middle : middle + side, middle : middle + side] = 0.52
    # a faint highlight and a low amount keep the light far from white, where screening stops being proportional
    glow = fx.starburst(rays=4, length=30.0, width=3.0, threshold=0.5, amount=0.1)
    out = render(picture, Recipe(name="square", steps=[glow]), size=size)
    return float((out - picture).sum() / (size / 480) ** 2)


@pytest.mark.parametrize("size", [240, 720, 960], ids=["half", "one and a half", "twice"])
def test_the_light_of_a_bright_area_does_not_grow_with_the_resolution(size: int) -> None:
    """Every pixel of a bright area radiates, and the area has more pixels the higher the resolution: the same
    recipe lights the same total (per design px squared) at any size."""
    assert square_light(size) == pytest.approx(square_light(480), rel=0.05)


def test_variation_dims_points_by_a_random_share_between_one_minus_variation_and_one() -> None:
    alike = lattice_gains(variation=0.0, seed=2)
    varied = lattice_gains(variation=0.5, seed=2)
    np.testing.assert_allclose(alike, 1.0, atol=1e-4)
    assert varied.min() >= 0.5 - 1e-3
    assert varied.max() <= 1.0 + 1e-3
    assert varied.min() < 0.6  # and it uses the range
    assert varied.max() > 0.9
    assert varied.std() > 0.1


def test_the_preset_sparkle_scatters_glints_over_a_bright_area_where_plain_spikes_flood_it() -> None:
    """A bright block: the preset's spikes without its spacing wrap the whole block in light; with it a few small
    separate glints are lit."""
    picture = np.zeros((SIZE, SIZE, 3), np.float32)
    picture[60:140, 60:140] = 0.95
    sparkle = registry.presets(NAME)["sparkle"].params
    glitter = registry.apply(NAME, picture, **sparkle, seed=1)
    flood = registry.apply(NAME, picture, **{**sparkle, "spacing": 0.0})
    lit_glitter = (glitter - picture).max(axis=-1) > 0.02
    lit_flood = (flood - picture).max(axis=-1) > 0.02
    assert lit_glitter.any()
    assert lit_glitter.sum() < lit_flood.sum() / 4


@pytest.mark.parametrize(("spacing", "design_spacing"), [(0.0, 0.0), (25.8, 12.9)], ids=["every point", "a random few"])
def test_a_call_with_size_ratio_equals_the_recipe_rendered_at_that_ratio(spacing: float, design_spacing: float) -> None:
    """A recipe rendered at 960 hands the effect `size_ratio` 2 and doubles its px values; calling the effect directly
    with those values and `size_ratio=2` gives the same picture."""
    picture = np.zeros((960, 960, 3), np.float32)
    picture[300:660, 300:660] = 0.9
    step = fx.starburst(length=15.0, width=1.0, threshold=0.5, spacing=design_spacing, seed=3, variation=0.3)
    by_recipe = render(picture, Recipe(name="two", steps=[step]), size=960)
    by_call = registry.apply(
        NAME, picture, length=30.0, width=2.0, threshold=0.5, spacing=spacing, seed=3, variation=0.3, size_ratio=2.0
    )
    np.testing.assert_array_equal(by_call, by_recipe)
    assert not np.array_equal(
        by_call, registry.apply(NAME, picture, length=30.0, width=2.0, threshold=0.5)
    )  # it matters


def test_a_call_without_a_recipe_has_a_size_ratio_of_1() -> None:
    picture = np.zeros((SIZE, SIZE, 3), np.float32)
    picture[60:140, 60:140] = 0.9
    plain = registry.apply(NAME, picture, threshold=0.5, length=8.0)
    np.testing.assert_array_equal(plain, registry.apply(NAME, picture, threshold=0.5, length=8.0, size_ratio=1.0))
    assert not np.array_equal(plain, registry.apply(NAME, picture, threshold=0.5, length=8.0, size_ratio=2.0))
