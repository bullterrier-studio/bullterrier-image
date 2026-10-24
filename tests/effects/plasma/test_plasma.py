"""Behaviour of the `plasma` effect: a warped sum of sine waves, coloured through a (by default cyclic) ramp."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "plasma"
GREY = ["#000000", "#ffffff"]


def plasma(settings: dict, shape: tuple[int, int] = (192, 192)) -> np.ndarray:
    """The plasma as a grey picture (h, w): with a black-to-white ramp the grey level is the position on the ramp."""
    blank = np.zeros((*shape, 3), np.float32)
    return registry.apply(NAME, blank, colors=GREY, **settings)[..., 0].astype(np.float64)


def autocorrelation(values: np.ndarray, lag: int) -> float:
    return float(np.corrcoef(values[:, :-lag].ravel(), values[:, lag:].ravel())[0, 1])


def strongest_wavelength(values: np.ndarray) -> float:
    """The period in px of the strongest wave of a square picture, from the peak of its 2-D spectrum (not the mean)."""
    spectrum = np.abs(np.fft.fftshift(np.fft.fft2(values - values.mean())))
    centre = len(values) // 2
    peak_row, peak_col = np.unravel_index(np.argmax(spectrum), spectrum.shape)
    return len(values) / float(np.hypot(peak_row - centre, peak_col - centre))


@pytest.mark.parametrize("size", [(40, 64), (97, 33)], ids=["wide", "tall"])
def test_the_picture_takes_only_its_size_from_the_input(size: tuple[int, int]) -> None:
    dark, light = np.zeros((*size, 3), np.float32), np.ones((*size, 3), np.float32)
    out = registry.apply(NAME, dark, seed=3)
    assert out.shape == (*size, 3)
    np.testing.assert_array_equal(out, registry.apply(NAME, light, seed=3))


def test_a_two_colour_ramp_mixes_the_colours_and_uses_all_of_it() -> None:
    """Red-to-blue: every pixel is red x (1 - t) + blue x t, so red + blue = 1 and green = 0; t covers the ramp."""
    out = registry.apply(NAME, np.zeros((128, 128, 3), np.float32), colors=["#ff0000", "#0000ff"], seed=2)
    np.testing.assert_allclose(out[..., 0] + out[..., 2], 1.0, atol=1e-5)
    np.testing.assert_allclose(out[..., 1], 0.0, atol=1e-6)
    assert out[..., 2].min() < 0.1
    assert out[..., 2].max() > 0.9


def test_every_part_of_the_ramp_is_used_about_equally() -> None:
    """A pixel's place on the ramp is its rank among all pixels, so each quarter of a black-to-white ramp holds about a
    quarter of the picture (not, say, half of it at the dark end)."""
    grey = plasma({"scale": 24.0, "seed": 2}, shape=(256, 256))
    shares = np.histogram(grey, bins=4, range=(0.0, 1.0))[0] / grey.size
    np.testing.assert_allclose(shares, 0.25, atol=0.08)


def test_the_same_seed_gives_the_same_picture_and_another_seed_another() -> None:
    first = plasma({"seed": 1})
    np.testing.assert_array_equal(first, plasma({"seed": 1}))
    assert np.abs(first - plasma({"seed": 2})).mean() > 0.05


@pytest.mark.parametrize("scale", [16.0, 32.0, 64.0])
def test_scale_is_the_wavelength_of_the_coarsest_waves(scale: float) -> None:
    """One layer is two plane waves of wavelength `scale`: the 2-D spectrum peaks at radius 256 / scale cycles."""
    peak = strongest_wavelength(plasma({"scale": scale, "octaves": 1, "warp": 0.0, "seed": 3}, shape=(256, 256)))
    assert peak == pytest.approx(scale, rel=0.1)


def test_the_first_layer_has_nothing_to_be_warped_by() -> None:
    """Each layer is pushed along by the layer before it, so with one layer `warp` changes nothing."""
    np.testing.assert_array_equal(
        plasma({"octaves": 1, "warp": 0.0, "seed": 4}), plasma({"octaves": 1, "warp": 100.0, "seed": 4})
    )


@pytest.mark.parametrize(
    ("warp", "kept_at_least", "kept_at_most"),
    [(0.5, 0.99, 1.0), (60.0, -1.0, 0.9)],
    ids=["half a pixel barely moves it", "60 px visibly changes it"],
)
def test_warp_pushes_each_layer_by_the_layers_before_it(warp: float, kept_at_least: float, kept_at_most: float) -> None:
    base = {"scale": 60.0, "octaves": 3, "seed": 4}
    kept = np.corrcoef(plasma({**base, "warp": 0.0}).ravel(), plasma({**base, "warp": warp}).ravel())[0, 1]
    assert kept_at_least <= kept <= kept_at_most


def test_zero_persistence_leaves_only_the_first_layer() -> None:
    only_first = plasma({"octaves": 1, "seed": 6})
    np.testing.assert_allclose(plasma({"octaves": 4, "persistence": 0.0, "seed": 6}), only_first, atol=1e-5)


def test_layers_finer_than_two_pixels_are_left_out() -> None:
    """Wavelengths 5, 2.5, 1.25, ...: from the third layer on a wave has fewer than 2 px a period, which a pixel grid
    cannot show, so they add nothing."""
    two = plasma({"scale": 5.0, "octaves": 2, "seed": 2})
    np.testing.assert_array_equal(plasma({"scale": 5.0, "octaves": 8, "seed": 2}), two)


@pytest.mark.parametrize(
    ("smoother", "rougher"),
    [
        pytest.param({"octaves": 1}, {"octaves": 4, "persistence": 0.8}, id="more octaves add fine detail"),
        pytest.param({"persistence": 0.1}, {"persistence": 0.9}, id="more persistence keeps the fine layers strong"),
        pytest.param({"lacunarity": 1.2}, {"lacunarity": 3.0}, id="more lacunarity makes the layers finer"),
    ],
)
def test_the_layers_are_set_by_octaves_persistence_and_lacunarity(smoother: dict, rougher: dict) -> None:
    base = {"scale": 48.0, "octaves": 3, "persistence": 0.8, "lacunarity": 2.0, "warp": 0.0, "seed": 1}
    smooth, rough = (plasma({**base, **change}) for change in (smoother, rougher))
    assert autocorrelation(rough, lag=1) < autocorrelation(smooth, lag=1) - 0.01
