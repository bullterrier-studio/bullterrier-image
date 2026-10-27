"""Behaviour of the `ripple` effect: a travelling wave that moves each pixel perpendicular to the wave's direction."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "ripple"


def grey(values: np.ndarray) -> np.ndarray:
    """A picture with `values` (height, width) in all three channels."""
    return np.repeat(values.astype(np.float32)[..., None], 3, axis=2)


@pytest.mark.parametrize("interpolation", ["nearest", "bilinear", "bicubic"])
def test_a_ripple_of_no_amplitude_leaves_the_picture_unchanged(interpolation: str, photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, amplitude=0.0, interpolation=interpolation)
    np.testing.assert_array_equal(out, photo)
    assert not np.shares_memory(out, photo)  # a new picture, not the input handed back


@pytest.mark.parametrize(
    ("wavelength", "amplitude", "phase"),
    [(40.0, 6.3, 0.0), (40.0, 6.3, 90.0), (25.0, 4.7, -130.0), (64.0, 9.1, 200.0)],
    ids=["phase 0", "a quarter cycle on", "a negative phase", "a long wave"],
)
def test_a_line_across_the_wave_moves_down_by_amplitude_times_the_sine_at_its_column(
    wavelength: float, amplitude: float, phase: float
) -> None:
    """A white row at row 50 of a black picture 161 columns wide (centre column 80): the wave runs along the columns
    (angle 0) and is measured from the centre, so the pixel in column c is moved down by
    amplitude x sin(2 pi (c - 80) / wavelength + phase) px, and 'nearest' puts it on the closest row."""
    line = np.zeros((101, 161, 3), np.float32)
    line[50] = 1.0
    out = registry.apply(NAME, line, wavelength=wavelength, amplitude=amplitude, phase=phase, interpolation="nearest")
    cols = np.arange(161) - 80
    expected = np.floor(50 + amplitude * np.sin(2 * np.pi * cols / wavelength + np.radians(phase)) + 0.5)
    assert set(np.unique(out)) == {0.0, 1.0}  # 'nearest' keeps the picture's own values
    assert (out[..., 0].sum(axis=0) == 1).all()  # one white pixel in every column
    np.testing.assert_array_equal(out[..., 0].argmax(axis=0), expected)
    assert expected.max() - expected.min() >= 2 * round(amplitude) - 1  # and the line really is wavy


@pytest.mark.parametrize("interpolation", ["nearest", "bilinear"])
def test_a_phase_of_a_whole_cycle_is_no_phase_at_all(interpolation: str, photo: np.ndarray) -> None:
    plain = registry.apply(NAME, photo, phase=0.0, interpolation=interpolation)
    turned = registry.apply(NAME, photo, phase=360.0, interpolation=interpolation)
    np.testing.assert_allclose(turned, plain, atol=1e-5)
    assert np.abs(plain - photo).max() > 0.05  # and the ripple really changes the picture


@pytest.mark.parametrize(
    ("angle", "line_is"),
    [(0.0, "a full column"), (90.0, "a full row"), (180.0, "a full column"), (270.0, "a full row")],
    ids=["angle 0 moves along columns", "angle 90 moves along rows", "angle 180", "angle 270"],
)
def test_the_displacement_is_perpendicular_to_the_wave_so_a_line_along_it_does_not_change(
    angle: float, line_is: str
) -> None:
    """The wave moves pixels up and down for angle 0 and left and right for angle 90, so a white column (angle 0) or
    row (angle 90) slides along itself; 'nearest' edges keep its ends white. A line across the movement does change."""
    column = np.zeros((81, 81, 3), np.float32)
    column[:, 40] = 1.0
    along = column if line_is == "a full column" else column.transpose(1, 0, 2).copy()
    across = along.transpose(1, 0, 2).copy()
    settings = {"angle": angle, "amplitude": 7.0, "wavelength": 30.0, "interpolation": "nearest", "edge": "nearest"}
    np.testing.assert_array_equal(registry.apply(NAME, along, **settings), along)
    assert np.abs(registry.apply(NAME, across, **settings) - across).max() == 1.0


@pytest.mark.parametrize("angle", [30.0, 285.0, 180.0])
def test_the_angle_turns_the_whole_ripple_clockwise_about_the_centre(angle: float) -> None:
    """Turning the picture a quarter turn clockwise, rippling it at `angle` and turning it back is the same as
    rippling the original at angle - 90: the angle runs clockwise, like the picture's rows run down."""
    noise = np.random.default_rng(4).random((64, 64, 3), dtype=np.float32)
    settings = {"wavelength": 20.0, "amplitude": 3.0, "phase": 40.0, "interpolation": "bilinear"}
    turned = registry.apply(NAME, np.rot90(noise, -1).copy(), angle=angle, **settings)
    direct = registry.apply(NAME, noise, angle=angle - 90, **settings)
    np.testing.assert_allclose(np.rot90(turned, 1), direct, atol=1e-4)


@pytest.mark.parametrize(
    ("interpolation", "power"),
    [("bilinear", 1), ("bicubic", 1), ("bicubic", 2)],
    ids=[
        "bilinear reads a linear ramp exactly",
        "bicubic reads a linear ramp exactly",
        "bicubic reads a parabola exactly",
    ],
)
def test_between_pixels_a_smooth_picture_is_read_as_the_interpolation_says(interpolation: str, power: int) -> None:
    """A picture whose value is (row / 100)^power is a ramp (power 1: linear down the rows, read exactly by bilinear and
    bicubic) or a parabola (power 2: only bicubic's cubic spline reads it exactly, away from the edge), so the pixel in
    column c (centre column 60) shows the picture at row - amplitude x sin(2 pi (c - 60) / wavelength)."""
    rows = np.tile(((np.arange(101) / 100.0) ** power)[:, None], (1, 121))
    out = registry.apply(NAME, grey(rows), wavelength=32.0, amplitude=5.5, phase=0.0, interpolation=interpolation)[
        ..., 0
    ]
    cols = np.arange(121) - 60
    source = np.arange(101)[:, None] - 5.5 * np.sin(2 * np.pi * cols / 32.0)[None, :]
    np.testing.assert_allclose(out[15:-15], ((source / 100.0) ** power)[15:-15], atol=2e-6)


@pytest.mark.parametrize(
    ("edge", "expected"),
    [("nearest", 0.0), ("mirror", 0.5), ("reflect", 0.45), ("wrap", 0.55)],
    ids=["nearest", "mirror", "reflect", "wrap"],
)
def test_edge_sets_what_a_pixel_sees_when_the_wave_moves_it_in_from_beyond_the_picture(
    edge: str, expected: float
) -> None:
    """A picture of 21 rows and columns whose value is row / 20, rippled with amplitude 10 and wavelength 40: the
    centre column 10 is where the wave is at zero, so the quarter wave is column 20 (the last), where the pixels are
    moved down 10 px and row 0 shows the point 10 px above the picture, where row -10 is
    nearest: the first row, 0; mirror: row 10 (mirrored about row 0), 0.5; reflect: row 9 (mirrored about the edge of
    the first pixel, which repeats it), 0.45; wrap: row 11 (the picture repeated, 21 rows), 0.55."""
    rows = np.tile((np.arange(21) / 20.0)[:, None], (1, 21))
    out = registry.apply(NAME, grey(rows), wavelength=40.0, amplitude=10.0, phase=0.0, edge=edge)
    assert out[0, 20, 0] == pytest.approx(expected, abs=1e-5)
