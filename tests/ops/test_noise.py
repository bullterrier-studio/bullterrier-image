"""Properties of the seeded noise fields: determinism, range, crop consistency, feature size, no lattice artefacts."""

import time

import numpy as np
import pytest
from scipy import ndimage
from scipy.spatial import cKDTree

from photo_fx.ops.noise import fractal_noise, worley

KINDS = ["gradient", "value"]


def _correlation(a: np.ndarray, b: np.ndarray) -> float:
    return float(np.corrcoef(a.ravel(), b.ravel())[0, 1])


@pytest.mark.parametrize("kind", KINDS)
class TestFractalNoiseContract:
    def test_same_arguments_give_identical_arrays(self, kind):
        a = fractal_noise((64, 80), 16, 5, octaves=3, kind=kind)
        b = fractal_noise((64, 80), 16, 5, octaves=3, kind=kind)
        assert np.array_equal(a, b)

    def test_different_seed_gives_an_unrelated_field(self, kind):
        a = fractal_noise((128, 128), 16, 1, kind=kind)
        b = fractal_noise((128, 128), 16, 2, kind=kind)
        assert abs(_correlation(a, b)) < 0.1

    def test_float32_requested_shape_and_range_for_non_square(self, kind):
        field = fractal_noise((40, 90), 12, 3, octaves=3, kind=kind)
        assert field.dtype == np.float32
        assert field.shape == (40, 90)
        assert field.min() >= 0.0
        assert field.max() <= 1.0

    def test_crop_of_a_larger_field_equals_the_smaller_field(self, kind):
        # 37.5 divides neither 50 nor 100, so a lattice that depended on the canvas size would show up.
        big = fractal_noise((100, 100), 37.5, 7, octaves=3, kind=kind)
        small = fractal_noise((50, 50), 37.5, 7, octaves=3, kind=kind)
        assert np.array_equal(big[:50, :50], small)

    def test_crop_consistency_holds_for_non_square_canvases(self, kind):
        wide = fractal_noise((30, 120), 20, 4, octaves=2, kind=kind)
        narrow = fractal_noise((30, 45), 20, 4, octaves=2, kind=kind)
        assert np.array_equal(wide[:, :45], narrow)


def test_gradient_noise_uses_most_of_the_range():
    field = fractal_noise((256, 256), 64, 1)
    low, high = np.percentile(field, [1, 99])
    assert low < 0.25
    assert high > 0.75


def _roughness(field: np.ndarray) -> float:
    """Mean absolute difference between horizontally and vertically neighbouring pixels."""
    return float((np.abs(np.diff(field, axis=0)).mean() + np.abs(np.diff(field, axis=1)).mean()) / 2)


def _fine_detail(field: np.ndarray) -> float:
    """Variance of the Laplacian: high-frequency energy."""
    return float(ndimage.laplace(field.astype(np.float64)).var())


def _autocorrelation(field: np.ndarray, lag: int, axis: int) -> float:
    centred = field - field.mean()
    a, b = (centred[:, :-lag], centred[:, lag:]) if axis == 1 else (centred[:-lag], centred[lag:])
    return float((a * b).mean() / centred.var())


def _lag_anomaly(field: np.ndarray, lag: int) -> float:
    """How far the autocorrelation at `lag` sticks out from its neighbours at lag - 1 and lag + 1 (worst axis).

    A seamless field has an autocorrelation that varies smoothly with the lag, so the value at `lag` is close to the
    mean of its two neighbours and the anomaly is near 0. A lattice artefact (the field repeating every `lag` pixels,
    a seam on every lattice line) makes that one lag stand out. Comparing with the neighbouring lags, rather than
    with a fixed expected value, makes the statistic independent of how fast a particular field decorrelates, and
    taking the worse of the two axes catches artefacts along either one.
    """
    anomalies = []
    for axis in (0, 1):
        before, at, after = (_autocorrelation(field, lag + d, axis) for d in (-1, 0, 1))
        anomalies.append(abs(at - (before + after) / 2))
    return max(anomalies)


@pytest.mark.parametrize("kind", KINDS)
class TestFractalNoiseCharacter:
    def test_larger_scale_means_smoother_field(self, kind):
        roughness = [_roughness(fractal_noise((256, 256), s, 3, kind=kind)) for s in (8, 32, 128)]
        assert roughness[0] > 2 * roughness[1] > 4 * roughness[2]

    def test_more_octaves_add_fine_detail_and_stay_in_range(self, kind):
        # Averaged over seeds: a single 256x256 field holds only a few coarse features, so one seed is too noisy.
        detail = []
        for octaves in (1, 2, 3, 4):
            fields = [fractal_noise((256, 256), 32, seed, octaves=octaves, kind=kind) for seed in range(8)]
            detail.append(np.mean([_fine_detail(f) for f in fields]))
            assert min(f.min() for f in fields) >= 0.0
            assert max(f.max() for f in fields) <= 1.0
        assert detail == sorted(detail)
        assert detail[3] > 4 * detail[0]

    def test_lower_persistence_fades_the_fine_detail(self, kind):
        faint = fractal_noise((256, 256), 64, 3, octaves=4, persistence=0.3, kind=kind)
        strong = fractal_noise((256, 256), 64, 3, octaves=4, persistence=0.7, kind=kind)
        assert _fine_detail(faint) < _fine_detail(strong)

    @pytest.mark.parametrize("octaves", [1, 4])
    @pytest.mark.parametrize("seed", [0, 1, 2, 3])
    def test_no_artefact_at_the_lattice_spacing(self, kind, seed, octaves):
        # scale 32 is the lattice spacing of the coarsest octave. Measured on real fields the
        # anomaly is at most 0.002; the tiled known-bad field below reaches 0.02 and more.
        field = fractal_noise((256, 256), 32, seed, octaves=octaves, kind=kind)
        assert _lag_anomaly(field, 32) < 0.005

    def test_octaves_finer_than_a_pixel_are_ignored(self, kind):
        # At scale 8 the octaves have spacing 8, 4, 2, 1; the fifth would be half a pixel.
        capped = fractal_noise((64, 64), 8, 2, octaves=4, kind=kind)
        many = fractal_noise((64, 64), 8, 2, octaves=40, kind=kind)
        assert np.array_equal(capped, many)


@pytest.mark.parametrize("kind", KINDS)
def test_lag_anomaly_flags_a_field_that_repeats_at_the_lattice_spacing(kind):
    tile = fractal_noise((256, 32), 32, 1, octaves=4, kind=kind)
    tiled = np.tile(tile, (1, 8))
    assert _lag_anomaly(tiled, 32) > 0.015


def test_four_octaves_at_1600_square_take_under_two_seconds():
    start = time.perf_counter()
    field = fractal_noise((1600, 1600), 100, 1, octaves=4)
    elapsed = time.perf_counter() - start
    assert field.shape == (1600, 1600)
    assert elapsed < 2.0, f"took {elapsed:.2f}s"


# ---------------------------------------------------------------------------------------------------------------------
# worley
# ---------------------------------------------------------------------------------------------------------------------

METRICS = ["f1", "f2-f1"]


def _feature_points(f1: np.ndarray, cell_pixels: float) -> np.ndarray:
    """Pixel positions (row, column) of the feature points, found as the local minima of an f1 field.

    This reads the points back out of the picture alone, so tests of the cell structure do not depend on how
    `worley` places or stores them. f1 is measured in cell widths, and at a feature point it is 0 up to the half
    pixel (0.71 pixels diagonally) its position is rounded to; a larger minimum is a point just off the canvas.
    """
    is_minimum = (f1 == ndimage.minimum_filter(f1, size=3)) & (f1 < 0.75 / cell_pixels)
    return np.argwhere(is_minimum)


class TestWorleyContract:
    @pytest.mark.parametrize("metric", METRICS)
    def test_same_arguments_give_identical_arrays(self, metric):
        assert np.array_equal(worley((50, 70), 5, 9, metric=metric), worley((50, 70), 5, 9, metric=metric))

    @pytest.mark.parametrize("metric", METRICS)
    def test_different_seed_gives_an_unrelated_field(self, metric):
        a, b = worley((128, 128), 8, 1, metric=metric), worley((128, 128), 8, 2, metric=metric)
        assert abs(_correlation(a, b)) < 0.2

    @pytest.mark.parametrize("metric", METRICS)
    def test_float32_requested_shape_and_range_for_non_square(self, metric):
        field = worley((40, 90), 5, 3, metric=metric)
        assert field.dtype == np.float32
        assert field.shape == (40, 90)
        assert field.min() >= 0.0
        assert field.max() <= 1.0

    @pytest.mark.parametrize("metric", METRICS)
    def test_field_uses_a_good_part_of_the_range(self, metric):
        field = worley((256, 256), 8, 1, metric=metric)
        assert np.percentile(field, 99) > 0.5


class TestWorleyCells:
    def test_f1_is_zero_at_every_feature_point_and_grows_away_from_it(self):
        field = worley((256, 256), 8, 4)
        points = _feature_points(field, 32)
        assert len(points) > 50
        interior = [p for p in points if 8 <= p[0] < 248 and 8 <= p[1] < 248]
        assert len(interior) > 30
        for y, x in interior:
            near = np.mean([field[y + dy, x + dx] for dy, dx in ((2, 0), (-2, 0), (0, 2), (0, -2))])
            far = np.mean([field[y + dy, x + dx] for dy, dx in ((5, 0), (-5, 0), (0, 5), (0, -5))])
            assert field[y, x] < near < far
        assert np.median(field) > 5 * field[tuple(points.T)].max()

    def test_there_is_one_feature_point_per_cell(self):
        # 8 cells across a 256 pixel canvas: an 8 x 8 grid of 32 pixel cells, so 64 feature points.
        points = _feature_points(worley((256, 256), 8, 4), 32)
        # Within 2: a point just off the canvas edge can look like a minimum on the edge pixel.
        assert abs(len(points) - 64) <= 2

    def test_more_cells_means_more_and_smaller_cells(self):
        counts = [len(_feature_points(worley((256, 256), cells, 2), 256 / cells)) for cells in (4, 8, 16)]
        areas = [256 * 256 / count for count in counts]
        assert areas[0] > areas[1] > areas[2]
        for cells, count in zip((4, 8, 16), counts, strict=True):
            assert abs(count - cells**2) <= 2

    def test_cells_are_counted_across_the_longer_side(self):
        # 6 cells across 192 pixels: 32 pixel cells, so a 2 x 6 grid on a 64 x 192 canvas.
        points = _feature_points(worley((64, 192), 6, 5), 32)
        assert abs(len(points) - 12) <= 2
        tall = _feature_points(worley((192, 64), 6, 5), 32)
        assert abs(len(tall) - 12) <= 2

    def test_f2_minus_f1_is_small_on_cell_borders_and_large_at_centres(self):
        size, cells = 320, 10  # 32 pixel cells
        points = _feature_points(worley((size, size), cells, 6), 32)
        gap = worley((size, size), cells, 6, metric="f2-f1")

        # Independent Voronoi cells, from the feature points read back out of the f1 picture.
        rows, columns = np.mgrid[0:size, 0:size]
        pixels = np.stack([rows.ravel(), columns.ravel()], axis=1)
        owner = cKDTree(points).query(pixels)[1].reshape(size, size)
        border = np.zeros((size, size), dtype=bool)
        border[:, :-1] |= owner[:, :-1] != owner[:, 1:]
        border[:-1, :] |= owner[:-1, :] != owner[1:, :]
        # Away from the canvas edge, where cells whose point lies off the canvas are not in `points`.
        inside = np.zeros((size, size), dtype=bool)
        inside[64:-64, 64:-64] = True
        border &= inside

        centres = gap[tuple(points.T)]
        centres = centres[(points >= 64).all(axis=1) & (points < size - 64).all(axis=1)]
        assert border.sum() > 500
        assert len(centres) > 20
        assert np.median(centres) > 5 * np.median(gap[border])
        assert np.max(gap[border]) < np.min(centres)

    def test_f2_minus_f1_is_never_negative_and_is_not_f1(self):
        f1 = worley((128, 128), 6, 3)
        gap = worley((128, 128), 6, 3, metric="f2-f1")
        assert gap.min() >= 0.0
        assert not np.allclose(f1, gap)
