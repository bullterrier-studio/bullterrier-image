"""Behaviour of photo_fx.ops.edges, stated as properties of the output (never as a golden image)."""

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


@pytest.mark.parametrize("method", LINEAR_METHODS)
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
