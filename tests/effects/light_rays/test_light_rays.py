"""Behaviour of the `light_rays` effect: the highlights streaked away from a point and screened over the picture."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "light_rays"
SIZE = 101
MIDDLE = SIZE // 2  # the centre pixel (50, 50): x = y = 0.5


def point(row: int, col: int, value: float = 1.0, channel: slice | int = slice(None)) -> np.ndarray:
    """A black SIZE x SIZE picture with one lit pixel."""
    dark = np.zeros((SIZE, SIZE, 3), np.float32)
    dark[row, col, channel] = value
    return dark


def shine(rgb: np.ndarray, **settings: object) -> np.ndarray:
    """The effect with its light in the middle of the picture, unless the settings say where."""
    return registry.apply(NAME, rgb, **{"x": 0.5, "y": 0.5, **settings})


def lit(out: np.ndarray, floor: float = 1e-3) -> tuple[np.ndarray, ...]:
    """Rows and columns of the pixels of channel 0 above `floor`."""
    return np.nonzero(out[..., 0] > floor)


def test_rays_run_from_a_bright_point_along_the_line_away_from_the_light() -> None:
    """Brute force on an impulse 20 px right of and 10 px above the light: every lit pixel lies on the line from the
    light through the point (give or take the blending of neighbours), not nearer the light than the point itself
    (less one px) and no farther than where the last reading reaches it (length 0.4: 1 / (1 - 0.4) = 1.67 times its
    distance); and the line is lit beyond the point."""
    row, col = MIDDLE - 10, MIDDLE + 20
    out = shine(point(row, col), x=0.5, y=0.5, length=0.4, samples=128, threshold=0.5, decay=1.0, amount=1.0)
    lit_rows, lit_cols = lit(out)
    down, right = lit_rows - MIDDLE, lit_cols - MIDDLE
    towards = np.array([-10.0, 20.0]) / np.hypot(10, 20)  # unit direction (row, column) from the light to the point
    along = down * towards[0] + right * towards[1]
    across = np.abs(down * towards[1] - right * towards[0])
    distance = np.hypot(10, 20)
    # bilinear reading blends the four pixels round a point, so a reading up to 1.5 px (the diagonal) off the line still
    # sees the point, and the farthest pixels magnify that by 1 / 0.6
    assert (across <= 1.5 / (1 - 0.4)).all()
    assert (along >= distance - 1.0).all()
    assert (along <= distance / (1 - 0.4) + 1.0).all()
    assert along.max() >= distance / (1 - 0.4) - 3.0  # the whole length is drawn
    assert (along > distance + 5).sum() >= 10


@pytest.mark.parametrize(
    ("x", "y", "side"),
    [(0.1, 0.5, "right"), (0.9, 0.5, "left"), (0.5, 0.1, "below"), (0.5, 0.9, "above")],
    ids=["light left of the point", "light right of it", "light above it", "light below it"],
)
def test_the_rays_point_away_from_x_and_y(x: float, y: float, side: str) -> None:
    out = shine(point(MIDDLE, MIDDLE), x=x, y=y, length=0.3, samples=64, threshold=0.5)
    rows, cols = lit(out)
    centroid = (rows.mean() - MIDDLE, cols.mean() - MIDDLE)
    expected = {"right": (0, 1), "left": (0, -1), "below": (1, 0), "above": (-1, 0)}[side]
    assert (
        centroid[0] * expected[0] + centroid[1] * expected[1] > 2.0
    )  # the light's streaks lie that way from the point
    assert abs(centroid[0] * expected[1] - centroid[1] * expected[0]) < 1.0  # and on the line from the light


@pytest.mark.parametrize("interpolation", ["nearest", "bilinear", "bicubic"])
@pytest.mark.parametrize("decay", [0.5, 1.0])
def test_a_ray_is_the_decay_weighted_average_of_the_readings_toward_the_light(interpolation: str, decay: float) -> None:
    """Two readings at scales 1 and 1 - 0.5 x 1/2 = 0.75 about the light, weighted 1 and `decay` (adding up to 1 after
    scaling): a point of 0.5 (all of it highlight over threshold 0), 30 px right of the light, is read by itself and
    by the pixel 30 / 0.75 = 40 px right. Worked out by hand, amount 1: the light is 0.5 / (1 + decay) at the point,
    screened over its own 0.5, and 0.5 decay / (1 + decay) at 40 px, over black."""
    out = shine(
        point(MIDDLE, MIDDLE + 30, 0.5), length=0.5, samples=2, threshold=0.0, amount=1.0, decay=decay,
        interpolation=interpolation,
    )  # fmt: skip
    assert out[MIDDLE, MIDDLE + 30, 0] == pytest.approx(1 - 0.5 * (1 - 0.5 / (1 + decay)), abs=1e-5)
    assert out[MIDDLE, MIDDLE + 40, 0] == pytest.approx(0.5 * decay / (1 + decay), abs=1e-5)


def test_amount_scales_the_light_of_the_rays() -> None:
    weak = shine(point(MIDDLE, MIDDLE + 20, 0.6), length=0.4, samples=32, threshold=0.5, amount=0.5)
    strong = shine(point(MIDDLE, MIDDLE + 20, 0.6), length=0.4, samples=32, threshold=0.5, amount=1.5)
    streak = (MIDDLE, slice(MIDDLE + 24, MIDDLE + 32), 0)
    assert weak[streak].min() > 0.001  # on the streak, where nothing saturates
    np.testing.assert_allclose(strong[streak], 3 * weak[streak], rtol=2e-3)


def test_decay_dims_the_far_end_of_the_rays() -> None:
    """Even with decay 1 a ray thins out away from its point (the same light spreads over more pixels), so the far
    end is compared with the near end of the even ray: decay 0.8 makes it much fainter still."""
    long_ray = {"length": 0.5, "samples": 48, "threshold": 0.5, "amount": 1.0}
    flat = shine(point(MIDDLE, MIDDLE + 20), decay=1.0, **long_ray)[MIDDLE, :, 0]
    fading = shine(point(MIDDLE, MIDDLE + 20), decay=0.8, **long_ray)[MIDDLE, :, 0]
    near, far = slice(MIDDLE + 22, MIDDLE + 26), slice(MIDDLE + 36, MIDDLE + 40)
    assert flat[far].mean() / flat[near].mean() > 0.4
    assert fading[far].mean() / fading[near].mean() < 0.1 * (flat[far].mean() / flat[near].mean())


def test_a_single_reading_adds_the_highlights_where_they_are_and_nothing_else() -> None:
    """One sample reads the picture as it is: 0.8 over a threshold of 0.6 is 0.5 of highlight, light 0.5 x 0.6 = 0.3
    screened over 0.8: 1 - (1 - 0.8)(1 - 0.3) = 0.86."""
    out = shine(point(MIDDLE, MIDDLE + 20, 0.8), samples=1, threshold=0.6, amount=0.6)
    assert out[MIDDLE, MIDDLE + 20, 0] == pytest.approx(0.86, abs=1e-5)
    out[MIDDLE, MIDDLE + 20] = 0
    assert out.max() == 0.0


@pytest.mark.parametrize(
    ("value", "threshold", "rays"),
    [(0.5, 0.6, False), (0.7, 0.6, True), (1.0, 0.99, True)],
    ids=["below the threshold", "above it", "white over the highest threshold"],
)
def test_only_what_is_brighter_than_threshold_makes_rays(value: float, threshold: float, rays: bool) -> None:
    dark = point(MIDDLE, MIDDLE + 20, value)
    out = shine(dark, threshold=threshold, length=0.5, samples=64, amount=1.0)
    streak = out[MIDDLE, MIDDLE + 24 : MIDDLE + 40, 0]
    assert bool(streak.max() > 1e-3) is rays


def test_black_stays_black_and_amount_zero_changes_nothing(photo: np.ndarray) -> None:
    black = np.zeros_like(photo)
    np.testing.assert_array_equal(shine(black), black)
    np.testing.assert_array_equal(shine(photo, amount=0.0, threshold=0.3), photo)


@pytest.mark.parametrize(
    "settings",
    [{}, {"x": 0.0, "y": 1.0, "length": 1.0}, {"threshold": 0.2, "amount": 4.0, "decay": 0.3, "samples": 7}],
    ids=["defaults", "from a corner, to the light", "bright and few"],
)
def test_the_picture_only_ever_gets_brighter(settings: dict, photo: np.ndarray) -> None:
    out = shine(photo, **settings)
    assert (out >= photo - 1e-6).all()


def test_it_brightens_a_photo_at_its_defaults(photo: np.ndarray) -> None:
    assert shine(photo).mean() > photo.mean() + 0.005


def test_each_colour_channel_streaks_on_its_own() -> None:
    out = shine(point(MIDDLE, MIDDLE + 20, 1.0, channel=0), threshold=0.5, length=0.4, samples=32)
    assert out[..., 0].max() > 0.5
    assert out[..., 1:].max() == 0.0


def test_a_light_in_a_corner_streaks_across_the_whole_picture_without_error(photo: np.ndarray) -> None:
    out = shine(photo, x=1.0, y=1.0, length=1.0, samples=24, edge="wrap", interpolation="bicubic")
    assert out.shape == photo.shape


def test_an_unknown_edge_rule_is_refused(photo: np.ndarray) -> None:
    with pytest.raises(ValueError, match="`edge`"):
        shine(photo, edge="constant")


def test_nearest_reads_whole_pixels_where_bilinear_and_bicubic_blend_them() -> None:
    """With decay 1 and amount 1 every reading weighs 1/64, so light that comes only from whole pixels is a whole
    number of 64ths; blended readings are not."""
    dot = point(MIDDLE - 7, MIDDLE + 13, 0.9)
    settings = {"length": 0.5, "samples": 64, "threshold": 0.0, "amount": 1.0, "decay": 1.0}
    far = (slice(None), slice(MIDDLE + 20, None), 0)  # away from the point itself, where the light is only streaks
    in_64ths = {
        choice: shine(dot, interpolation=choice, **settings)[far] * 64 / 0.9
        for choice in ("nearest", "bilinear", "bicubic")
    }
    whole = {choice: np.abs(light - np.round(light)).max() for choice, light in in_64ths.items()}
    assert whole["nearest"] < 1e-3
    assert whole["bilinear"] > 0.05
    assert whole["bicubic"] > 0.05


@pytest.mark.parametrize(
    ("interpolation", "edge_matters"),
    [("nearest", False), ("bilinear", False), ("bicubic", True)],
    ids=["nearest never reads beyond the edge", "bilinear neither", "bicubic's spline fit does"],
)
def test_the_edge_rule_only_matters_to_the_spline_fit_of_bicubic_reading(
    interpolation: str, edge_matters: bool
) -> None:
    noise = np.random.default_rng(4).random((40, 60, 3), dtype=np.float32)
    settings = {"interpolation": interpolation, "threshold": 0.3, "length": 0.6, "samples": 8}
    mirrored, wrapped = (shine(noise, edge=edge, **settings) for edge in ("mirror", "wrap"))
    assert bool(np.abs(mirrored - wrapped).max() > 1e-4) is edge_matters
