"""Behaviour of the `fractal` effect: the Mandelbrot set or a Julia set, coloured by smooth escape time."""

import math

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "fractal"
BAILOUT = 256.0  # the escape radius the effect documents for its smooth colouring
BLACK_TO_WHITE = ["#000000", "#ffffff"]


def render(shape: tuple[int, int] = (31, 41), **settings: object) -> np.ndarray:
    """The fractal over a picture of `shape` (h, w), brightness = position on a black-to-white ramp, interior red."""
    settings = {"colors": BLACK_TO_WHITE, "interior": "#ff0000", **settings}
    return registry.apply(NAME, np.zeros((*shape, 3), np.float32), **settings)


def is_interior(out: np.ndarray) -> np.ndarray:
    """(h, w) bool: the pixels in the interior colour (red: the only colour whose green and blue are 0 but red is 1)."""
    return (out[..., 0] == 1.0) & (out[..., 1] == 0.0)


def reference(
    shape: tuple[int, int], *, kind: str, zoom: float, x: float, y: float, c: complex, iterations: int, period: float
) -> tuple[np.ndarray, np.ndarray]:
    """Escape-time fractal in plain Python complex arithmetic, one pixel at a time: (brightness, interior), (h, w).

    The picture's shorter side spans 4 / zoom units of the plane, centred on x + iy, imaginary axis upwards; the
    pixel (row i, column j) is at distance (j - (w-1)/2, (i - (h-1)/2)) pixel steps from the centre. A point escapes
    when |z| > 256; its smooth count is n + 1 - log2(log|z| / log 256) for the n-th iterate, and the brightness is the
    triangle wave of count / period: 1 - |1 - (count / period) mod 2|.
    """
    height, width = shape
    step = 4 / zoom / min(shape)
    brightness, interior = np.zeros(shape), np.zeros(shape, bool)
    for i in range(height):
        for j in range(width):
            point = complex(x + (j - (width - 1) / 2) * step, y - (i - (height - 1) / 2) * step)
            z, offset = (point, c) if kind == "julia" else (0j, point)
            for n in range(1, iterations + 1):
                z = z * z + offset
                if abs(z) > BAILOUT:
                    count = n + 1 - math.log2(math.log(abs(z)) / math.log(BAILOUT))
                    brightness[i, j] = 1 - abs(1 - (count / period) % 2)
                    break
            else:
                interior[i, j] = True
    return brightness, interior


CASES = [
    pytest.param({"kind": "mandelbrot", "x": -0.5, "zoom": 1.0}, id="mandelbrot, whole set"),
    pytest.param({"kind": "mandelbrot", "x": -0.75, "y": 0.1, "zoom": 6.0}, id="mandelbrot, zoomed in near the neck"),
    pytest.param(
        {"kind": "julia", "c_real": -0.8, "c_imag": 0.156, "zoom": 1.0}, id="julia, a swirl, imaginary part up"
    ),
    pytest.param(
        {"kind": "julia", "c_real": 0.285, "c_imag": 0.01, "x": 0.1, "zoom": 2.0}, id="julia, off centre and zoomed"
    ),
]


@pytest.mark.parametrize("settings", CASES)
def test_the_picture_agrees_with_a_one_pixel_at_a_time_escape_time_calculation(settings: dict) -> None:
    """The effect iterates in single precision; a pixel right on an iteration boundary may fall either side, so a
    handful of pixels may differ, the rest must match."""
    shape, iterations, period = (31, 41), 40, 12.0
    out = render(shape, iterations=iterations, period=period, **settings)
    brightness, interior = reference(
        shape,
        kind=settings["kind"],
        zoom=settings.get("zoom", 1.0),
        x=settings.get("x", 0.0),
        y=settings.get("y", 0.0),
        c=complex(settings.get("c_real", -0.8), settings.get("c_imag", 0.156)),
        iterations=iterations,
        period=period,
    )
    assert is_interior(out).sum() == pytest.approx(interior.sum(), abs=0.01 * out[..., 0].size)
    assert np.mean(is_interior(out) == interior) > 0.99
    both_outside = ~is_interior(out) & ~interior
    assert np.mean(np.abs(out[..., 0] - brightness)[both_outside] < 2e-3) > 0.98
    assert both_outside.sum() > 100  # the case really has an exterior to compare


@pytest.mark.parametrize(
    ("settings", "inside"),
    [
        pytest.param({"kind": "mandelbrot", "x": 0.0}, True, id="the origin is in the Mandelbrot set"),
        pytest.param({"kind": "mandelbrot", "x": -2.0}, True, id="-2 is in it: its orbit is -2, 2, 2, 2..."),
        pytest.param({"kind": "mandelbrot", "x": 0.25}, True, id="0.25, the cusp, is in it: the orbit creeps to 1/2"),
        pytest.param({"kind": "mandelbrot", "x": 2.0}, False, id="2 is not: its orbit is 2, 6, 38, 1446..."),
        pytest.param({"kind": "mandelbrot", "x": 1.0}, False, id="1 is not: its orbit is 1, 2, 5, 26, 677..."),
    ],
)
def test_the_centre_pixel_is_interior_exactly_when_its_point_belongs_to_the_set(settings: dict, inside: bool) -> None:
    out = render((1, 1), iterations=200, zoom=8.0, **settings)  # a one-pixel picture is its centre point
    assert bool(is_interior(out)[0, 0]) is inside


def test_a_julia_set_is_unchanged_by_a_half_turn_about_the_origin() -> None:
    """z -> -z maps the orbit of z onto the orbit of -z, so the picture centred on 0 is its own half turn."""
    out = render((41, 51), kind="julia", c_real=-0.4, c_imag=0.6, x=0.0, y=0.0)
    np.testing.assert_array_equal(out, out[::-1, ::-1])


def test_more_iterations_never_make_an_exterior_pixel_interior() -> None:
    """A point that has not escaped after 100 iterations had not escaped after 20 either."""
    few = is_interior(render(iterations=20, kind="mandelbrot", x=-0.5))
    many = is_interior(render(iterations=100, kind="mandelbrot", x=-0.5))
    assert many.sum() > 0
    assert (many <= few).all()
    assert many.sum() < few.sum()  # and some points do take more than 20 iterations to escape


def test_the_julia_constant_is_ignored_by_the_mandelbrot_set_and_the_centre_by_nothing() -> None:
    np.testing.assert_array_equal(render(kind="mandelbrot", c_real=0.3), render(kind="mandelbrot", c_real=-0.9))
    assert not np.array_equal(render(kind="julia", c_real=0.3), render(kind="julia", c_real=-0.9))
    assert not np.array_equal(render(kind="julia", x=0.0), render(kind="julia", x=0.2))


def test_a_two_colour_ramp_mixes_its_two_colours_and_reaches_both_ends() -> None:
    """Black to white: every exterior pixel is a grey (equal channels), and with a short period the bands run the
    whole way from black to white."""
    out = render(kind="julia", period=5.0)
    outside = out[~is_interior(out)]
    np.testing.assert_allclose(outside[:, 0], outside[:, 1], atol=1e-6)
    np.testing.assert_allclose(outside[:, 0], outside[:, 2], atol=1e-6)
    assert outside[:, 0].min() < 0.1
    assert outside[:, 0].max() > 0.9
