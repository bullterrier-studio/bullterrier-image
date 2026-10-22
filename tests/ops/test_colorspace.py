import numpy as np
import pytest

from photo_fx.ops import colorspace as cs

# Every public one-array conversion; the shape/dtype/input contracts apply to each of them.
CONVERSIONS = ["srgb_to_linear", "linear_to_srgb"]


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
