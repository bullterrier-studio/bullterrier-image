"""Behaviour of the `white_balance` effect."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "white_balance"


# The sRGB transfer function (IEC 61966-2-1), written out here so the expectations do not use the library's code.
def decode(value: float) -> float:
    return value / 12.92 if value <= 0.04045 else ((value + 0.055) / 1.055) ** 2.4


def encode(value: float) -> float:
    return value * 12.92 if value <= 0.0031308 else 1.055 * value ** (1 / 2.4) - 0.055


def balanced(pixel: tuple[float, float, float], red: float, green: float, blue: float) -> list[float]:
    """The pixel after scaling its linear-light channels by the given gains (hand-worked von Kries)."""
    return [encode(decode(value) * gain) for value, gain in zip(pixel, (red, green, blue), strict=True)]


def grey_pixel(value: float) -> np.ndarray:
    return np.full((1, 1, 3), value, np.float32)


@pytest.mark.parametrize(
    ("settings", "gains"),
    [
        # Largest gain is scaled to 1 so nothing brightens: temperature 1 stop = red x 2**0.5, blue x 2**-0.5.
        pytest.param({"temperature": 1.0}, (1.0, 2**-0.5, 0.5), id="warmer: red up and blue down by a stop together"),
        pytest.param({"temperature": -1.0}, (0.5, 2**-0.5, 1.0), id="cooler: blue up and red down"),
        pytest.param({"temperature": 2.0}, (1.0, 0.5, 0.25), id="two stops warmer"),
        pytest.param({"tint": 1.0}, (1.0, 0.5, 1.0), id="magenta tint takes a stop out of green"),
        pytest.param({"tint": -1.0}, (0.5, 1.0, 0.5), id="green tint takes a stop out of red and blue"),
        pytest.param({"temperature": 1.0, "tint": 1.0}, (1.0, 0.5 * 2**-0.5, 0.5), id="temperature and tint combine"),
        pytest.param({}, (1.0, 1.0, 1.0), id="the defaults change nothing"),
    ],
)
def test_the_channels_are_scaled_in_linear_light_by_the_documented_gains(
    settings: dict[str, float], gains: tuple[float, float, float]
) -> None:
    pixel = (0.5, 0.5, 0.5)
    out = registry.apply(NAME, grey_pixel(0.5), **settings)
    np.testing.assert_allclose(out[0, 0], balanced(pixel, *gains), atol=1e-6)


def test_gains_act_on_each_channel_of_a_coloured_pixel_in_linear_light() -> None:
    pixel = (0.8, 0.4, 0.2)
    out = registry.apply(NAME, np.array([[pixel]], np.float32), temperature=1.0)
    np.testing.assert_allclose(out[0, 0], balanced(pixel, 1.0, 2**-0.5, 0.5), atol=1e-6)


def test_the_defaults_leave_a_photo_unchanged(photo: np.ndarray) -> None:
    np.testing.assert_allclose(registry.apply(NAME, photo), photo, atol=1e-6)


@pytest.mark.parametrize(
    ("settings", "red_over_blue"),
    [
        pytest.param({"temperature": 0.5}, 1, id="positive temperature warms a grey"),
        pytest.param({"temperature": -0.5}, -1, id="negative temperature cools a grey"),
        pytest.param({"temperature": 0.0}, 0, id="zero temperature leaves a grey neutral"),
    ],
)
def test_temperature_moves_a_grey_along_the_red_blue_axis_and_never_off_green(
    settings: dict[str, float], red_over_blue: int
) -> None:
    out = registry.apply(NAME, grey_pixel(0.6), **settings)[0, 0]
    assert np.sign(round(float(out[0] - out[2]), 5)) == red_over_blue
    assert out[0] >= out[1] >= out[2] if red_over_blue > 0 else True
    assert out[0] <= out[1] <= out[2] if red_over_blue < 0 else True


@pytest.mark.parametrize(
    ("tint", "green_against_red_and_blue"),
    [(0.5, -1), (-0.5, 1), (0.0, 0)],
    ids=["positive tint is magenta: green falls", "negative tint is green: green leads", "zero tint is neutral"],
)
def test_tint_moves_a_grey_along_the_green_magenta_axis(tint: float, green_against_red_and_blue: int) -> None:
    out = registry.apply(NAME, grey_pixel(0.6), tint=tint)[0, 0]
    assert np.sign(round(float(out[1] - out[0]), 5)) == green_against_red_and_blue
    assert out[0] == pytest.approx(out[2], abs=1e-6)


@pytest.mark.parametrize("settings", [{"temperature": 2.0}, {"temperature": -2.0}, {"tint": 1.0}, {"tint": -1.0}])
def test_no_channel_is_ever_brighter_than_in_the_input_and_black_stays_black(
    settings: dict[str, float], photo: np.ndarray
) -> None:
    out = registry.apply(NAME, photo, **settings)
    assert (out <= photo + 1e-6).all()
    np.testing.assert_array_equal(registry.apply(NAME, np.zeros((2, 2, 3), np.float32), **settings), 0.0)


def test_white_keeps_its_strongest_channel_so_it_never_clips_into_a_cast() -> None:
    white = np.ones((1, 1, 3), np.float32)
    np.testing.assert_allclose(registry.apply(NAME, white, temperature=1.0)[0, 0, 0], 1.0, atol=1e-6)
    np.testing.assert_allclose(registry.apply(NAME, white, temperature=-1.0)[0, 0, 2], 1.0, atol=1e-6)
    np.testing.assert_allclose(
        registry.apply(NAME, white, tint=1.0)[0, 0, 1], 0.5 ** (1 / 2.4) * 1.055 - 0.055, atol=1e-6
    )


@pytest.mark.parametrize(
    ("settings", "match"),
    [
        pytest.param({"temperature": 2.5}, "temperature", id="temperature above the maximum"),
        pytest.param({"temperature": -2.5}, "temperature", id="temperature below the minimum"),
        pytest.param({"tint": 1.5}, "tint", id="tint above the maximum"),
        pytest.param({"kelvin": 5000}, "kelvin", id="there is no kelvin setting"),
    ],
)
def test_a_setting_that_is_out_of_range_or_unknown_is_refused(settings: dict[str, float], match: str, photo) -> None:
    with pytest.raises(ValueError, match=match):
        registry.apply(NAME, photo, **settings)
