"""Behaviour of the `noise_field` effect: a generated noise picture (value, perlin, worley or sine) through a ramp."""

import numpy as np
import pytest
from scipy import ndimage

from bullterrier_image.core import registry

NAME = "noise_field"
# A warp of 0.01 px shifts a point by at most about 0.04 px (its push is a standardised field, four deviations at
# most), and the ramp's grey level changes by under 0.05 per pixel at scale 60 (measured 0.0018 at most over the
# kinds), so a merely small warp moves no pixel by more than this; a jump on leaving 0 does.
TINY_WARP_MOVES_BY_AT_MOST = 0.005
KINDS = ["value", "perlin", "worley", "sine"]
LAYERED = ["value", "perlin", "sine"]  # the kinds built from octaves; worley is a single layer
GREY = ["#000000", "#ffffff"]


def field(settings: dict, shape: tuple[int, int] = (160, 160)) -> np.ndarray:
    """The noise as a grey picture (h, w): with a black-to-white ramp the grey level is the position on the ramp."""
    blank = np.zeros((*shape, 3), np.float32)
    return registry.apply(NAME, blank, colors=GREY, **settings)[..., 0].astype(np.float64)


def autocorrelation(values: np.ndarray, lag: int) -> float:
    return float(np.corrcoef(values[:, :-lag].ravel(), values[:, lag:].ravel())[0, 1])


@pytest.mark.parametrize("kind", KINDS)
def test_a_two_colour_ramp_mixes_the_colours_and_uses_all_of_it(kind: str) -> None:
    """Red-to-blue: every pixel is red x (1 - t) + blue x t, so red + blue = 1 and green = 0; t covers the ramp."""
    blank = np.zeros((128, 128, 3), np.float32)
    out = registry.apply(NAME, blank, kind=kind, colors=["#ff0000", "#0000ff"], seed=2)
    np.testing.assert_allclose(out[..., 0] + out[..., 2], 1.0, atol=1e-5)
    np.testing.assert_allclose(out[..., 1], 0.0, atol=1e-6)
    assert out[..., 2].min() < 0.1
    assert out[..., 2].max() > 0.9


def test_more_than_two_colours_are_evenly_spaced_along_the_ramp() -> None:
    """Three colours: t < 0.5 mixes the first two, t > 0.5 the last two, so green is never above its stop's 1."""
    blank = np.zeros((128, 128, 3), np.float32)
    out = registry.apply(NAME, blank, kind="value", colors=["#ff0000", "#00ff00", "#0000ff"], seed=2)
    np.testing.assert_allclose(out.sum(axis=-1), 1.0, atol=1e-5)  # always a mix of neighbouring stops: sums to 1
    assert out[..., 1].max() > 0.9  # t passes the middle stop
    assert np.minimum(out[..., 0], out[..., 2]).max() < 1e-5  # red and blue never mix directly


@pytest.mark.parametrize("kind", KINDS)
def test_the_same_seed_gives_the_same_picture_and_another_seed_another(kind: str) -> None:
    first = field({"kind": kind, "seed": 1})
    np.testing.assert_array_equal(first, field({"kind": kind, "seed": 1}))
    assert np.abs(first - field({"kind": kind, "seed": 2})).mean() > 0.05


@pytest.mark.parametrize("kind", KINDS)
def test_a_larger_scale_gives_broader_features(kind: str) -> None:
    fine, medium, coarse = (
        autocorrelation(field({"kind": kind, "scale": scale, "octaves": 1}, shape=(192, 192)), lag=4)
        for scale in (4.0, 16.0, 64.0)
    )
    assert fine < medium < coarse


@pytest.mark.parametrize("kind", LAYERED)
@pytest.mark.parametrize(
    ("smoother", "rougher"),
    [
        pytest.param({"octaves": 1}, {"octaves": 4, "persistence": 0.8}, id="more octaves add fine detail"),
        pytest.param({"persistence": 0.1}, {"persistence": 0.9}, id="more persistence keeps the fine layers strong"),
        pytest.param({"lacunarity": 1.2}, {"lacunarity": 3.0}, id="more lacunarity makes the layers finer"),
    ],
)
def test_the_layers_are_set_by_octaves_persistence_and_lacunarity(kind: str, smoother: dict, rougher: dict) -> None:
    base = {"kind": kind, "scale": 16.0, "octaves": 3, "persistence": 0.8, "lacunarity": 2.0, "seed": 1}
    smooth, rough = (field({**base, **change}, shape=(192, 192)) for change in (smoother, rougher))
    assert autocorrelation(rough, lag=1) < autocorrelation(smooth, lag=1) - 0.01


def test_worley_is_one_layer_whatever_the_octave_settings() -> None:
    single = field({"kind": "worley", "octaves": 1, "persistence": 0.1, "lacunarity": 1.0})
    np.testing.assert_array_equal(
        field({"kind": "worley", "octaves": 5, "persistence": 0.9, "lacunarity": 3.0}), single
    )


def test_perlin_is_zero_at_every_lattice_point() -> None:
    """Gradient noise vanishes at the corners of its cells (a point's own gradient dotted with a zero offset): with one
    octave and a scale of 16 px those are the pixels at multiples of 16, all at the ramp's middle value 0.5, while
    elsewhere the picture strays far from it."""
    grey = field({"kind": "perlin", "scale": 16.0, "octaves": 1, "seed": 5}, shape=(129, 129))
    corners = grey[::16, ::16]
    others = np.ones(grey.shape, bool)
    others[::16, ::16] = False
    np.testing.assert_allclose(corners, 0.5, atol=0.03)
    assert np.abs(grey[others] - 0.5).mean() > 0.15


def test_worley_has_one_feature_point_per_cell() -> None:
    """Distance to the nearest feature point is 0 at the point and grows away from it, so the picture's local minima
    are the feature points: one in each 20 px cell. Away from the border (where the nearest point may lie outside the
    picture) the 180 x 180 px middle of 200 px holds 81 cells' worth, all equally dark (distance ~0)."""
    grey = field({"kind": "worley", "scale": 20.0, "seed": 3}, shape=(200, 200))
    minima = (grey == ndimage.minimum_filter(grey, size=3))[10:-10, 10:-10]
    assert minima.sum() == pytest.approx(81, abs=10)
    assert grey[10:-10, 10:-10][minima].max() - grey.min() < 0.1


@pytest.mark.parametrize("kind", ["value", "perlin"])
def test_layers_finer_than_a_pixel_are_left_out(kind: str) -> None:
    """Lattice spacings 3, 1.5, 0.75, ...: from the third layer on they are finer than the pixel grid: no effect."""
    two = field({"kind": kind, "scale": 3.0, "octaves": 2})
    np.testing.assert_array_equal(field({"kind": kind, "scale": 3.0, "octaves": 8}), two)


def second_difference(grey: np.ndarray, columns: slice) -> float:
    """Mean |f(x - 1) - 2 f(x) + f(x + 1)| along the rows, at the given columns: the kink in the picture there."""
    middle = grey[:, 1:-1]
    return float(np.abs(grey[:, :-2] - 2 * middle + grey[:, 2:])[:, columns].mean())


def test_perlin_bends_smoothly_across_the_lattice_lines() -> None:
    """With the fade curve the slope is continuous across a lattice line, so the picture shows no more kink there than
    between lines (a straight blend between cells shows a crease on every lattice line, several times the kink)."""
    grey = field({"kind": "perlin", "scale": 16.0, "octaves": 1, "seed": 5}, shape=(160, 161))
    on_lines = second_difference(grey, slice(15, None, 16))  # column 16 k, offset by the trimmed first column
    between = second_difference(grey, slice(7, None, 16))
    assert on_lines < 1.6 * between


def test_worley_has_feature_points_just_beyond_the_border_too() -> None:
    """The nearest point to a pixel at the edge can lie outside the picture, so over many seeds the top and left 3 px
    bands are as dark as the middle on average (without points beyond the edge they are about 0.2 brighter)."""
    excess = []
    for seed in range(10):
        grey = field({"kind": "worley", "scale": 20.0, "seed": seed}, shape=(200, 200))
        middle = grey[40:-40, 40:-40].mean()
        excess += [grey[:3, 40:-40].mean() - middle, grey[40:-40, :3].mean() - middle]
    assert np.mean(excess) == pytest.approx(0.0, abs=0.08)


def strongest_wavelength(values: np.ndarray) -> float:
    """The period in px of the strongest wave of a square picture, from the peak of its 2-D spectrum (not the mean)."""
    spectrum = np.abs(np.fft.fftshift(np.fft.fft2(values - values.mean())))
    centre = len(values) // 2
    peak_row, peak_col = np.unravel_index(np.argmax(spectrum), spectrum.shape)
    return len(values) / float(np.hypot(peak_row - centre, peak_col - centre))


@pytest.mark.parametrize("scale", [16.0, 32.0, 64.0])
def test_sine_scale_is_the_wavelength_of_the_coarsest_waves(scale: float) -> None:
    """One layer is two plane waves of wavelength `scale`: the 2-D spectrum peaks at radius 256 / scale cycles."""
    grey = field({"kind": "sine", "scale": scale, "octaves": 1, "seed": 3}, shape=(256, 256))
    assert strongest_wavelength(grey) == pytest.approx(scale, rel=0.1)


def test_sine_layers_finer_than_two_pixels_are_left_out() -> None:
    """Wavelengths 5, 2.5, 1.25, ...: from the third layer on a wave has fewer than 2 px a period, which a pixel grid
    cannot show, so they add nothing."""
    two = field({"kind": "sine", "scale": 5.0, "octaves": 2, "seed": 2})
    np.testing.assert_array_equal(field({"kind": "sine", "scale": 5.0, "octaves": 8, "seed": 2}), two)


def test_sine_zero_persistence_leaves_only_the_first_layer() -> None:
    only_first = field({"kind": "sine", "octaves": 1, "seed": 6})
    np.testing.assert_allclose(
        field({"kind": "sine", "octaves": 4, "persistence": 0.0, "seed": 6}), only_first, atol=1e-5
    )


def test_sine_warp_needs_a_layer_before_to_push_by() -> None:
    """Each sine layer is pushed along by the layer before it, so with one layer there is nothing to push by."""
    one = {"kind": "sine", "octaves": 1, "seed": 4}
    np.testing.assert_array_equal(field({**one, "warp": 0.0}), field({**one, "warp": 100.0}))


@pytest.mark.parametrize("kind", KINDS)
@pytest.mark.parametrize(
    ("warp", "kept_at_least", "kept_at_most"),
    [(0.5, 0.99, 1.0), (60.0, -1.0, 0.9)],
    ids=["half a pixel barely moves it", "60 px visibly changes it"],
)
def test_warp_pushes_the_pattern_about_in_every_kind(
    kind: str, warp: float, kept_at_least: float, kept_at_most: float
) -> None:
    base = {"kind": kind, "scale": 60.0, "octaves": 3, "seed": 4}
    kept = np.corrcoef(field({**base, "warp": 0.0}).ravel(), field({**base, "warp": warp}).ravel())[0, 1]
    assert kept_at_least <= kept <= kept_at_most


@pytest.mark.parametrize("kind", ["value", "perlin", "worley"])
def test_warp_keeps_every_part_of_the_ramp_in_use(kind: str) -> None:
    """Pushing a noise about moves its values, it does not squash them: the ramp is still used end to end."""
    grey = field({"kind": kind, "scale": 40.0, "warp": 30.0, "seed": 2}, shape=(256, 256))
    assert grey.min() < 0.1
    assert grey.max() > 0.9


@pytest.mark.parametrize("kind", KINDS)
def test_a_tiny_warp_gives_nearly_the_unwarped_picture(kind: str) -> None:
    """Raising `warp` from 0 must not jump, so that animating it up from 0 starts smoothly."""
    base = {"kind": kind, "scale": 60.0, "octaves": 3, "seed": 4}
    moved = np.abs(field({**base, "warp": 0.0}) - field({**base, "warp": 0.01}))
    assert moved.max() <= TINY_WARP_MOVES_BY_AT_MOST
