"""Behaviour of the `smoke` effect: soft-edged plumes of noise, stretched upward and swirled, mixed in as a colour."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "smoke"


def plumes(settings: dict, shape: tuple[int, int] = (192, 192)) -> np.ndarray:
    """The smoke's opacity map (h, w): white smoke over a black picture shows exactly its opacity."""
    black = np.zeros((*shape, 3), np.float32)
    return registry.apply(NAME, black, color="#ffffff", **settings)[..., 0].astype(np.float64)


def correlation(first: np.ndarray, second: np.ndarray) -> float:
    return float(np.corrcoef(first.ravel(), second.ravel())[0, 1])


def autocorrelation(field: np.ndarray, lag: int, axis: int = 1) -> float:
    """Correlation of the field with itself `lag` pixels along `axis` (1 = across the columns, 0 = down the rows)."""
    a, b = (field[:, :-lag], field[:, lag:]) if axis == 1 else (field[:-lag], field[lag:])
    return correlation(a, b)


def test_smoke_mixes_each_pixel_toward_the_colour_by_an_opacity_between_0_and_amount(photo: np.ndarray) -> None:
    """out = picture + opacity x (smoke - picture): so every channel lies between the picture and the smoke colour, and
    where the picture already is the smoke colour nothing changes."""
    smoke = np.array([0.9, 0.8, 0.7], np.float32)
    out = registry.apply(NAME, photo, amount=0.6, color=smoke.tolist(), seed=1)
    opacity = (out - photo) / (smoke - photo)  # the same in every channel, where the picture is far from the smoke
    far = np.abs(smoke - photo) > 0.1
    assert far.mean() > 0.5
    assert opacity[far].min() >= -1e-3
    assert opacity[far].max() <= 0.6 + 1e-3
    assert opacity[far].max() > 0.55  # and the smoke does reach its full amount somewhere
    for channel in (1, 2):
        both = far[..., 0] & far[..., channel]
        np.testing.assert_allclose(opacity[..., channel][both], opacity[..., 0][both], atol=2e-3)
    flat = np.broadcast_to(smoke, photo.shape).copy()
    np.testing.assert_allclose(registry.apply(NAME, flat, amount=0.6, color=smoke.tolist()), flat, atol=1e-6)


@pytest.mark.parametrize("coverage", [0.25, 0.5, 0.8])
def test_coverage_is_about_the_share_of_the_picture_that_has_smoke(coverage: float) -> None:
    opacity = plumes({"amount": 0.7, "coverage": coverage, "scale": 8.0, "turbulence": 0.0, "rise": 1.0, "seed": 4})
    assert (opacity > 0).mean() == pytest.approx(coverage, abs=0.06)


@pytest.mark.parametrize(("coverage", "expected"), [(0.0, 0.0), (1.0, 1.0)], ids=["no coverage", "full coverage"])
def test_the_extremes_of_coverage_are_a_clear_picture_and_smoke_everywhere(coverage: float, expected: float) -> None:
    opacity = plumes({"amount": 0.7, "coverage": coverage, "scale": 8.0, "seed": 4})
    assert (opacity > 0).mean() == pytest.approx(expected, abs=0.01)


@pytest.mark.parametrize("feather", [0.05, 0.2, 0.4])
def test_feather_is_the_share_of_the_picture_on_the_soft_edge_of_the_plumes(feather: float) -> None:
    """Opacity rises from 0 to `amount` across the `feather` share of pixels just above the coverage cut, so that share
    of the picture is neither clear nor at full opacity; the rest is clear or full."""
    opacity = plumes(
        {"amount": 0.7, "coverage": 0.5, "feather": feather, "scale": 4.0, "turbulence": 0.0, "rise": 1.0, "seed": 2}
    )
    soft = ((opacity > 1e-6) & (opacity < 0.7 - 1e-6)).mean()
    assert soft == pytest.approx(feather, abs=0.05)
    assert opacity.max() == pytest.approx(0.7, abs=1e-4)


def test_the_soft_edge_follows_a_smoothstep_curve() -> None:
    """Across the ramp, opacity = amount x (3t^2 - 2t^3) with t running evenly 0..1 over the ramp's pixels. By hand,
    that reaches a quarter of `amount` at t = 0.326, so 1 - 0.326 = 0.674 of the ramp's pixels are above it (a straight
    ramp would give 0.75), and half of them are above half of `amount`."""
    opacity = plumes(
        {"amount": 0.8, "coverage": 0.5, "feather": 0.3, "scale": 4.0, "turbulence": 0.0, "rise": 1.0, "seed": 2},
        shape=(256, 256),
    )
    ramp = opacity[(opacity > 1e-6) & (opacity < 0.8 - 1e-6)]
    assert (ramp > 0.8 / 4).mean() == pytest.approx(0.674, abs=0.04)
    assert (ramp > 0.8 / 2).mean() == pytest.approx(0.5, abs=0.04)


def test_the_same_seed_gives_the_same_smoke_and_another_seed_another() -> None:
    first = plumes({"seed": 1})
    np.testing.assert_array_equal(first, plumes({"seed": 1}))
    assert np.abs(first - plumes({"seed": 2})).mean() > 0.02


def test_a_larger_scale_gives_broader_plumes() -> None:
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
def test_the_plumes_are_layered_by_octaves_persistence_and_lacunarity(smoother: dict, rougher: dict) -> None:
    base = {"scale": 24.0, "octaves": 3, "persistence": 0.8, "lacunarity": 2.0, "turbulence": 0.0, "rise": 1.0}
    smooth, rough = (plumes({**base, **change, "seed": 1}) for change in (smoother, rougher))
    assert autocorrelation(rough, lag=1) < autocorrelation(smooth, lag=1) - 0.01


@pytest.mark.parametrize(
    ("rise", "longer_down_than_across"),
    [(1.0, False), (4.0, True)],
    ids=["rise 1 is isotropic", "rise 4 stretches the plumes upward"],
)
def test_rise_stretches_the_plumes_upward(rise: float, longer_down_than_across: bool) -> None:
    opacity = plumes({"scale": 12.0, "octaves": 1, "turbulence": 0.0, "rise": rise, "seed": 3}, shape=(256, 256))
    along, across = autocorrelation(opacity, lag=6, axis=0), autocorrelation(opacity, lag=6, axis=1)
    if longer_down_than_across:
        assert along > across + 0.1
    else:
        assert along == pytest.approx(across, abs=0.1)


def test_the_plumes_are_drawn_out_from_the_bottom_edge() -> None:
    """With rise 3 the bottom row still reads the noise where it is; the top row reads it from two thirds down."""
    base = {"scale": 12.0, "octaves": 1, "turbulence": 0.0, "seed": 3}
    plain, drawn = plumes({**base, "rise": 1.0}), plumes({**base, "rise": 3.0})
    assert correlation(plain[-1], drawn[-1]) > 0.97
    assert correlation(plain[0], drawn[0]) < 0.5


def test_more_turbulence_moves_the_plumes_further_from_where_the_plain_noise_has_them() -> None:
    base = {"scale": 40.0, "octaves": 1, "rise": 1.0, "seed": 5}
    plain = plumes({**base, "turbulence": 0.0})
    kept = [correlation(plain, plumes({**base, "turbulence": px})) for px in (4.0, 16.0, 64.0)]
    assert kept[0] > kept[1] > kept[2]
    assert kept[0] > 0.8
    assert kept[2] < 0.5


@pytest.mark.parametrize("resampling", ["nearest", "bilinear", "bicubic"])
@pytest.mark.parametrize("edge", ["reflect", "nearest", "mirror", "wrap"])
def test_reading_rule_and_edge_rule_change_nothing_when_no_plume_is_moved(resampling: str, edge: str) -> None:
    """With turbulence 0 and rise 1 every pixel reads the noise exactly where it is."""
    still = {"turbulence": 0.0, "rise": 1.0, "seed": 2}
    reference = plumes(still)
    np.testing.assert_allclose(plumes({**still, "resampling": resampling, "edge": edge}), reference, atol=1e-5)


def test_the_edge_rule_decides_what_plumes_dragged_in_from_beyond_the_border_look_like() -> None:
    settings = {"scale": 30.0, "turbulence": 24.0, "rise": 1.0, "seed": 6}
    reflect, wrap = plumes({**settings, "edge": "reflect"}), plumes({**settings, "edge": "wrap"})
    inside, border = (slice(48, -48), slice(48, -48)), (slice(0, 6), slice(None))
    assert correlation(reflect[inside], wrap[inside]) > 0.99
    assert correlation(reflect[border], wrap[border]) < 0.9


def test_the_reading_rule_changes_how_the_moved_plumes_are_read_but_not_where_they_are() -> None:
    settings = {"scale": 30.0, "turbulence": 24.0, "rise": 1.0, "seed": 6}
    nearest, bilinear = (plumes({**settings, "resampling": mode}) for mode in ("nearest", "bilinear"))
    assert np.abs(nearest - bilinear).mean() > 1e-3
    assert correlation(nearest, bilinear) > 0.9  # the same plumes, read more crudely
