"""Behaviour of the `fog` effect: the picture mixed toward a colour by a cloudy, bottom-heavy density map."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "fog"


def haze(settings: dict, shape: tuple[int, int] = (128, 128)) -> np.ndarray:
    """The fog's opacity map (h, w): white fog over a black picture shows exactly its opacity."""
    black = np.zeros((*shape, 3), np.float32)
    return registry.apply(NAME, black, color="#ffffff", **settings)[..., 0].astype(np.float64)


def autocorrelation(field: np.ndarray, lag: int) -> float:
    return float(np.corrcoef(field[:, :-lag].ravel(), field[:, lag:].ravel())[0, 1])


def test_density_zero_leaves_the_picture_unchanged(photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, density=0.0)
    np.testing.assert_array_equal(out, photo)
    assert not np.shares_memory(out, photo)


@pytest.mark.parametrize("density", [0.25, 0.6, 1.0])
def test_even_fog_mixes_every_pixel_toward_the_colour_by_the_density(density: float, photo: np.ndarray) -> None:
    """With no gradient and no patchiness the opacity is `density` everywhere: out = picture + density x (fog -
    picture), which also shrinks the picture's contrast by 1 - density."""
    fog = np.array([0.8, 0.5, 0.2], np.float32)
    out = registry.apply(NAME, photo, density=density, color=fog.tolist(), gradient=0.0, patchiness=0.0)
    np.testing.assert_allclose(out, photo + density * (fog - photo), atol=1e-5)
    assert out.std(axis=(0, 1)) == pytest.approx((1 - density) * photo.std(axis=(0, 1)), abs=1e-3)


@pytest.mark.parametrize(
    ("gradient", "opacity_by_row"),
    [
        pytest.param(0.0, [0.5, 0.5, 0.5, 0.5], id="gradient 0 is even"),
        pytest.param(0.5, [0.28125, 0.34375, 0.40625, 0.46875], id="gradient 0.5 halves the range"),
        pytest.param(1.0, [0.0625, 0.1875, 0.3125, 0.4375], id="gradient 1 fades to nothing at the top"),
    ],
)
def test_the_gradient_thickens_the_fog_toward_the_bottom(gradient: float, opacity_by_row: list[float]) -> None:
    """Row centres of a 4-row picture sit at t = 1/8, 3/8, 5/8, 7/8 of the way down; with density 0.5 the opacity is
    0.5 x (1 - gradient x (1 - t)), worked out by hand for each row in the table."""
    out = haze({"density": 0.5, "gradient": gradient, "patchiness": 0.0}, shape=(4, 3))
    np.testing.assert_allclose(out, np.repeat(opacity_by_row, 3).reshape(4, 3), atol=1e-6)


def test_with_a_gradient_the_fog_is_thicker_in_the_lower_rows_on_average() -> None:
    opacity = haze({"density": 0.8, "gradient": 1.0, "patchiness": 0.5, "seed": 3})
    rows = opacity.mean(axis=1)
    assert rows[-32:].mean() > 2 * rows[:32].mean()


@pytest.mark.parametrize("patchiness", [0.0, 0.5, 1.0])
def test_fog_is_never_denser_than_density_and_gaps_open_with_patchiness(patchiness: float) -> None:
    opacity = haze({"density": 0.6, "gradient": 0.0, "patchiness": patchiness, "seed": 2})
    assert opacity.max() <= 0.6 + 1e-6
    assert opacity.min() >= 0.6 * (1 - patchiness) - 1e-6  # the gaps never open wider than patchiness allows
    assert opacity.std() >= 0.05 * patchiness  # and the more patchiness, the more the opacity varies


@pytest.mark.parametrize("value", [0.0, 1.0], ids=["black picture", "white picture"])
def test_full_even_fog_hides_the_picture_behind_its_colour(value: float) -> None:
    fog = [0.2, 0.4, 0.6]
    picture = np.full((32, 32, 3), value, np.float32)
    out = registry.apply(NAME, picture, density=1.0, color=fog, gradient=0.0, patchiness=0.0)
    np.testing.assert_allclose(out, np.broadcast_to(fog, (32, 32, 3)), atol=1e-6)


def test_the_same_seed_gives_the_same_fog_and_another_seed_another() -> None:
    first = haze({"seed": 1})
    np.testing.assert_array_equal(first, haze({"seed": 1}))
    assert np.abs(first - haze({"seed": 2})).mean() > 0.02


def test_a_larger_scale_gives_broader_clouds() -> None:
    fine, medium, coarse = (
        autocorrelation(haze({"scale": scale, "octaves": 1, "gradient": 0.0}, shape=(192, 192)), lag=4)
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
    base = {"scale": 24.0, "octaves": 3, "persistence": 0.8, "lacunarity": 2.0, "gradient": 0.0, "seed": 1}
    smooth = haze({**base, **smoother}, shape=(192, 192))
    rough = haze({**base, **rougher}, shape=(192, 192))
    assert autocorrelation(rough, lag=1) < autocorrelation(smooth, lag=1) - 0.01
