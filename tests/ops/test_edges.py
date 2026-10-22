"""Behaviour of photo_fx.ops.edges, stated as properties of the output (never as a golden image)."""

import numpy as np
import pytest
from scipy import ndimage

from photo_fx.ops.edges import Edges, detect_edges

METHODS = ["sobel", "dog"]  # "canny" joins in the Canny commit


def vertical_step(h=40, w=60, low=0.0, high=1.0, column=30):
    """Left of `column` is `low`, from `column` on is `high`."""
    img = np.full((h, w), low, dtype=np.float32)
    img[:, column:] = high
    return img


def blocks(seed=7, shape=(8, 11), size=6):
    """A grey picture of random flat squares: plenty of straight edges of every strength and sign."""
    rng = np.random.default_rng(seed)
    return np.kron(rng.random(shape, dtype=np.float32), np.ones((size, size), dtype=np.float32))


def luminance(rgb):
    """Rec. 709 luminance, written out independently of the module."""
    return (0.2126 * rgb[..., 0] + 0.7152 * rgb[..., 1] + 0.0722 * rgb[..., 2]).astype(np.float32)


# --- interface: validation, shapes, purity -------------------------------------------------------------------------


@pytest.mark.parametrize(
    ("kwargs", "name"),
    [
        ({"sigma": 0.0}, "sigma"),
        ({"sigma": -1.0}, "sigma"),
        ({"low": 0.3, "high": 0.3}, "low"),
        ({"low": 0.5, "high": 0.2}, "low"),
        ({"high": 1.5}, "high"),
        ({"low": -0.1}, "low"),
        ({"method": "laplacian"}, "method"),
    ],
)
def test_bad_arguments_raise_value_error_naming_the_argument(kwargs, name):
    with pytest.raises(ValueError, match=name):
        detect_edges(vertical_step(), **kwargs)


@pytest.mark.parametrize("shape", [(8,), (8, 8, 4), (8, 8, 1), (2, 8, 8, 3)])
def test_unsupported_image_shape_raises_value_error_naming_image(shape):
    with pytest.raises(ValueError, match="image"):
        detect_edges(np.zeros(shape, dtype=np.float32))


@pytest.mark.parametrize("method", METHODS)
@pytest.mark.parametrize("shape", [(40, 60), (40, 60, 3)])
def test_output_shapes_and_dtypes(method, shape):
    rng = np.random.default_rng(3)
    edges = detect_edges(rng.random(shape, dtype=np.float32), method)
    assert isinstance(edges, Edges)
    for field in (edges.magnitude, edges.direction):
        assert field.shape == (40, 60)
        assert field.dtype == np.float32


@pytest.mark.parametrize("method", METHODS)
def test_input_is_not_modified(method):
    rng = np.random.default_rng(4)
    img = rng.random((30, 40, 3), dtype=np.float32)
    before = img.copy()
    detect_edges(img, method)
    np.testing.assert_array_equal(img, before)


def test_float64_input_is_accepted_and_gives_float32_output():
    edges = detect_edges(vertical_step().astype(np.float64))
    assert edges.magnitude.dtype == np.float32
    assert edges.magnitude.max() > 0


@pytest.mark.parametrize("method", METHODS)
@pytest.mark.parametrize("value", [0.0, 0.37, 1.0])
def test_flat_image_has_zero_magnitude(method, value):
    edges = detect_edges(np.full((32, 48), value, dtype=np.float32), method)
    assert not edges.magnitude.any()


@pytest.mark.parametrize("method", METHODS)
def test_one_pixel_image_does_not_crash_and_has_no_edges(method):
    edges = detect_edges(np.full((1, 1), 0.5, dtype=np.float32), method)
    assert edges.magnitude.shape == (1, 1)
    assert edges.magnitude[0, 0] == 0


@pytest.mark.parametrize("method", METHODS)
def test_rgb_equals_the_grey_result_of_its_rec709_luminance(method):
    rng = np.random.default_rng(5)
    rgb = rng.random((30, 40, 3), dtype=np.float32)
    from_rgb = detect_edges(rgb, method)
    from_grey = detect_edges(luminance(rgb), method)
    np.testing.assert_allclose(from_rgb.magnitude, from_grey.magnitude, atol=1e-6)
    np.testing.assert_allclose(from_rgb.direction, from_grey.direction, atol=1e-5)


def test_rgb_uses_luminance_weights_not_a_plain_mean():
    green = np.zeros((40, 60, 3), dtype=np.float32)
    green[:, 30:, 1] = 1.0
    blue = np.zeros((40, 60, 3), dtype=np.float32)
    blue[:, 30:, 2] = 1.0
    green_strength = detect_edges(green).magnitude.max()
    blue_strength = detect_edges(blue).magnitude.max()
    # Rec. 709 weights green at 0.7152 and blue at 0.0722: a ratio of about 9.9, where a plain mean would give 1.
    assert green_strength / blue_strength == pytest.approx(0.7152 / 0.0722, rel=0.02)


# --- step edges: where they are and which way they point -----------------------------------------------------------


def step_tolerance(method, sigma):
    """How far from the step the strongest response may sit. |DoG| is zero on the step itself and peaks beside it."""
    return 2 * sigma + 1 if method == "dog" else 1


@pytest.mark.parametrize("method", METHODS)
@pytest.mark.parametrize("sigma", [1.0, 2.0])
def test_vertical_step_peaks_at_the_step_column_with_direction_zero(method, sigma):
    edges = detect_edges(vertical_step(column=30), method, sigma=sigma)
    row = 20
    peak_column = int(np.argmax(edges.magnitude[row]))
    assert abs(peak_column - 30) <= step_tolerance(method, sigma)
    assert abs(edges.direction[row, peak_column]) < 0.05
    # The response is the same along the whole straight edge: nothing depends on the row.
    np.testing.assert_allclose(edges.magnitude[5:-5], np.tile(edges.magnitude[row], (30, 1)), atol=1e-6)


@pytest.mark.parametrize("method", METHODS)
def test_horizontal_step_dark_above_bright_below_points_down_which_is_plus_half_pi(method):
    # Convention: x to the right, y DOWN the picture, direction = atan2(d/dy, d/dx), so "rising downwards" is +pi/2.
    img = vertical_step(column=20).T  # dark rows 0..19, bright rows 20.. : a horizontal step
    edges = detect_edges(img, method)
    peak_row = int(np.argmax(edges.magnitude[:, 30]))
    assert abs(peak_row - 20) <= step_tolerance(method, 1.0)
    assert edges.direction[peak_row, 30] == pytest.approx(np.pi / 2, abs=0.05)


@pytest.mark.parametrize("method", METHODS)
def test_horizontal_step_bright_above_dark_below_points_up_which_is_minus_half_pi(method):
    img = vertical_step(low=1.0, high=0.0, column=20).T
    edges = detect_edges(img, method)
    peak_row = int(np.argmax(edges.magnitude[:, 30]))
    assert edges.direction[peak_row, 30] == pytest.approx(-np.pi / 2, abs=0.05)


@pytest.mark.parametrize("method", METHODS)
def test_reversing_brightness_flips_the_direction_by_pi(method):
    forward = detect_edges(vertical_step(), method)
    reverse = detect_edges(vertical_step(low=1.0, high=0.0), method)
    row = 20
    column = int(np.argmax(forward.magnitude[row]))
    assert forward.direction[row, column] == pytest.approx(0.0, abs=0.05)
    assert reverse.direction[row, column] == pytest.approx(np.pi, abs=0.05)
    np.testing.assert_allclose(forward.magnitude, reverse.magnitude, atol=1e-6)


@pytest.mark.parametrize("method", METHODS)
def test_direction_stays_in_the_half_open_interval_minus_pi_exclusive_to_pi_inclusive(method):
    # A leftward gradient (bright to the left) has y-gradient exactly 0 and is where atan2 can return -pi.
    edges = detect_edges(vertical_step(low=1.0, high=0.0), method)
    assert edges.direction.min() > -np.pi
    assert edges.direction.max() <= np.float32(np.pi)


@pytest.mark.parametrize("method", METHODS)
def test_a_tilted_ramp_has_the_direction_of_its_slope(method):
    # Unequal slopes (x: 0.002/px, y: 0.004/px) so a swapped axis or a flipped y would give a different angle.
    y, x = np.mgrid[0:60, 0:80].astype(np.float32)
    edges = detect_edges(0.1 + 0.002 * x + 0.004 * y, method)
    interior = edges.direction[8:-8, 8:-8]
    np.testing.assert_allclose(interior, np.arctan2(0.004, 0.002), atol=1e-3)


@pytest.mark.parametrize("method", METHODS)
def test_a_flat_region_next_to_a_step_has_no_response_far_from_it(method):
    edges = detect_edges(vertical_step(column=30), method, sigma=1.0)
    assert edges.magnitude[:, :15].max() < 1e-3
    assert edges.magnitude[:, 45:].max() < 1e-3


# --- magnitude: range, normalisation, linearity --------------------------------------------------------------------


def best_binary_3x3_for_sobel():
    """Brute force: the 0/1 3x3 patch with the strongest Sobel response, using hand-written kernels."""
    kx = np.array([[-1, 0, 1], [-2, 0, 2], [-1, 0, 1]], dtype=float)
    best, best_patch = -1.0, None
    for bits in range(512):
        patch = np.array([(bits >> i) & 1 for i in range(9)], dtype=float).reshape(3, 3)
        strength = np.hypot((kx * patch).sum(), (kx.T * patch).sum())
        if strength > best:
            best, best_patch = strength, patch
    return best, best_patch


def test_the_strongest_possible_sobel_patch_reads_exactly_one():
    strength, patch = best_binary_3x3_for_sobel()
    assert strength == pytest.approx(np.sqrt(20))  # the corner pattern, stronger than the axis-aligned step (4)
    img = np.zeros((7, 7), dtype=np.float32)
    img[2:5, 2:5] = patch
    # sigma this small makes the pre-blur the identity, so the 3x3 patch reaches the Sobel kernel untouched.
    edges = detect_edges(img, "sobel", sigma=0.01)
    assert edges.magnitude[3, 3] == pytest.approx(1.0, abs=1e-5)


def test_a_clean_black_to_white_step_is_strong_but_below_the_corner_maximum():
    edges = detect_edges(vertical_step(), "sobel", sigma=0.01)
    assert edges.magnitude.max() == pytest.approx(4 / np.sqrt(20), abs=1e-5)


def test_the_strongest_possible_difference_of_gaussians_patch_reads_exactly_one():
    sigma = 1.0
    delta = np.zeros((41, 41), dtype=np.float64)
    delta[20, 20] = 1.0
    # The test builds its own kernel, independent of the module.
    kernel = ndimage.gaussian_filter(delta, sigma) - ndimage.gaussian_filter(delta, 1.6 * sigma)
    img = (kernel > 0).astype(np.float32)  # exactly the pixels the kernel weighs positively
    edges = detect_edges(img, "dog", sigma=sigma)
    # Correlating that picture with the (symmetric) kernel at the centre gives sum(kernel[kernel > 0]), the most any
    # 0..1 picture can give; reading exactly 1.0 there means the normalisation divides by that.
    assert kernel[kernel > 0].sum() > 0.05  # the kernel is not degenerate, so the 1.0 below is not 0/0
    assert edges.magnitude[20, 20] == pytest.approx(1.0, abs=1e-4)


@pytest.mark.parametrize("method", METHODS)
@pytest.mark.parametrize("sigma", [0.5, 1.0, 3.0])
def test_magnitude_stays_within_zero_and_one_on_binary_noise(method, sigma):
    rng = np.random.default_rng(11)
    img = (rng.random((64, 64)) > 0.5).astype(np.float32)
    magnitude = detect_edges(img, method, sigma=sigma).magnitude
    assert magnitude.min() >= 0
    assert magnitude.max() <= 1 + 1e-6
    assert magnitude.max() > 0.1  # binary noise is full of edges, so the bound is not met by returning zeros


@pytest.mark.parametrize("method", METHODS)
def test_half_the_contrast_gives_half_the_magnitude(method):
    picture = blocks()
    full = detect_edges(picture, method).magnitude
    half = detect_edges(0.5 * picture, method).magnitude
    assert full.max() > 0.1
    np.testing.assert_allclose(half, 0.5 * full, rtol=0.05, atol=1e-6)


@pytest.mark.parametrize("method", METHODS)
def test_adding_a_constant_brightness_offset_changes_nothing(method):
    picture = blocks()
    base = detect_edges(picture, method)
    shifted = detect_edges(picture + 0.25, method)
    np.testing.assert_allclose(shifted.magnitude, base.magnitude, atol=1e-5)
    strong = base.magnitude > 0.05
    assert strong.sum() > 100
    np.testing.assert_allclose(shifted.direction[strong], base.direction[strong], atol=1e-3)


@pytest.mark.parametrize("method", METHODS)
def test_transposing_the_image_transposes_the_magnitude_and_mirrors_the_direction(method):
    picture = blocks()
    base = detect_edges(picture, method)
    transposed = detect_edges(picture.T, method)
    np.testing.assert_allclose(transposed.magnitude, base.magnitude.T, atol=1e-6)
    # Swapping x and y turns the gradient angle t into pi/2 - t.
    strong = base.magnitude.T > 0.05
    assert strong.sum() > 100
    np.testing.assert_allclose(np.cos(transposed.direction[strong]), np.sin(base.direction.T[strong]), atol=1e-3)
    np.testing.assert_allclose(np.sin(transposed.direction[strong]), np.cos(base.direction.T[strong]), atol=1e-3)


@pytest.mark.parametrize("sigma", [1.0, 2.5])
def test_dog_profile_across_a_step_is_the_difference_of_a_sigma_and_a_one_point_six_sigma_blur(sigma):
    img = vertical_step(column=30)
    expected = np.abs(
        ndimage.gaussian_filter(img, sigma, mode="nearest") - ndimage.gaussian_filter(img, 1.6 * sigma, mode="nearest")
    )
    magnitude = detect_edges(img, "dog", sigma=sigma).magnitude
    # Compared as shapes, so the test does not repeat the module's normalisation.
    np.testing.assert_allclose(magnitude[20] / magnitude[20].max(), expected[20] / expected[20].max(), atol=1e-4)
