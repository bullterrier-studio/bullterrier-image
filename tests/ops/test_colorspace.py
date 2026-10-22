import time

import numpy as np
import pytest

from photo_fx.ops import colorspace as cs

# Every public one-array conversion; the shape/dtype/input contracts apply to each of them.
CONVERSIONS = ["srgb_to_linear", "linear_to_srgb", "rgb_to_hsv", "hsv_to_rgb", "rgb_to_oklab", "oklab_to_rgb"]


def random_colours(n=10_000, seed=0):
    return np.random.default_rng(seed).random((n, 3), dtype=np.float32)


# --- shared contracts -------------------------------------------------------------------------------------------------


@pytest.mark.parametrize("name", CONVERSIONS)
@pytest.mark.parametrize("shape", [(3,), (5, 7, 3), (2, 5, 7, 3)])
def test_any_leading_shape_is_preserved_and_result_is_float32(name, shape):
    x = np.random.default_rng(1).random(shape, dtype=np.float32)
    out = getattr(cs, name)(x)
    assert out.shape == shape and out.dtype == np.float32


@pytest.mark.parametrize("name", CONVERSIONS)
def test_input_is_never_modified(name):
    x = random_colours(100)
    before = x.copy()
    getattr(cs, name)(x)
    np.testing.assert_array_equal(x, before)


@pytest.mark.parametrize("name", CONVERSIONS)
def test_float64_input_gives_float32_output(name):
    out = getattr(cs, name)(random_colours(10).astype(np.float64))
    assert out.dtype == np.float32


@pytest.mark.parametrize("name", CONVERSIONS)
@pytest.mark.parametrize("shape", [(), (4,), (5, 2), (5, 4), (3, 5)])
def test_last_axis_that_is_not_three_raises_a_clear_error(name, shape):
    with pytest.raises(ValueError, match=r"last axis.*length 3.*got shape"):
        getattr(cs, name)(np.zeros(shape, dtype=np.float32))


# --- sRGB transfer function -------------------------------------------------------------------------------------------


def test_srgb_half_is_linear_0_2140411():
    out = cs.srgb_to_linear(np.array([0.5, 0.5, 0.5], dtype=np.float32))
    np.testing.assert_allclose(out, 0.2140411, atol=1e-6)


def test_srgb_endpoints_are_fixed():
    np.testing.assert_allclose(cs.srgb_to_linear(np.array([0, 0, 0], dtype=np.float32)), 0, atol=1e-7)
    np.testing.assert_allclose(cs.srgb_to_linear(np.array([1, 1, 1], dtype=np.float32)), 1, atol=1e-6)
    np.testing.assert_allclose(cs.linear_to_srgb(np.array([1, 1, 1], dtype=np.float32)), 1, atol=1e-6)


def test_srgb_channels_are_converted_independently_and_in_place_of_their_channel():
    out = cs.srgb_to_linear(np.array([0.0, 0.5, 1.0], dtype=np.float32))
    np.testing.assert_allclose(out, [0.0, 0.2140411, 1.0], atol=1e-6)


def test_srgb_branches_meet_at_0_04045():
    # The two branches written out from IEC 61966-2-1, evaluated at the breakpoint.
    linear_branch = 0.04045 / 12.92
    power_branch = ((0.04045 + 0.055) / 1.055) ** 2.4
    assert linear_branch == pytest.approx(power_branch, abs=1e-6)
    eps = 1e-5
    below, above = (np.full(3, v, dtype=np.float32) for v in (0.04045 - eps, 0.04045 + eps))
    gap = cs.srgb_to_linear(above)[0] - cs.srgb_to_linear(below)[0]
    assert 0 < gap < 2 * eps  # continuous and no steeper than the slope (1/12.92) allows


def test_srgb_to_linear_is_strictly_increasing():
    ramp = np.repeat(np.linspace(0, 1, 10_001, dtype=np.float32)[:, None], 3, axis=1)
    assert np.all(np.diff(cs.srgb_to_linear(ramp)[:, 0]) > 0)
    assert np.all(np.diff(cs.linear_to_srgb(ramp)[:, 0]) > 0)


def test_linear_to_srgb_inverts_srgb_to_linear_on_random_colours():
    x = random_colours()
    np.testing.assert_allclose(cs.linear_to_srgb(cs.srgb_to_linear(x)), x, atol=1e-4)


def test_out_of_range_values_stay_finite_and_round_trip():
    # oklab_to_rgb is unclipped, so the transfer functions must tolerate what it hands on.
    ramp = np.repeat(np.linspace(-0.2, 1.5, 200, dtype=np.float32)[:, None], 3, axis=1)
    with np.errstate(all="raise"):
        linear = cs.srgb_to_linear(ramp)
        back = cs.linear_to_srgb(linear)
    assert np.isfinite(linear).all()
    np.testing.assert_allclose(back, ramp, atol=1e-4)


# --- HSV --------------------------------------------------------------------------------------------------------------


@pytest.mark.parametrize(
    ("rgb", "hsv"),
    [
        ((1, 0, 0), (0, 1, 1)),
        ((0, 1, 0), (1 / 3, 1, 1)),
        ((0, 0, 1), (2 / 3, 1, 1)),
        ((1, 1, 0), (1 / 6, 1, 1)),
        ((0, 1, 1), (1 / 2, 1, 1)),
        ((1, 0, 1), (5 / 6, 1, 1)),
        ((1, 1, 1), (0, 0, 1)),
        ((0, 0, 0), (0, 0, 0)),
        ((0.5, 0.25, 0.25), (0, 0.5, 0.5)),  # an unsaturated, darker red: distinct s and v
        ((0.2, 0.6, 0.4), (5 / 12, 2 / 3, 0.6)),  # all three channels differ: exposes swapped channels
    ],
)
def test_rgb_to_hsv_known_colours(rgb, hsv):
    out = cs.rgb_to_hsv(np.array(rgb, dtype=np.float32))
    np.testing.assert_allclose(out, hsv, atol=1e-6)


@pytest.mark.parametrize("grey", [0.0, 0.1, 0.5, 0.9, 1.0])
def test_greys_have_zero_saturation(grey):
    assert cs.rgb_to_hsv(np.full(3, grey, dtype=np.float32))[1] == 0


def test_hsv_to_rgb_known_colours():
    hsv = np.array([[0, 1, 1], [1 / 3, 1, 1], [2 / 3, 1, 1], [5 / 12, 2 / 3, 0.6]], dtype=np.float32)
    expected = [[1, 0, 0], [0, 1, 0], [0, 0, 1], [0.2, 0.6, 0.4]]
    np.testing.assert_allclose(cs.hsv_to_rgb(hsv), expected, atol=1e-6)


@pytest.mark.parametrize("hue", [0.0, 0.15, 0.4, 0.77, 0.99])
@pytest.mark.parametrize("shift", [1, -1, 3])
def test_hue_wraps_around_the_circle(hue, shift):
    base = cs.hsv_to_rgb(np.array([hue, 0.8, 0.7], dtype=np.float32))
    shifted = cs.hsv_to_rgb(np.array([hue + shift, 0.8, 0.7], dtype=np.float32))
    np.testing.assert_allclose(shifted, base, atol=1e-5)


def test_hsv_round_trip_on_random_colours():
    x = random_colours()
    np.testing.assert_allclose(cs.hsv_to_rgb(cs.rgb_to_hsv(x)), x, atol=1e-4)


def test_hsv_is_the_inverse_in_the_other_direction_too():
    hsv = random_colours(seed=3)
    hsv[:, 1:] = 0.2 + 0.8 * hsv[:, 1:]  # hue is undefined as s or v -> 0, so keep clear of that
    np.testing.assert_allclose(cs.rgb_to_hsv(cs.hsv_to_rgb(hsv)), hsv, atol=1e-4)


def test_hue_is_always_below_one_even_for_reds_with_a_hint_of_blue():
    reds = np.array([[1, 0, 1e-9], [1, 1e-9, 1e-8], [1, 0, 1e-6]], dtype=np.float32)
    hue = cs.rgb_to_hsv(reds)[:, 0]
    assert ((hue >= 0) & (hue < 1)).all()
    h = cs.rgb_to_hsv(random_colours())[:, 0]
    assert ((h >= 0) & (h < 1)).all()


# --- OKLab ------------------------------------------------------------------------------------------------------------


@pytest.mark.parametrize(
    ("rgb", "lab"),
    [
        ((1, 1, 1), (1, 0, 0)),
        ((0, 0, 0), (0, 0, 0)),
        # Primaries as published with Bjorn Ottosson's OKLab definition.
        ((1, 0, 0), (0.6280, 0.2249, 0.1258)),
        ((0, 1, 0), (0.8664, -0.2339, 0.1795)),
        ((0, 0, 1), (0.4520, -0.0324, -0.3115)),
    ],
)
def test_rgb_to_oklab_known_colours(rgb, lab):
    out = cs.rgb_to_oklab(np.array(rgb, dtype=np.float32))
    np.testing.assert_allclose(out, lab, atol=2e-3)


@pytest.mark.parametrize("grey", [0.05, 0.3, 0.5, 0.8, 0.95])
def test_greys_have_no_chroma(grey):
    lab = cs.rgb_to_oklab(np.full(3, grey, dtype=np.float32))
    np.testing.assert_allclose(lab[1:], 0, atol=1e-4)
    assert 0 < lab[0] < 1


def test_oklab_lightness_is_not_a_plain_channel_average():
    # Green is far lighter than blue to the eye: L must order them that way (catches swapped channels).
    green, blue = cs.rgb_to_oklab(np.array([[0, 1, 0], [0, 0, 1]], dtype=np.float32))[:, 0]
    assert green > blue + 0.3


def test_oklab_round_trip_on_random_colours():
    x = random_colours()
    np.testing.assert_allclose(cs.oklab_to_rgb(cs.rgb_to_oklab(x)), x, atol=1e-4)


def test_oklab_to_rgb_does_not_clip_out_of_gamut_colours():
    lab = np.array([0.5, 0.3, 0.3], dtype=np.float32)  # far more chromatic than any sRGB colour of this lightness
    rgb = cs.oklab_to_rgb(lab)
    assert np.isfinite(rgb).all()
    assert rgb.min() < 0 or rgb.max() > 1
    # Still the same colour: converting back recovers the OKLab coordinates.
    np.testing.assert_allclose(cs.rgb_to_oklab(rgb), lab, atol=1e-3)


def test_oklab_converts_a_full_size_image_quickly():
    # A loose regression guard against per-pixel Python loops (these run in a fraction of a second when vectorised).
    image = np.random.default_rng(2).random((1600, 1600, 3), dtype=np.float32)
    start = time.perf_counter()
    lab = cs.rgb_to_oklab(image)
    cs.oklab_to_rgb(lab)
    assert time.perf_counter() - start < 4
