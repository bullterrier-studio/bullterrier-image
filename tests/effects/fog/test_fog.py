"""Behaviour of the `fog` effect: the picture mixed toward a colour by a noise-shaped, bottom-heavy opacity map."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "fog"
SMOKE = registry.presets(NAME)["smoke"].params


def veil(settings: dict, shape: tuple[int, int] = (192, 192)) -> np.ndarray:
    """The opacity map (h, w): white fog over a black picture shows exactly its opacity."""
    black = np.zeros((*shape, 3), np.float32)
    return registry.apply(NAME, black, **{**settings, "color": "#ffffff"})[..., 0].astype(np.float64)


def plumes(settings: dict, shape: tuple[int, int] = (192, 192)) -> np.ndarray:
    """The opacity map of the `smoke` preset with `settings` changed."""
    return veil({**SMOKE, **settings}, shape)


def correlation(first: np.ndarray, second: np.ndarray) -> float:
    return float(np.corrcoef(first.ravel(), second.ravel())[0, 1])


def autocorrelation(field: np.ndarray, lag: int, axis: int = 1) -> float:
    """Correlation of the field with itself `lag` pixels along `axis` (1 = across the columns, 0 = down the rows)."""
    a, b = (field[:, :-lag], field[:, lag:]) if axis == 1 else (field[:-lag], field[lag:])
    return correlation(a, b)


def test_amount_zero_leaves_the_picture_unchanged(photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, amount=0.0)
    np.testing.assert_array_equal(out, photo)
    assert not np.shares_memory(out, photo)


@pytest.mark.parametrize("amount", [0.25, 0.6, 1.0])
def test_a_floor_of_1_makes_an_even_veil_that_mixes_every_pixel_toward_the_colour_by_the_amount(
    amount: float, photo: np.ndarray
) -> None:
    """With no gradient and the floor at 1 the opacity is `amount` everywhere: out = picture + amount x (fog -
    picture), which also shrinks the picture's contrast by 1 - amount."""
    fog = np.array([0.8, 0.5, 0.2], np.float32)
    out = registry.apply(NAME, photo, amount=amount, color=fog.tolist(), gradient=0.0, floor=1.0)
    np.testing.assert_allclose(out, photo + amount * (fog - photo), atol=1e-5)
    assert out.std(axis=(0, 1)) == pytest.approx((1 - amount) * photo.std(axis=(0, 1)), abs=1e-3)


@pytest.mark.parametrize("value", [0.0, 1.0], ids=["black picture", "white picture"])
def test_a_full_even_veil_hides_the_picture_behind_its_colour(value: float) -> None:
    fog = [0.2, 0.4, 0.6]
    picture = np.full((32, 32, 3), value, np.float32)
    out = registry.apply(NAME, picture, amount=1.0, color=fog, gradient=0.0, floor=1.0)
    np.testing.assert_allclose(out, np.broadcast_to(fog, (32, 32, 3)), atol=1e-6)


@pytest.mark.parametrize(
    ("gradient", "opacity_by_row"),
    [
        pytest.param(0.0, [0.5, 0.5, 0.5, 0.5], id="gradient 0 is even"),
        pytest.param(0.5, [0.28125, 0.34375, 0.40625, 0.46875], id="gradient 0.5 halves the range"),
        pytest.param(1.0, [0.0625, 0.1875, 0.3125, 0.4375], id="gradient 1 fades to nothing at the top"),
    ],
)
def test_the_gradient_thickens_the_fog_toward_the_bottom(gradient: float, opacity_by_row: list[float]) -> None:
    """Row centres of a 4-row picture sit at t = 1/8, 3/8, 5/8, 7/8 of the way down; with amount 0.5 the opacity is
    0.5 x (1 - gradient x (1 - t)), worked out by hand for each row in the table."""
    out = veil({"amount": 0.5, "gradient": gradient, "floor": 1.0}, shape=(4, 3))
    np.testing.assert_allclose(out, np.repeat(opacity_by_row, 3).reshape(4, 3), atol=1e-6)


def test_with_a_gradient_the_fog_is_thicker_in_the_lower_rows_on_average() -> None:
    opacity = veil({"amount": 0.8, "gradient": 1.0, "floor": 0.5, "seed": 3})
    rows = opacity.mean(axis=1)
    assert rows[-48:].mean() > 2 * rows[:48].mean()


@pytest.mark.parametrize("floor", [0.0, 0.3, 0.8, 1.0])
def test_the_floor_is_the_opacity_where_there_is_no_cloud_as_a_share_of_the_amount(floor: float) -> None:
    """Where the shape is 0 (`coverage` 0: nowhere above the cut) the opacity is `amount` x `floor`; with cloud the
    opacity climbs from there to `amount`."""
    still = {"amount": 0.6, "gradient": 0.0, "scale": 8.0, "seed": 2}
    np.testing.assert_allclose(veil({**still, "coverage": 0.0, "floor": floor}), 0.6 * floor, atol=1e-6)
    cloudy = veil({**still, "coverage": 0.5, "feather": 0.2, "floor": floor})
    assert cloudy.min() == pytest.approx(0.6 * floor, abs=1e-6)
    assert cloudy.max() == pytest.approx(0.6, abs=1e-4)


def test_a_floor_keeps_a_veil_over_the_gaps_that_a_floor_of_0_opens() -> None:
    """Every pixel is at least `amount` x `floor` x the row's gradient factor, 1 - gradient x (1 - t) by hand."""
    settings = {"amount": 0.8, "gradient": 0.5, "coverage": 0.5, "scale": 12.0, "seed": 1}
    height = 96
    factor = 1 - 0.5 * (1 - (np.arange(height) + 0.5) / height)
    veiled = veil({**settings, "floor": 0.4}, shape=(height, 96))
    assert (veiled >= (0.8 * 0.4 * factor)[:, None] - 1e-6).all()
    assert (veil({**settings, "floor": 0.0}, shape=(height, 96)) < 1e-6).mean() > 0.2


def test_the_smoke_preset_has_clear_air_between_plumes_that_reach_the_amount() -> None:
    opacity = plumes({})
    assert (opacity == 0).mean() > 0.4
    assert opacity.max() == pytest.approx(SMOKE["amount"], abs=0.01)


def test_the_smoke_preset_is_thickest_at_the_bottom_and_thinner_toward_the_top() -> None:
    """With the floor raised to 1 the cloud shape drops out and only the row-by-row gradient is left."""
    rows = plumes({"floor": 1.0}).mean(axis=1)
    assert rows[0] < 0.5 * rows[-1]
    assert rows[-1] == pytest.approx(SMOKE["amount"], abs=0.01)
    assert (np.diff(rows) > 0).all()


@pytest.mark.parametrize("coverage", [0.25, 0.5, 0.8])
def test_coverage_is_about_the_share_of_the_picture_that_has_cloud(coverage: float) -> None:
    opacity = plumes({"coverage": coverage, "scale": 8.0, "turbulence": 0.0, "rise": 1.0, "seed": 4})
    assert (opacity > 0).mean() == pytest.approx(coverage, abs=0.06)


@pytest.mark.parametrize("seed", [1, 2, 3])
def test_coverage_still_holds_once_the_clouds_are_drawn_out_and_swirled(seed: int) -> None:
    """The warped noise is respread evenly, so the share with cloud follows `coverage` however much it is moved."""
    opacity = plumes({"coverage": 0.45, "rise": 4.0, "turbulence": 60.0, "scale": 40.0, "seed": seed})
    assert (opacity > 0).mean() == pytest.approx(0.45, abs=0.06)


@pytest.mark.parametrize(("coverage", "expected"), [(0.0, 0.0), (1.0, 1.0)], ids=["no coverage", "full coverage"])
def test_the_extremes_of_coverage_are_a_clear_picture_and_cloud_everywhere(coverage: float, expected: float) -> None:
    opacity = plumes({"coverage": coverage, "scale": 8.0, "seed": 4})
    assert (opacity > 0).mean() == pytest.approx(expected, abs=0.01)


@pytest.mark.parametrize("feather", [0.05, 0.2, 0.4])
def test_feather_is_the_share_of_the_picture_on_the_soft_edge_of_the_cloud(feather: float) -> None:
    """Opacity rises from the floor to `amount` across the `feather` share of pixels just above the coverage cut, so
    that share of the picture is neither at the floor nor at full opacity; the rest is at one or the other."""
    opacity = plumes(
        {
            "amount": 0.5,
            "gradient": 0.0,
            "floor": 0.0,
            "coverage": 0.5,
            "feather": feather,
            "scale": 4.0,
            "turbulence": 0.0,
            "rise": 1.0,
            "seed": 2,
        }
    )
    soft = ((opacity > 1e-6) & (opacity < 0.5 - 1e-6)).mean()
    assert soft == pytest.approx(feather, abs=0.05)
    assert opacity.max() == pytest.approx(0.5, abs=1e-4)


@pytest.mark.parametrize("floor", [0.0, 0.5], ids=["no floor", "half floor"])
def test_the_soft_edge_follows_a_smoothstep_curve_from_the_floor_to_the_amount(floor: float) -> None:
    """Across the ramp, (opacity - amount x floor) / (amount x (1 - floor)) = 3t^2 - 2t^3 with t running evenly 0..1
    over the ramp's pixels. By hand, that reaches a quarter at t = 0.326, so 1 - 0.326 = 0.674 of the ramp's pixels are
    above it (a straight ramp would give 0.75), and half of them are above one half."""
    amount = 0.8
    opacity = plumes(
        {
            "amount": amount,
            "gradient": 0.0,
            "octaves": 4,
            "coverage": 0.5,
            "feather": 0.3,
            "floor": floor,
            "scale": 4.0,
            "turbulence": 0.0,
            "rise": 1.0,
            "seed": 2,
        },
        shape=(256, 256),
    )
    low = amount * floor
    ramp = (opacity[(opacity > low + 1e-6) & (opacity < amount - 1e-6)] - low) / (amount - low)
    assert (ramp > 1 / 4).mean() == pytest.approx(0.674, abs=0.04)
    assert (ramp > 1 / 2).mean() == pytest.approx(0.5, abs=0.04)


def test_the_same_seed_gives_the_same_fog_and_another_seed_another() -> None:
    first = plumes({"seed": 1})
    np.testing.assert_array_equal(first, plumes({"seed": 1}))
    assert np.abs(first - plumes({"seed": 2})).mean() > 0.02


def test_a_larger_scale_gives_broader_clouds() -> None:
    fine, medium, coarse = (
        autocorrelation(plumes({"scale": scale, "octaves": 1, "turbulence": 0.0, "rise": 1.0}), lag=4)
        for scale in (2.0, 8.0, 32.0)
    )
    assert fine < 0.5 < medium < coarse
    assert coarse > 0.9


@pytest.mark.parametrize(
    ("smoother", "rougher"),
    [
        pytest.param({"octaves": 1}, {"octaves": 4, "persistence": 0.8}, id="more octaves add fine detail"),
        pytest.param({"persistence": 0.1}, {"persistence": 0.9}, id="more persistence keeps the fine layers strong"),
        pytest.param({"lacunarity": 1.2}, {"lacunarity": 3.0}, id="more lacunarity makes the layers finer"),
    ],
)
def test_the_clouds_are_layered_by_octaves_persistence_and_lacunarity(smoother: dict, rougher: dict) -> None:
    base = {"scale": 24.0, "octaves": 3, "persistence": 0.8, "lacunarity": 2.0, "turbulence": 0.0, "rise": 1.0}
    smooth, rough = (plumes({**base, **change, "seed": 1}) for change in (smoother, rougher))
    assert autocorrelation(rough, lag=1) < autocorrelation(smooth, lag=1) - 0.01


@pytest.mark.parametrize(
    ("rise", "longer_down_than_across"),
    [(1.0, False), (4.0, True)],
    ids=["rise 1 is isotropic", "rise 4 stretches the clouds upward"],
)
def test_rise_stretches_the_clouds_upward(rise: float, longer_down_than_across: bool) -> None:
    opacity = plumes({"scale": 12.0, "octaves": 1, "turbulence": 0.0, "rise": rise, "seed": 3}, shape=(256, 256))
    along, across = autocorrelation(opacity, lag=6, axis=0), autocorrelation(opacity, lag=6, axis=1)
    if longer_down_than_across:
        assert along > across + 0.1
    else:
        assert along == pytest.approx(across, abs=0.1)


def test_the_clouds_are_drawn_out_from_the_bottom_edge() -> None:
    """With rise 3 the bottom row still reads the noise where it is; the top row reads it from two thirds down."""
    base = {"scale": 12.0, "octaves": 1, "turbulence": 0.0, "seed": 3}
    plain, drawn = plumes({**base, "rise": 1.0}), plumes({**base, "rise": 3.0})
    assert correlation(plain[-1], drawn[-1]) > 0.97
    assert correlation(plain[0], drawn[0]) < 0.5


def test_more_turbulence_moves_the_clouds_further_from_where_the_plain_noise_has_them() -> None:
    base = {"scale": 40.0, "octaves": 1, "rise": 1.0, "seed": 5}
    plain = plumes({**base, "turbulence": 0.0})
    kept = [correlation(plain, plumes({**base, "turbulence": px})) for px in (4.0, 16.0, 64.0)]
    assert kept[0] > kept[1] > kept[2]
    assert kept[0] > 0.8
    assert kept[2] < 0.5


@pytest.mark.parametrize("resampling", ["nearest", "bilinear", "bicubic"])
@pytest.mark.parametrize("edge", ["reflect", "nearest", "mirror", "wrap"])
def test_reading_rule_and_edge_rule_change_nothing_when_no_cloud_is_moved(resampling: str, edge: str) -> None:
    """With turbulence 0 and rise 1 every pixel reads the noise exactly where it is."""
    still = {"turbulence": 0.0, "rise": 1.0, "seed": 2}
    reference = plumes(still)
    np.testing.assert_allclose(plumes({**still, "resampling": resampling, "edge": edge}), reference, atol=1e-5)


def test_the_edge_rule_decides_what_clouds_dragged_in_from_beyond_the_border_look_like() -> None:
    settings = {"scale": 30.0, "turbulence": 24.0, "rise": 1.0, "seed": 6}
    reflect, wrap = plumes({**settings, "edge": "reflect"}), plumes({**settings, "edge": "wrap"})
    inside, border = (slice(48, -48), slice(48, -48)), (slice(0, 6), slice(None))
    assert correlation(reflect[inside], wrap[inside]) > 0.99
    assert correlation(reflect[border], wrap[border]) < 0.9


def test_the_reading_rule_changes_how_the_moved_clouds_are_read_but_not_where_they_are() -> None:
    settings = {"scale": 30.0, "turbulence": 24.0, "rise": 1.0, "seed": 6}
    nearest, bilinear = (plumes({**settings, "resampling": mode}) for mode in ("nearest", "bilinear"))
    assert np.abs(nearest - bilinear).mean() > 1e-3
    assert correlation(nearest, bilinear) > 0.9  # the same clouds, read more crudely
