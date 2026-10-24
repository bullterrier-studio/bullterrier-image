"""Behaviour of the `levels` effect, against hand-computed points and a pixel-by-pixel reference written here."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "levels"


def grey(*values: float) -> np.ndarray:
    """A 1 x n picture whose pixel i is the grey `values[i]`."""
    return np.repeat(np.array(values, np.float32)[None, :, None], 3, axis=2)


def reference(
    value: float, *, black: float, white: float, gamma: float, output_black: float, output_white: float
) -> float:
    """Photoshop's Levels for one channel value, one pixel at a time: normalise between the input points, clip, apply
    the midtone gamma (out = in ** (1 / gamma)), then place between the output points."""
    x = min(max((value - black) / (white - black), 0.0), 1.0)
    return output_black + x ** (1.0 / gamma) * (output_white - output_black)


def test_the_defaults_leave_the_picture_unchanged(photo: np.ndarray) -> None:
    np.testing.assert_allclose(registry.apply(NAME, photo), photo, atol=1e-6)


@pytest.mark.parametrize(
    ("settings", "values", "expected"),
    [
        pytest.param({"black": 0.2, "white": 0.8}, [0.2, 0.5, 0.8], [0.0, 0.5, 1.0], id="input points land on 0 and 1"),
        pytest.param(
            {"black": 0.2, "white": 0.8}, [0.0, 0.1, 0.9, 1.0], [0, 0, 1, 1], id="beyond the points is clipped"
        ),
        pytest.param({"black": 0.1, "white": 0.5}, [0.2, 0.3], [0.25, 0.5], id="linear between the points (gamma 1)"),
        pytest.param({"gamma": 2.0}, [0.25, 0.81], [0.5, 0.9], id="gamma 2 brightens the midtones: square root"),
        pytest.param({"gamma": 0.5}, [0.5, 0.9], [0.25, 0.81], id="gamma 0.5 darkens the midtones: square"),
        pytest.param({"gamma": 4.0}, [0.0, 1.0], [0.0, 1.0], id="gamma leaves black and white where they are"),
        pytest.param(
            {"output_black": 0.2, "output_white": 0.8},
            [0.0, 0.5, 1.0],
            [0.2, 0.5, 0.8],
            id="output points set the range",
        ),
        pytest.param(
            {"output_black": 1.0, "output_white": 0.0},
            [0.0, 0.25, 1.0],
            [1.0, 0.75, 0.0],
            id="swapped output is a negative",
        ),
        pytest.param(
            {"black": 0.25, "white": 0.75, "gamma": 2.0, "output_black": 0.1, "output_white": 0.9},
            [0.25, 0.5, 0.75],
            [0.1, 0.1 + 0.8 * 0.5**0.5, 0.9],
            id="all five together",
        ),
        pytest.param(
            {"black": 0.5, "white": 0.5},
            [0.499, 0.501],
            [0.0, 1.0],
            id="equal input points make a threshold, not a division by zero",
        ),
        pytest.param(
            {"black": 0.6, "white": 0.4}, [0.3, 0.7], [0.0, 1.0], id="crossed input points still make a threshold"
        ),
    ],
)
def test_known_points_are_remapped_as_worked_out_by_hand(
    settings: dict[str, float], values: list[float], expected: list[float]
) -> None:
    out = registry.apply(NAME, grey(*values), **settings)
    np.testing.assert_allclose(out[0, :, 0], expected, atol=1e-5)


def test_every_channel_is_remapped_on_its_own_like_the_pixel_by_pixel_reference(photo: np.ndarray) -> None:
    settings = {"black": 0.15, "white": 0.85, "gamma": 1.6, "output_black": 0.05, "output_white": 0.95}
    small = np.ascontiguousarray(photo[::16, ::16])  # 6 x 8 pixels, 144 values: the loop below is the oracle
    expected = np.array([[[reference(float(v), **settings) for v in px] for px in row] for row in small], np.float32)
    np.testing.assert_allclose(registry.apply(NAME, small, **settings), expected, atol=1e-5)


@pytest.mark.parametrize(
    "settings",
    [
        pytest.param({"black": 0.1, "white": 0.9, "gamma": 0.4}, id="dark midtones"),
        pytest.param({"black": 0.3, "white": 0.7, "gamma": 3.0}, id="bright midtones, narrow input"),
        pytest.param({"output_black": 0.2, "output_white": 0.6, "gamma": 1.7}, id="reduced output range"),
    ],
)
def test_a_brighter_input_is_never_darker_in_the_output(photo: np.ndarray, settings: dict[str, float]) -> None:
    ramp = np.sort(photo[..., 0].ravel())  # every red value of the photo, darkest first
    out = registry.apply(NAME, np.repeat(ramp[None, :, None], 3, axis=2), **settings)[0, :, 0]
    assert np.all(np.diff(out) >= -1e-6)


@pytest.mark.parametrize("output_black", [0.0, 0.3, 0.7, 1.0])
@pytest.mark.parametrize("output_white", [0.0, 0.3, 0.7, 1.0])
@pytest.mark.parametrize("gamma", [0.1, 1.0, 10.0])
def test_the_output_never_leaves_0_to_1_not_even_by_rounding(
    photo: np.ndarray, output_black: float, output_white: float, gamma: float
) -> None:
    out = registry.apply(
        NAME, photo, black=0.2, white=0.8, gamma=gamma, output_black=output_black, output_white=output_white
    )
    assert out.min() >= 0.0
    assert out.max() <= 1.0
