"""Behaviour of the `aurora` effect: soft vertical light curtains, brightest at a wavy base, over a dark sky."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "aurora"
WHITE = ["#ffffff", "#ffffff"]
FLAT = {"colors": WHITE, "sky": "#000000", "waviness": 0.0}  # a white curtain on black: grey level = its brightness


def curtain(settings: dict, shape: tuple[int, int] = (100, 120)) -> np.ndarray:
    """The curtain's brightness as a grey picture (h, w), 0 = sky, 1 = full light, for a white curtain on black sky."""
    out = registry.apply(NAME, np.zeros((*shape, 3), np.float32), **{**FLAT, **settings})
    return out[..., 0].astype(np.float64)


def autocorrelation(values: np.ndarray, lag: int, axis: int) -> float:
    a, b = (values[:, :-lag], values[:, lag:]) if axis == 1 else (values[:-lag], values[lag:])
    return float(np.corrcoef(a.ravel(), b.ravel())[0, 1])


@pytest.mark.parametrize("size", [(40, 64), (97, 33)], ids=["wide", "tall"])
def test_the_picture_takes_only_its_size_from_the_input(size: tuple[int, int]) -> None:
    dark, light = np.zeros((*size, 3), np.float32), np.ones((*size, 3), np.float32)
    out = registry.apply(NAME, dark, seed=3)
    assert out.shape == (*size, 3)
    np.testing.assert_array_equal(out, registry.apply(NAME, light, seed=3))


def test_the_sky_is_dark_below_the_curtain_base_and_above_the_curtain_top() -> None:
    """Base at 80 % of 100 rows, curtain 30 rows tall: rows from 81 down and rows above row 50 are the sky colour."""
    out = registry.apply(
        NAME,
        np.zeros((100, 60, 3), np.float32),
        base=0.8,
        height=0.3,
        waviness=0.0,
        rays=0.0,
        sky="#102030",
        colors=["#ffffff", "#ffffff"],
    )
    sky = np.array([0x10, 0x20, 0x30], np.float32) / 255
    np.testing.assert_allclose(out[81:], np.broadcast_to(sky, out[81:].shape), atol=1e-6)
    np.testing.assert_allclose(out[:50], np.broadcast_to(sky, out[:50].shape), atol=1e-6)
    assert out[60:79].min() > 0.2  # and the curtain itself is lit


@pytest.mark.parametrize("falloff", [0.5, 1.0, 2.0])
def test_brightness_falls_off_upward_from_the_base_as_one_minus_height_to_the_falloff(falloff: float) -> None:
    """Without rays or waviness, row y of a curtain of base row 80 and height 50 rows (base 0.8, height 0.5 of 100 rows)
    has brightness (1 - (80 - y) / 50) ** falloff: 1 at the base, 0 at the curtain's top, worked out by hand."""
    grey = curtain({"base": 0.8, "height": 0.5, "falloff": falloff, "rays": 0.0})
    rows = np.arange(40, 80)  # the base's own row is half lit by anti-aliasing: rows above it are exact
    np.testing.assert_allclose(grey[rows, 7], (1 - (80 - rows) / 50) ** falloff, atol=1e-5)
    np.testing.assert_allclose(grey[rows], np.broadcast_to(grey[rows, :1], grey[rows].shape), atol=1e-6)  # no rays


def test_the_curtain_is_vertical_streaks_a_brightness_per_column_times_a_profile_down_the_rows() -> None:
    """With a flat base the picture is profile(row) x ray(column): a matrix of rank 1, whose second singular value is
    0; the rays differ from column to column."""
    grey = curtain({"base": 0.8, "height": 0.5, "rays": 1.0, "ray_width": 3.0, "seed": 2})
    lit = grey[40:79]
    singular = np.linalg.svd(lit, compute_uv=False)
    assert singular[1] / singular[0] < 1e-5
    assert lit[0].std() > 0.05 * lit[0].mean()


def test_without_rays_every_column_is_alike() -> None:
    grey = curtain({"rays": 0.0, "seed": 2})
    np.testing.assert_allclose(grey, np.broadcast_to(grey[:, :1], grey.shape), atol=1e-6)


@pytest.mark.parametrize(
    ("rays", "darkest_ray"),
    [(0.0, 1.0), (0.5, 0.5), (1.0, 0.0)],
    ids=["no rays", "half", "full rays go dark"],
)
def test_rays_is_how_dark_the_gaps_between_rays_are(rays: float, darkest_ray: float) -> None:
    """Each ray has a brightness 0..1 of its own; the curtain is brightness x (1 - rays x (1 - ray)), so the darkest
    column reaches 1 - rays of the brightest, and the brightest stays at full."""
    grey = curtain({"base": 0.8, "height": 0.5, "falloff": 1.0, "rays": rays, "ray_width": 2.0, "seed": 5}, (100, 400))
    row = grey[60]  # d = 0.4: profile 0.6
    assert row.max() == pytest.approx(0.6, abs=0.06)
    assert row.min() == pytest.approx(0.6 * darkest_ray, abs=0.08)


def test_a_larger_ray_width_gives_broader_rays() -> None:
    fine, medium, broad = (
        autocorrelation(curtain({"ray_width": width, "octaves": 1, "seed": 4}, (100, 300))[40:79], lag=3, axis=1)
        for width in (3.0, 12.0, 48.0)
    )
    assert fine < medium < broad


@pytest.mark.parametrize("waviness", [10.0, 20.0])
def test_waviness_is_the_spread_in_pixels_of_the_curtain_base_across_the_picture(waviness: float) -> None:
    """The base of each column (its lowest row at more than half brightness) wobbles with a standard deviation of
    `waviness` px about the `base` row (the noise has unit deviation)."""
    grey = curtain(
        {"base": 0.6, "height": 0.4, "rays": 0.0, "waviness": waviness, "scale": 20.0, "seed": 3}, (300, 600)
    )
    lowest = (grey > 0.5).cumsum(axis=0).argmax(axis=0)  # the last lit row of each column
    assert lowest.std() == pytest.approx(waviness, rel=0.3)
    assert lowest.mean() == pytest.approx(0.6 * 300, abs=0.5 * waviness)


def test_a_larger_scale_gives_a_slower_ripple_along_the_base() -> None:
    def base_row(scale: float) -> np.ndarray:
        grey = curtain(
            {"base": 0.6, "height": 0.4, "rays": 0.0, "waviness": 15.0, "scale": scale, "octaves": 1}, (300, 600)
        )
        return (grey > 0.5).cumsum(axis=0).argmax(axis=0).astype(float)

    fine, medium, broad = (np.corrcoef(base_row(scale)[:-4], base_row(scale)[4:])[0, 1] for scale in (6.0, 24.0, 96.0))
    assert fine < medium < broad


def test_the_curtain_reaches_height_up_from_its_base() -> None:
    """With falloff 1 the brightness is 1 - d, so at `height` 0.3 of 200 rows the curtain is lit more than 2 % for
    about 0.98 x 60 rows above a base at row 160."""
    grey = curtain({"base": 0.8, "height": 0.3, "falloff": 1.0, "rays": 0.0}, (200, 50))
    lit_rows = np.flatnonzero(grey[:, 0] > 0.02)
    assert lit_rows.min() == pytest.approx(160 - 0.98 * 60, abs=2)
    assert lit_rows.max() == pytest.approx(160, abs=1)


def test_colours_run_from_the_first_at_the_base_to_the_last_at_the_top() -> None:
    """Red at the base, blue at the top, black sky, no rays: at height d above the base the colour is the mix
    (1 - d) red + d blue at the curtain's brightness, so red : blue is (1 - d) : d."""
    out = registry.apply(
        NAME,
        np.zeros((100, 20, 3), np.float32),
        colors=["#ff0000", "#0000ff"],
        sky="#000000",
        base=0.8,
        height=0.5,
        falloff=1.0,
        rays=0.0,
        waviness=0.0,
    )
    for row, d in ((70, 0.2), (60, 0.4), (55, 0.5)):
        red, green, blue = out[row, 0]
        assert green == pytest.approx(0.0, abs=1e-6)
        assert blue / (red + blue) == pytest.approx(d, abs=0.01)
        assert red + blue == pytest.approx(1 - d, abs=0.01)


@pytest.mark.parametrize(
    "settings",
    [{"waviness": 20.0, "rays": 0.0}, {"waviness": 0.0, "rays": 1.0}],
    ids=["the folds of the base", "the rays"],
)
def test_the_same_seed_gives_the_same_picture_and_another_seed_another(settings: dict) -> None:
    first = curtain({**settings, "seed": 1})
    np.testing.assert_array_equal(first, curtain({**settings, "seed": 1}))
    assert np.abs(first - curtain({**settings, "seed": 2})).mean() > 0.01


@pytest.mark.parametrize(
    ("smoother", "rougher"),
    [
        pytest.param({"octaves": 1}, {"octaves": 4, "persistence": 0.8}, id="more octaves add fine detail"),
        pytest.param({"persistence": 0.1}, {"persistence": 0.9}, id="more persistence keeps the fine layers strong"),
        pytest.param({"lacunarity": 1.2}, {"lacunarity": 3.0}, id="more lacunarity makes the layers finer"),
    ],
)
def test_the_noise_layers_are_set_by_octaves_persistence_and_lacunarity(smoother: dict, rougher: dict) -> None:
    base = {"ray_width": 24.0, "octaves": 3, "persistence": 0.8, "lacunarity": 2.0, "rays": 1.0, "seed": 1}
    smooth, rough = (curtain({**base, **change}, (100, 400))[40:79] for change in (smoother, rougher))
    assert autocorrelation(rough, lag=1, axis=1) < autocorrelation(smooth, lag=1, axis=1) - 0.01
