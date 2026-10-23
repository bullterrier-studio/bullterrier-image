"""Behaviour of the `selective_color` effect."""

import colorsys

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "selective_color"


def pixel(hue_degrees: float, saturation: float = 1.0, value: float = 1.0) -> np.ndarray:
    """A 1x1 picture of the colour with this hue (0 red, 120 green, 240 blue), made with the standard library."""
    return np.array([[colorsys.hsv_to_rgb(hue_degrees / 360, saturation, value)]], np.float32)


def hue_of(picture: np.ndarray) -> float:
    """Hue in degrees of a 1x1 picture, by the standard library."""
    return colorsys.rgb_to_hsv(*map(float, picture[0, 0]))[0] * 360


def circular_distance(a: float, b: float) -> float:
    return abs((a - b + 180) % 360 - 180)


@pytest.mark.parametrize(
    ("hue", "settings", "expected_hue"),
    [
        pytest.param(0, {"hue_shift": 120}, 120, id="red in the range, shifted 120 degrees, is green"),
        pytest.param(0, {"hue_shift": -60}, 300, id="a negative shift wraps round to magenta"),
        pytest.param(0, {"hue_shift": 0}, 0, id="no shift keeps the hue"),
        pytest.param(15, {"hue_shift": 45}, 60, id="a hue 15 degrees off the centre is still fully inside"),
        pytest.param(30, {"hue_shift": 60}, 60, id="half way down the feather the shift is halved: 30 + 30"),
        pytest.param(350, {"hue_shift": 30}, 20, id="the range wraps round red: 350 is 10 degrees below the centre"),
        pytest.param(120, {"hue_shift": 90}, 120, id="outside the feather nothing moves"),
    ],
)
def test_hue_shift_moves_hues_by_the_weight_of_their_place_in_the_range(
    hue: float, settings: dict[str, float], expected_hue: float
) -> None:
    # Range: centre 0, width 40 (fully inside up to 20 degrees either side), feather 20 (zero from 40 degrees on).
    out = registry.apply(NAME, pixel(hue), hue=0, width=40, feather=20, **settings)
    assert circular_distance(hue_of(out), expected_hue) < 0.05


@pytest.mark.parametrize(
    ("colour", "settings", "expected"),
    [
        # Fully saturated colours (saturation 1) are selected at the full weight of their hue.
        pytest.param((1, 0, 0), {"saturation": 0.0}, (1, 1, 1), id="saturation 0 turns red grey, keeping its value"),
        pytest.param((1, 0, 0), {"saturation": 0.5}, (1, 0.5, 0.5), id="saturation 0.5 halves the saturation"),
        pytest.param((1, 0, 0), {"saturation": 3.0}, (1, 0, 0), id="saturation cannot go past full"),
        pytest.param((1, 0, 0), {"lightness": -0.5}, (0.5, 0, 0), id="lightness -0.5 halves the value"),
        pytest.param((0.5, 0, 0), {"lightness": 0.5}, (0.75, 0, 0), id="lightness +0.5 goes half way to white"),
        pytest.param((1, 0, 0), {"lightness": -1.0}, (0, 0, 0), id="lightness -1 is black"),
        pytest.param((0.5, 0, 0), {"lightness": 1.0}, (1, 0, 0), id="lightness +1 is the full value"),
        # Half saturated: the selection weight is 0.5, so the change is half as strong. HSV (0, 0.5, 0.5).
        pytest.param((0.5, 0.25, 0.25), {"lightness": -0.5}, (0.375, 0.1875, 0.1875), id="a pale colour changes less"),
        pytest.param((0.5, 0.25, 0.25), {"saturation": 0.0}, (0.5, 0.375, 0.375), id="saturation 0 at weight 0.5"),
    ],
)
def test_saturation_and_lightness_change_the_selected_colours(
    colour: tuple[float, float, float], settings: dict[str, float], expected: tuple[float, float, float]
) -> None:
    out = registry.apply(NAME, np.array([[colour]], np.float32), hue=0, width=360, feather=0, **settings)
    np.testing.assert_allclose(out[0, 0], expected, atol=1e-6)


def reference(rgb: np.ndarray, *, hue: float, width: float, feather: float, hue_shift: float, saturation: float,
              lightness: float) -> np.ndarray:  # fmt: skip
    """The effect worked out pixel by pixel with the standard library's HSV conversion."""
    out = rgb.copy()
    for y in range(rgb.shape[0]):
        for x in range(rgb.shape[1]):
            h, s, v = colorsys.rgb_to_hsv(*map(float, rgb[y, x]))
            distance = circular_distance(h * 360, hue)
            if distance <= width / 2:
                weight = 1.0
            elif feather > 0:
                weight = max(0.0, 1 - (distance - width / 2) / feather)
            else:
                weight = 0.0
            weight *= s
            if weight == 0:
                continue
            s_new = min(1.0, s * (1 + weight * (saturation - 1)))
            v_new = v + weight * (lightness * (1 - v) if lightness > 0 else lightness * v)
            out[y, x] = colorsys.hsv_to_rgb(h + weight * hue_shift / 360, s_new, v_new)
    return out


@pytest.mark.parametrize(
    "settings",
    [
        pytest.param(
            {"hue": 0, "width": 60, "feather": 30, "hue_shift": 40, "saturation": 1.4, "lightness": 0.2}, id="reds"
        ),
        pytest.param(
            {"hue": 120, "width": 90, "feather": 60, "hue_shift": -50, "saturation": 0.3, "lightness": -0.4},
            id="greens",
        ),
        pytest.param(
            {"hue": 350, "width": 20, "feather": 0, "hue_shift": 90, "saturation": 2.0, "lightness": 0.0},
            id="hard edge",
        ),
        pytest.param(
            {"hue": 200, "width": 360, "feather": 0, "hue_shift": 15, "saturation": 0.8, "lightness": 0.1},
            id="all hues",
        ),
    ],
)
def test_a_photo_agrees_with_a_per_pixel_reference(settings: dict[str, float], photo: np.ndarray) -> None:
    small = photo[:12, :16]
    np.testing.assert_allclose(registry.apply(NAME, small, **settings), reference(small, **settings), atol=2e-5)


def test_pixels_outside_the_range_and_its_feather_are_returned_bit_for_bit(photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, hue=0, width=40, feather=20, hue_shift=90, saturation=0.2, lightness=-0.5)
    hues = np.array([colorsys.rgb_to_hsv(*map(float, p))[0] * 360 for p in photo.reshape(-1, 3)]).reshape(
        photo.shape[:2]
    )
    distance = np.abs((hues - 0 + 180) % 360 - 180)
    outside = distance > 40 + 1e-3
    assert outside.sum() > 1000  # the photo has plenty of hues outside the range, so the property is exercised
    np.testing.assert_array_equal(out[outside], photo[outside])
    assert (out[distance < 20 - 1e-3] != photo[distance < 20 - 1e-3]).any()  # and some inside did change


@pytest.mark.parametrize(
    "settings",
    [
        pytest.param({"hue_shift": 90}, id="a hue shift"),
        pytest.param({"saturation": 3.0}, id="saturation"),
        pytest.param({"lightness": -1.0}, id="lightness"),
    ],
)
def test_greys_have_no_hue_so_no_range_selects_them(settings: dict[str, float]) -> None:
    ramp = np.linspace(0, 1, 11, dtype=np.float32)[None, :, None].repeat(3, axis=2)
    np.testing.assert_array_equal(registry.apply(NAME, ramp, hue=0, width=360, feather=0, **settings), ramp)


def test_the_defaults_leave_a_photo_unchanged(photo: np.ndarray) -> None:
    np.testing.assert_allclose(registry.apply(NAME, photo), photo, atol=1e-6)


def test_a_photo_with_neutral_adjustments_is_unchanged_in_any_range(photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, hue=200, width=360, feather=0, hue_shift=0, saturation=1.0, lightness=0.0)
    np.testing.assert_allclose(out, photo, atol=1e-6)


@pytest.mark.parametrize(
    ("settings", "match"),
    [
        pytest.param({"hue": 361}, "hue", id="hue centre past a full turn"),
        pytest.param({"width": -1}, "width", id="negative width"),
        pytest.param({"feather": 181}, "feather", id="feather past half a turn"),
        pytest.param({"hue_shift": 200}, "hue_shift", id="shift past half a turn"),
        pytest.param({"saturation": -0.1}, "saturation", id="negative saturation gain"),
        pytest.param({"lightness": 1.5}, "lightness", id="lightness above 1"),
        pytest.param({"range": "reds"}, "range", id="named ranges are written as a hue centre"),
    ],
)
def test_a_setting_that_is_out_of_range_or_unknown_is_refused(
    settings: dict[str, float], match: str, photo: np.ndarray
) -> None:
    with pytest.raises(ValueError, match=match):
        registry.apply(NAME, photo, **settings)
