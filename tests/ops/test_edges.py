"""Behaviour of photo_fx.ops.edges, stated as properties of the output (never as a golden image)."""

import math
import time

import numpy as np
import pytest
from scipy import ndimage

from photo_fx.ops.edges import Edges, detect_edges

METHODS = ["sobel", "canny", "dog"]
LINEAR_METHODS = ["sobel", "dog"]  # canny is binary, so contrast scaling does not apply to it


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
        ({"dog_ratio": 1.0}, "dog_ratio"),
        ({"dog_ratio": 0.5}, "dog_ratio"),
        ({"truncate": 0.0}, "truncate"),
        ({"truncate": -1.0}, "truncate"),
        ({"border_mode": "zeros"}, "border_mode"),
        ({"tie_tolerance": -0.1}, "tie_tolerance"),
        ({"tie_tolerance": 1.0}, "tie_tolerance"),
        ({"connectivity": 6}, "connectivity"),
    ],
)
def test_bad_arguments_raise_value_error_naming_the_argument(kwargs, name):
    with pytest.raises(ValueError, match=name):
        detect_edges(vertical_step(), **kwargs)


@pytest.mark.parametrize("shape", [(8,), (8, 8, 4), (8, 8, 1), (2, 8, 8, 3), (0, 5), (5, 0), (0, 5, 3), (0,)])
def test_unsupported_or_empty_image_shape_raises_value_error_naming_image(shape):
    with pytest.raises(ValueError, match="image"):
        detect_edges(np.zeros(shape, dtype=np.float32))


@pytest.mark.parametrize("method", METHODS)
@pytest.mark.parametrize("bad_value", [np.nan, np.inf, -np.inf], ids=["nan", "inf", "minus-inf"])
@pytest.mark.parametrize("shape", [(24, 30), (24, 30, 3)])
def test_pictures_with_nan_or_infinity_are_rejected_naming_image(method, bad_value, shape):
    picture = np.full(shape, 0.5, dtype=np.float32)
    picture[7, 9] = bad_value
    with pytest.raises(ValueError, match="image"):
        detect_edges(picture, method)


@pytest.mark.parametrize("method", METHODS)
@pytest.mark.parametrize(("left", "right"), [(1.0, 1.2), (0.0, -0.3), (1.0, 255.0)])
def test_overshoot_beyond_the_range_is_clipped_so_a_step_that_clipping_flattens_has_no_edge(method, left, right):
    # Both sides clip to the same value, so there is no edge to find. A missing clip would report one.
    picture = np.full((30, 40), left, dtype=np.float32)
    picture[:, 20:] = right
    assert not detect_edges(picture, method).magnitude.any()


@pytest.mark.parametrize("method", METHODS)
@pytest.mark.parametrize("shape", [(48, 66), (48, 66, 3)])
def test_values_outside_zero_to_one_give_exactly_the_result_for_the_clipped_picture(method, shape):
    rng = np.random.default_rng(8)
    cells = 1.6 * rng.random((8, 11) + shape[2:], dtype=np.float32) - 0.3  # flat squares between -0.3 and 1.3
    picture = np.kron(cells, np.ones((6, 6) + (1,) * (len(shape) - 2), dtype=np.float32))
    assert (picture > 1).any()
    assert (picture < 0).any()
    clipped = np.clip(picture, 0, 1)
    unclipped_anyway = detect_edges(picture, method)
    expected = detect_edges(clipped, method)
    np.testing.assert_array_equal(unclipped_anyway.magnitude, expected.magnitude)
    np.testing.assert_array_equal(unclipped_anyway.direction, expected.direction)
    assert expected.magnitude.max() > 0.1  # the comparison is between real edge maps, not two empty ones


def test_eight_bit_scale_pictures_are_not_detected_and_are_clipped_to_black_and_white():
    rng = np.random.default_rng(9)
    eight_bit = np.kron(rng.integers(0, 256, (8, 11), dtype=np.uint8), np.ones((6, 6), dtype=np.uint8))
    as_clipped = np.clip(eight_bit.astype(np.float32), 0, 1)  # every non-zero value becomes 1
    for method in METHODS:
        np.testing.assert_array_equal(
            detect_edges(eight_bit, method).magnitude, detect_edges(as_clipped, method).magnitude
        )


@pytest.mark.parametrize("method", METHODS)
def test_the_boundary_values_zero_and_one_are_accepted(method):
    picture = np.zeros((10, 10), dtype=np.float32)
    picture[:, 5:] = 1.0
    assert detect_edges(picture, method).magnitude.max() > 0


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


@pytest.mark.parametrize("method", LINEAR_METHODS)
def test_half_the_contrast_gives_half_the_magnitude(method):
    picture = blocks()
    full = detect_edges(picture, method).magnitude
    half = detect_edges(0.5 * picture, method).magnitude
    assert full.max() > 0.1
    np.testing.assert_allclose(half, 0.5 * full, rtol=0.05, atol=1e-6)


@pytest.mark.parametrize("method", METHODS)
def test_adding_a_constant_brightness_offset_changes_nothing(method):
    # Headroom below 1 for the offset: brightness above 1 is clipped, and clipping is not offset-invariant.
    picture = 0.7 * blocks()
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


# --- canny: thin lines, hysteresis ---------------------------------------------------------------------------------


def edge_pixels(edges):
    return edges.magnitude > 0


def test_canny_magnitude_is_exactly_one_on_kept_pixels_and_zero_elsewhere():
    magnitude = detect_edges(blocks(), "canny").magnitude
    assert set(np.unique(magnitude)) == {0.0, 1.0}


def test_canny_gives_a_line_one_pixel_wide_on_a_straight_step():
    on = edge_pixels(detect_edges(vertical_step(), "canny"))
    assert on.any()
    padded = np.pad(on, 1)
    neighbours = sum(
        padded[1 + dy : 1 + dy + on.shape[0], 1 + dx : 1 + dx + on.shape[1]]
        for dy in (-1, 0, 1)
        for dx in (-1, 0, 1)
        if (dy, dx) != (0, 0)
    )
    assert neighbours[on].max() <= 3
    assert on[20].sum() == 1  # one pixel across the edge, on every row
    assert abs(int(np.argmax(on[20])) - 30) <= 1


def test_canny_is_blind_to_contrast_because_thresholds_are_shares_of_the_strongest_gradient():
    picture = blocks()
    np.testing.assert_array_equal(
        detect_edges(picture, "canny").magnitude, detect_edges(0.5 * picture, "canny").magnitude
    )


def strong_then_weak_edge_with_an_isolated_weak_edge():
    """A bright band whose left edge fades from contrast 1.0 (top) to 0.2 (bottom): one line, strong then weak.

    A second, separate step of the same 0.2 contrast stands at column 80, 20 px from the band. Gradient shares after
    normalising by the strongest are about 0.2 for both weak edges, between low=0.1 and high=0.3.
    """
    img = np.zeros((80, 100), dtype=np.float32)
    level = np.interp(np.arange(80), [0, 20, 40, 79], [1.0, 1.0, 0.2, 0.2]).astype(np.float32)
    img[:, 20:60] = level[:, None]
    img[:, 80:] = 0.2
    return img


def test_hysteresis_keeps_a_weak_edge_connected_to_a_strong_one_and_drops_an_isolated_weak_edge():
    edges = detect_edges(strong_then_weak_edge_with_an_isolated_weak_edge(), "canny", low=0.1, high=0.3)
    on = edge_pixels(edges)
    assert on[5:15, 19:22].any()  # the strong part is found
    assert on[55:75, 19:22].any()  # the weak part of the same line is kept: it is connected
    assert not on[:, 75:].any()  # the isolated weak step (same 0.2 contrast) is dropped


def test_lowering_high_lets_the_isolated_weak_edge_start_its_own_line():
    img = strong_then_weak_edge_with_an_isolated_weak_edge()
    on = edge_pixels(detect_edges(img, "canny", low=0.05, high=0.15))
    assert on[:, 75:].any()


def test_raising_low_above_the_weak_strength_disconnects_the_weak_part_of_the_line():
    img = strong_then_weak_edge_with_an_isolated_weak_edge()
    on = edge_pixels(detect_edges(img, "canny", low=0.25, high=0.3))
    assert on[5:15, 19:22].any()
    assert not on[55:75, 19:22].any()


@pytest.mark.parametrize("highs", [(0.2, 0.4), (0.4, 0.6), (0.6, 0.9)])
def test_raising_high_never_adds_edge_pixels(highs):
    picture = blocks() + 0.1 * np.random.default_rng(2).random((48, 66), dtype=np.float32)
    lower, higher = (edge_pixels(detect_edges(picture, "canny", low=0.1, high=h)) for h in highs)
    assert lower.sum() > higher.sum() > 0  # the thresholds really bite on this picture
    assert not (higher & ~lower).any()  # every pixel kept at the higher threshold was already kept at the lower one


# --- noise and speed -----------------------------------------------------------------------------------------------


@pytest.mark.parametrize("method", METHODS)
def test_a_larger_sigma_finds_fewer_false_edges_in_a_noisy_flat_region(method):
    rng = np.random.default_rng(0)
    scene = np.full((96, 128), 0.3, dtype=np.float32)
    scene[:, 64:] = 0.7  # a real step on the right keeps the relative thresholds of canny anchored to a real edge
    noisy = np.clip(scene + rng.normal(0, 0.2, scene.shape), 0, 1).astype(np.float32)

    def false_edges(sigma):
        flat_part = detect_edges(noisy, method, sigma=sigma).magnitude[:, :40]  # 24 px clear of the step
        return int((flat_part > 0.05).sum())

    fine, coarse = false_edges(1.0), false_edges(3.0)
    assert fine > 100  # the noise really does produce false edges at sigma=1
    assert coarse < fine / 2


def test_the_real_step_survives_the_noise_that_the_large_sigma_removes():
    rng = np.random.default_rng(0)
    scene = np.full((96, 128), 0.3, dtype=np.float32)
    scene[:, 64:] = 0.7
    noisy = np.clip(scene + rng.normal(0, 0.2, scene.shape), 0, 1).astype(np.float32)
    on = detect_edges(noisy, "canny", sigma=3.0).magnitude > 0
    assert on[:, 62:67].any(axis=1).mean() > 0.9  # most rows still have an edge pixel at the step


@pytest.mark.parametrize(("method", "budget_seconds"), [("sobel", 0.3), ("canny", 1.5)])
def test_a_1600_by_1600_picture_is_processed_within_budget(method, budget_seconds):
    picture = np.random.default_rng(0).random((1600, 1600), dtype=np.float32)  # noise: the most edges there can be
    timings = []
    # CPU time of this process, best of three: other processes sharing the machine do not fail a correct implementation.
    for _ in range(3):
        start = time.process_time()
        detect_edges(picture, method)
        timings.append(time.process_time() - start)
    assert min(timings) < budget_seconds


# --- canny on diagonal and curved edges (the suppression looks at a different neighbour pair for each direction) ----


def diagonal_step(orientation, size=64):
    """A 0|1 step along a 45 degree line, and each pixel's distance (px) from that line's geometric position."""
    y, x = np.mgrid[0:size, 0:size].astype(np.float32)
    if orientation == "anti-diagonal":  # bright where x + y >= size: the line runs from bottom-left to top-right
        return (x + y >= size).astype(np.float32), np.abs(x + y - (size - 0.5)) / np.sqrt(2)
    return (x - y >= 0).astype(np.float32), np.abs(x - y + 0.5) / np.sqrt(2)  # bright below-right of the main diagonal


def components(mask):
    return ndimage.label(mask, structure=np.ones((3, 3)))[1]


@pytest.mark.parametrize("orientation", ["anti-diagonal", "main-diagonal"])
def test_canny_traces_a_45_degree_step_as_one_unbroken_thin_line_on_the_true_edge(orientation):
    picture, distance = diagonal_step(orientation)
    on = detect_edges(picture, "canny").magnitude > 0
    inner = np.zeros_like(on)
    inner[8:56, 8:56] = True  # away from the picture corners where the step meets the border
    kept = on & inner

    assert distance[kept].max() <= 1.0  # on the geometric edge, not beside it
    assert components(kept) == 1  # one unbroken line
    assert kept[8:56].any(axis=1).all()  # a pixel in every row ...
    assert kept[:, 8:56].any(axis=0).all()  # ... and every column crosses the edge
    assert kept[8:56].sum(axis=1).max() <= 2  # at most two pixels thick
    assert kept[:, 8:56].sum(axis=0).max() <= 2


def test_canny_traces_a_disc_outline_as_one_closed_thin_ring_on_the_true_circle():
    y, x = np.mgrid[0:64, 0:64].astype(np.float32)
    radius = np.hypot(x - 31.5, y - 31.5)
    on = detect_edges((radius <= 20).astype(np.float32), "canny").magnitude > 0

    assert np.abs(radius[on] - 20.5).max() <= 1.5  # every kept pixel lies on the outline
    assert components(on) == 1
    angle = np.degrees(np.arctan2(y - 31.5, x - 31.5))
    for start in range(-180, 180, 5):  # no gap in any 5 degree wedge of the ring: the diagonal arcs included
        wedge = on & (angle >= start) & (angle < start + 5)
        assert wedge.any(), f"gap in the outline between {start} and {start + 5} degrees"
        assert np.ptp(radius[wedge]) <= 1.5  # and the ring is at most about two pixels thick there
    assert on.sum() <= 1.5 * 2 * np.pi * 20.5  # about one pixel of line per pixel of circumference


@pytest.mark.parametrize("bright_side", ["bright-after", "bright-before"])
@pytest.mark.parametrize(("axis", "expected"), [("main", {0, 1}), ("anti", {62, 63})])
def test_canny_gives_a_tie_to_the_pixel_that_comes_first_in_reading_order(axis, expected, bright_side):
    # A grey one-pixel diagonal between a dark and a bright side: the pixels on either side of it have exactly equal
    # gradients and are each other's neighbours across the edge, so only one may stay. Reading order (upper row first,
    # then left) names the winner: for x - y the pair is d = -1 / +1 and +1 is up-right; for x + y it is s = 62 / 64
    # and 62 is up-left. Each pair is a different diagonal sector of the suppression, so this pins both offsets.
    y, x = np.mgrid[0:64, 0:64]
    line = x - y if axis == "main" else x + y
    middle = 0 if axis == "main" else 63
    dark_side = line < middle if bright_side == "bright-after" else line > middle
    picture = np.where(line == middle, 0.5, np.where(dark_side, 0.0, 1.0)).astype(np.float32)
    on = detect_edges(picture, "canny", sigma=0.01).magnitude > 0
    interior = (y > 8) & (y < 56) & (x > 8) & (x < 56)
    assert set(line[on & interior]) == expected


# --- every value that shapes the result is a parameter -------------------------------------------------------------

BORDER_MODES = ["nearest", "reflect", "mirror", "wrap"]


def reference_sobel_magnitude(picture, sigma, truncate, border_mode):
    """Gaussian blur then Sobel, both straight from scipy with the given options: the independent oracle."""
    blurred = ndimage.gaussian_filter(picture, sigma, truncate=truncate, mode=border_mode)
    gx = ndimage.sobel(blurred, axis=1, mode=border_mode)
    gy = ndimage.sobel(blurred, axis=0, mode=border_mode)
    return np.hypot(gx, gy) / np.sqrt(20)


@pytest.mark.parametrize("border_mode", BORDER_MODES)
@pytest.mark.parametrize("truncate", [1.0, 2.5, 4.0])
def test_sobel_is_a_gaussian_blur_then_scipys_sobel_with_the_chosen_kernel_width_and_border(truncate, border_mode):
    picture = blocks()
    expected = reference_sobel_magnitude(picture, 2.0, truncate, border_mode)
    actual = detect_edges(picture, "sobel", sigma=2.0, truncate=truncate, border_mode=border_mode).magnitude
    np.testing.assert_allclose(actual, expected, atol=2e-5)


def test_the_border_mode_changes_the_edges_found_at_the_picture_border():
    ramp = np.tile(np.linspace(0, 1, 40, dtype=np.float32), (30, 1))  # dark left edge, bright right edge
    at_border = {mode: detect_edges(ramp, "sobel", border_mode=mode).magnitude[15, [0, -1]] for mode in BORDER_MODES}
    assert at_border["wrap"].min() > 5 * at_border["nearest"].max()  # wrapping glues bright to dark: a huge step
    assert len({round(float(v.max()), 5) for v in at_border.values()}) == 4  # no two modes are aliases


def test_defaults_are_nearest_borders_and_four_sigma_kernels_and_a_ratio_of_one_point_six():
    picture = blocks()
    for method in METHODS:
        default = detect_edges(picture, method)
        explicit = detect_edges(
            picture, method, dog_ratio=1.6, truncate=4.0, border_mode="nearest", tie_tolerance=1e-4, connectivity=8
        )
        np.testing.assert_array_equal(default.magnitude, explicit.magnitude)
        np.testing.assert_array_equal(default.direction, explicit.direction)


@pytest.mark.parametrize("border_mode", BORDER_MODES)
@pytest.mark.parametrize(
    ("sigma", "dog_ratio", "truncate"), [(1.0, 1.6, 4.0), (1.5, 2.0, 4.0), (1.0, 1.25, 3.0), (2.0, 3.0, 2.0)]
)
def test_dog_profile_is_the_difference_of_a_sigma_and_a_ratio_times_sigma_blur(sigma, dog_ratio, truncate, border_mode):
    picture = blocks()
    narrow = ndimage.gaussian_filter(picture, sigma, truncate=truncate, mode=border_mode)
    wide = ndimage.gaussian_filter(picture, dog_ratio * sigma, truncate=truncate, mode=border_mode)
    expected = np.abs(narrow - wide)
    actual = detect_edges(
        picture, "dog", sigma=sigma, dog_ratio=dog_ratio, truncate=truncate, border_mode=border_mode
    ).magnitude
    # Compared as shapes (both divided by their own maximum), so the test does not repeat the normalisation.
    np.testing.assert_allclose(actual / actual.max(), expected / expected.max(), atol=1e-4)


@pytest.mark.parametrize(("sigma", "dog_ratio", "truncate"), [(1.0, 2.5, 4.0), (1.5, 1.3, 2.0)])
def test_the_strongest_possible_dog_patch_reads_one_for_any_ratio_and_kernel_width(sigma, dog_ratio, truncate):
    delta = np.zeros((61, 61))
    delta[30, 30] = 1.0
    kernel = ndimage.gaussian_filter(delta, sigma, truncate=truncate) - ndimage.gaussian_filter(
        delta, dog_ratio * sigma, truncate=truncate
    )
    edges = detect_edges((kernel > 0).astype(np.float32), "dog", sigma=sigma, dog_ratio=dog_ratio, truncate=truncate)
    assert edges.magnitude[30, 30] == pytest.approx(1.0, abs=1e-4)


def test_tie_tolerance_decides_which_of_two_nearly_equal_gradients_wins():
    # Central differences along a row: 0, 0.4, 0.40002, 0: columns 29 and 30 differ by 0.005 percent.
    picture = np.zeros((20, 60), dtype=np.float32)
    picture[:, 30] = 0.4
    picture[:, 31:] = 0.40002
    sigma = 0.01  # narrower than a pixel: no blur

    def kept_columns(tie_tolerance):
        on = detect_edges(picture, "canny", sigma=sigma, tie_tolerance=tie_tolerance).magnitude > 0
        return set(np.nonzero(on[10])[0])

    assert kept_columns(1e-4) == {29}  # within tolerance they tie, and the earlier pixel wins
    assert kept_columns(0.0) == {30}  # exact comparison: the very slightly stronger pixel wins


def fading_diagonal_line():
    """A one-pixel bright diagonal fading from 1.0 to 0.25: strong edge pixels leading into weak ones."""
    y, x = np.mgrid[0:60, 0:60]
    level = np.interp(y, [0, 20, 40, 59], [1.0, 1.0, 0.25, 0.25])
    return np.where(x == y, level, 0.0).astype(np.float32)


def test_four_connectivity_follows_fewer_weak_edge_pixels_than_eight_because_diagonal_touches_do_not_link():
    picture = fading_diagonal_line()
    eight = detect_edges(picture, "canny", low=0.1, high=0.5, connectivity=8).magnitude > 0
    four = detect_edges(picture, "canny", low=0.1, high=0.5, connectivity=4).magnitude > 0
    assert not (four & ~eight).any()  # linking less never keeps more
    assert four[45:55].sum() < eight[45:55].sum()  # in the weak tail, diagonal links are what 8-connectivity adds
    assert four[:15].sum() == eight[:15].sum()  # the strong head is seeded pixel by pixel either way


# --- review round 2: exact range, non-finite arguments, fixed behaviours ------------------------------------------


@pytest.mark.parametrize(("sigma", "dog_ratio", "truncate"), [(0.7, 1.2, 4.0), (0.7, 2.0, 4.0), (0.7, 3.0, 3.0)])
@pytest.mark.parametrize("polarity", [1, -1])
def test_dog_magnitude_never_exceeds_one_even_for_the_strongest_possible_patch(sigma, dog_ratio, truncate, polarity):
    # In float32 the strongest patch can read 1.0000001 before clipping.
    size = 2 * int(truncate * dog_ratio * sigma + 0.5) + 9
    delta = np.zeros((size, size))
    delta[size // 2, size // 2] = 1.0
    kernel = ndimage.gaussian_filter(delta, sigma, truncate=truncate) - ndimage.gaussian_filter(
        delta, dog_ratio * sigma, truncate=truncate
    )
    picture = (polarity * kernel > 0).astype(np.float32)
    magnitude = detect_edges(picture, "dog", sigma=sigma, dog_ratio=dog_ratio, truncate=truncate).magnitude
    assert magnitude.max() <= 1.0
    assert magnitude.max() > 0.9999  # and it really is the strongest patch, so the bound was tested at its limit


@pytest.mark.parametrize("name", ["sigma", "dog_ratio", "truncate"])
@pytest.mark.parametrize("bad_value", [math.inf, math.nan], ids=["inf", "nan"])
def test_non_finite_sigma_dog_ratio_and_truncate_raise_value_error_naming_them(name, bad_value):
    with pytest.raises(ValueError, match=name):
        detect_edges(vertical_step(), **{name: bad_value})


@pytest.mark.parametrize("name", ["sigma", "low", "high", "dog_ratio", "truncate", "tie_tolerance"])
@pytest.mark.parametrize("bad_value", ["1.0", None, [1.0], 1.0 + 2.0j], ids=["str", "none", "list", "complex"])
def test_non_numeric_numeric_arguments_raise_value_error_naming_them(name, bad_value):
    with pytest.raises(ValueError, match=name):
        detect_edges(vertical_step(), **{name: bad_value})


def test_an_unhashable_border_mode_or_connectivity_raises_value_error_not_type_error():
    for name in ("border_mode", "connectivity", "method"):
        with pytest.raises(ValueError, match=name):
            detect_edges(vertical_step(), **{name: ["nearest"]})


def test_canny_suppression_treats_the_outside_of_the_picture_as_having_no_gradient():
    # Dark first row, bright below: the edge lies on the very top row. Its gradient is as strong as the row below,
    # and the row below wins any tie against a missing neighbour only if the outside copies the border (it must not).
    picture = np.ones((20, 30), dtype=np.float32)
    picture[0] = 0.0
    on = detect_edges(picture, "canny", sigma=0.01).magnitude > 0
    assert on[0, 4:-4].all()
    assert not on[1:].any(axis=None)


def two_steps_of_contrast_one_and_one_half():
    """Left to right: 0 -> 1 at column 10, 1 -> 0.5 at column 30. Gradient strengths are exactly 1 : 0.5."""
    picture = np.zeros((20, 50), dtype=np.float32)
    picture[:, 10:] = 1.0
    picture[:, 30:] = 0.5
    return picture


def test_canny_high_threshold_is_strict_an_edge_exactly_at_the_share_does_not_start_a_line():
    picture = two_steps_of_contrast_one_and_one_half()
    exactly_at = detect_edges(picture, "canny", sigma=0.01, low=0.1, high=0.5).magnitude > 0
    just_below = detect_edges(picture, "canny", sigma=0.01, low=0.1, high=0.4999).magnitude > 0
    assert exactly_at[:, 5:20].any()  # the strong step is found
    assert not exactly_at[:, 25:40].any()  # the half-strength step sits exactly on `high`: not above it
    assert just_below[:, 25:40].any()


def test_canny_low_threshold_is_strict_a_weak_pixel_exactly_at_the_share_is_not_linked():
    # A vertical edge of contrast 1 above row 20 and 0.5 below it: one line, strong then exactly half as strong.
    picture = np.zeros((40, 30), dtype=np.float32)
    picture[:20, 10:] = 1.0
    picture[20:, 10:] = 0.5
    linked = detect_edges(picture, "canny", sigma=0.01, low=0.4999, high=0.9).magnitude > 0
    cut = detect_edges(picture, "canny", sigma=0.01, low=0.5, high=0.9).magnitude > 0
    assert linked[30:, 8:12].any()
    assert not cut[30:, 8:12].any()
