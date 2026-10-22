"""Properties of the seeded noise fields: determinism, range, crop consistency, feature size, no lattice artefacts."""

import time
import warnings

import numpy as np
import pytest
from scipy import ndimage
from scipy.spatial import cKDTree

from photo_fx.ops.noise import fractal_noise, worley

KINDS = ["gradient", "value"]

# Seeds that differ in the low bit, in high bits only (2**32 would collide with 0 if the seed were truncated to 32 bits)
# and in sign. Every pair of them must give an unrelated field.
SEEDS = [*range(8), 2**32, 2**32 + 1, -1, -2]


def _correlation(a: np.ndarray, b: np.ndarray) -> float:
    return float(np.corrcoef(a.ravel(), b.ravel())[0, 1])


def _assert_pairwise_unrelated(fields: list[np.ndarray]) -> None:
    """Pearson correlation of independent fields with ~1000 features is ~0.02 (measured: mean 0.016-0.028, worst
    pair 0.10 over 66 pairs), so the bounds sit well above that and far below the 1.0 of two equal fields."""
    correlations = [abs(_correlation(a, b)) for i, a in enumerate(fields) for b in fields[:i]]
    assert np.mean(correlations) < 0.05
    assert np.max(correlations) < 0.15


@pytest.mark.parametrize("kind", KINDS)
class TestFractalNoiseContract:
    def test_same_arguments_give_identical_arrays(self, kind):
        a = fractal_noise((64, 80), 16, 5, octaves=3, kind=kind)
        b = fractal_noise((64, 80), 16, 5, octaves=3, kind=kind)
        assert np.array_equal(a, b)

    def test_different_seeds_give_unrelated_fields(self, kind):
        fields = [fractal_noise((128, 128), 4, seed, kind=kind) for seed in SEEDS]
        _assert_pairwise_unrelated(fields)

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


@pytest.mark.parametrize("seed", range(40))
def test_gradient_noise_uses_most_of_the_range(seed):
    # The brief's bounds (1st percentile < 0.25, 99th > 0.75) are a statistical property, not a guarantee for every
    # seed: a 256x256 canvas holds only 4 x 4 features at scale 64, so the spread depends on the seed. Over seeds
    # 0-299 two seeds (49 and 227) miss the upper bound (99th percentile 0.749 and 0.746) and the worst 1st
    # percentile is 0.241. The test pins seeds 0-39, which all meet the bounds (worst 0.241 and 0.775), so it is
    # deterministic and checks that typical fields use the range; a change that shrank the range would move
    # many seeds below the bound at once.
    field = fractal_noise((256, 256), 64, seed)
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
        many = fractal_noise((64, 64), 8, 2, octaves=32, kind=kind)
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
    def test_different_seeds_give_unrelated_fields(self, metric):
        fields = [worley((128, 128), 16, seed, metric=metric) for seed in SEEDS]
        _assert_pairwise_unrelated(fields)

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


# ---------------------------------------------------------------------------------------------------------------------
# invalid input and edge cases
# ---------------------------------------------------------------------------------------------------------------------


class TestInvalidArguments:
    @pytest.mark.parametrize("scale", [0, -1, -0.5, float("nan"), float("inf")])
    def test_fractal_noise_rejects_a_scale_that_is_not_positive_and_finite(self, scale):
        with pytest.raises(ValueError, match="scale"):
            fractal_noise((8, 8), scale, 1)

    @pytest.mark.parametrize("octaves", [0, -1])
    def test_fractal_noise_rejects_fewer_than_one_octave(self, octaves):
        with pytest.raises(ValueError, match="octaves"):
            fractal_noise((8, 8), 4, 1, octaves=octaves)

    @pytest.mark.parametrize("persistence", [0, -0.5, float("nan")])
    def test_fractal_noise_rejects_a_persistence_that_is_not_positive(self, persistence):
        with pytest.raises(ValueError, match="persistence"):
            fractal_noise((8, 8), 4, 1, octaves=2, persistence=persistence)

    def test_fractal_noise_rejects_an_unknown_kind(self):
        with pytest.raises(ValueError, match="kind"):
            fractal_noise((8, 8), 4, 1, kind="simplex")  # type: ignore[arg-type]

    @pytest.mark.parametrize("cells", [0, -3, float("nan"), float("inf")])
    def test_worley_rejects_cells_that_are_not_positive_and_finite(self, cells):
        with pytest.raises(ValueError, match="cells"):
            worley((8, 8), cells, 1)

    def test_worley_rejects_an_unknown_metric(self):
        with pytest.raises(ValueError, match="metric"):
            worley((8, 8), 2, 1, metric="f3")  # type: ignore[arg-type]

    @pytest.mark.parametrize("shape", [(0, 5), (5, 0), (-3, 5), (5,), (4, 4, 3), (2.5, 4)])
    def test_both_reject_a_shape_that_is_not_two_positive_integers(self, shape):
        with pytest.raises(ValueError, match="shape"):
            fractal_noise(shape, 4, 1)
        with pytest.raises(ValueError, match="shape"):
            worley(shape, 2, 1)

    def test_valid_arguments_are_not_rejected(self):
        # The known-good side of every check above, with numpy integers for the shape.
        assert fractal_noise((np.int64(8), np.int32(9)), 4, 1, octaves=2, persistence=2.0, kind="value").shape == (8, 9)
        assert worley((8, 9), 0.5, 1, metric="f2-f1").shape == (8, 9)


class TestEdgeCases:
    def test_any_integer_seed_works_without_overflow_warnings_and_gives_distinct_fields(self):
        seeds = [0, 1, -1, 2**31, 2**63, 2**64 - 2]
        with warnings.catch_warnings():
            warnings.simplefilter("error")
            fields = [fractal_noise((32, 32), 8, seed, octaves=2) for seed in seeds]
            cells = [worley((32, 32), 4, seed) for seed in seeds]
        for group in (fields, cells):
            for i in range(len(seeds)):
                for j in range(i):
                    assert not np.array_equal(group[i], group[j])

    @pytest.mark.parametrize("kind", KINDS)
    @pytest.mark.parametrize("scale", [1e-9, 0.3, 1e9])
    def test_scales_outside_the_useful_range_still_give_a_valid_field(self, kind, scale):
        # Features smaller than a pixel behave as one pixel features; a feature far larger than the canvas is flat-ish.
        field = fractal_noise((24, 40), scale, 2, octaves=3, kind=kind)
        assert field.shape == (24, 40)
        assert field.dtype == np.float32
        assert 0.0 <= field.min() and field.max() <= 1.0

    @pytest.mark.parametrize("persistence", [0.01, 1.0, 5.0])
    def test_persistence_beyond_the_usual_range_stays_in_0_1(self, persistence):
        field = fractal_noise((64, 64), 16, 2, octaves=4, persistence=persistence)
        assert 0.0 <= field.min() and field.max() <= 1.0

    def test_the_most_octaves_allowed_stay_in_range(self):
        field = fractal_noise((200, 200), 1e6, 2, octaves=32)
        assert 0.0 <= field.min() and field.max() <= 1.0

    @pytest.mark.parametrize("lacunarity", [1.0, 2.0])
    def test_more_than_32_octaves_are_rejected(self, lacunarity):
        # With lacunarity 1 no octave ever falls under a pixel, so a cap is the only bound on the work.
        # 33 and 64 are small enough that, without the cap, the call returns quickly and this test fails instead of
        # hanging (a huge count would loop for ever and stall CI rather than fail it).
        for octaves in (33, 64):
            with pytest.raises(ValueError, match="octaves"):
                fractal_noise((8, 8), 4, 1, octaves=octaves, lacunarity=lacunarity)

    @pytest.mark.parametrize("shape", [(1, 1), (1, 50), (50, 1)])
    def test_single_row_column_or_pixel_canvases(self, shape):
        for field in (fractal_noise(shape, 8, 3, octaves=2), worley(shape, 3, 3), worley(shape, 3, 3, metric="f2-f1")):
            assert field.shape == shape
            assert 0.0 <= field.min() and field.max() <= 1.0

    @pytest.mark.parametrize("cells", [0.2, 1e7])
    def test_worley_with_one_giant_cell_or_more_cells_than_pixels(self, cells):
        for metric in METRICS:
            field = worley((30, 50), cells, 4, metric=metric)
            assert field.shape == (30, 50)
            assert 0.0 <= field.min() and field.max() <= 1.0


# ---------------------------------------------------------------------------------------------------------------------
# octave structure and smoothness
# ---------------------------------------------------------------------------------------------------------------------


def _relative_roughness(field: np.ndarray) -> float:
    """Mean neighbour difference in units of the field's own spread: proportional to 1 / feature size."""
    return _roughness(field) / float(field.std())


def _mean_relative_roughness(**arguments) -> float:
    return float(
        np.mean([_relative_roughness(fractal_noise((256, 256), seed=seed, **arguments)) for seed in range(12)])
    )


@pytest.mark.parametrize("kind", KINDS)
class TestOctaveFeatureSize:
    # `persistence` of 1e6 makes octave 1 carry a million times the weight of octave 0, so the two-octave field is
    # octave 1 alone. Its roughness must then match a single octave whose scale is the expected feature size.
    # Roughness of a lattice field is inversely proportional to the lattice spacing: scale 32 and 64 measure
    # 0.075 and 0.039 (gradient), so a wrong factor (3 instead of 2 gives a spacing of 21, not 32) is off by 50%.

    def test_second_octave_has_half_the_feature_size(self, kind):
        second = _mean_relative_roughness(scale=64, octaves=2, persistence=1e6, kind=kind)
        half_size = _mean_relative_roughness(scale=32, kind=kind)
        assert second == pytest.approx(half_size, rel=0.1)

    def test_third_octave_has_a_quarter_of_the_feature_size(self, kind):
        third = _mean_relative_roughness(scale=128, octaves=3, persistence=1e6, kind=kind)
        quarter_size = _mean_relative_roughness(scale=32, kind=kind)
        assert third == pytest.approx(quarter_size, rel=0.1)

    def test_lacunarity_sets_the_ratio_between_octaves(self, kind):
        # Lacunarity 3: octave 1 has a third of the scale, so scale 96 gives 32.
        second = _mean_relative_roughness(scale=96, octaves=2, persistence=1e6, lacunarity=3.0, kind=kind)
        third_size = _mean_relative_roughness(scale=32, kind=kind)
        assert second == pytest.approx(third_size, rel=0.1)

    def test_lacunarity_defaults_to_two(self, kind):
        default = fractal_noise((48, 48), 24, 5, octaves=3, kind=kind)
        explicit = fractal_noise((48, 48), 24, 5, octaves=3, lacunarity=2.0, kind=kind)
        assert np.array_equal(default, explicit)

    def test_fractional_lacunarity_is_respected(self, kind):
        # 1.5: octave 1 has features 2/3 the size, so scale 48 matches scale 32.
        second = _mean_relative_roughness(scale=48, octaves=2, persistence=1e6, lacunarity=1.5, kind=kind)
        assert second == pytest.approx(_mean_relative_roughness(scale=32, kind=kind), rel=0.1)

    def test_octaves_are_independent_of_each_other(self, kind):
        # Octave 0 alone and octave 1 alone (by extreme persistence) must be unrelated fields even though they come
        # from one seed: octaves that shared their randomness would reinforce each other into visible patterns.
        # Lacunarity 1 gives both octaves the same feature size, where shared randomness would make them identical
        # (at other sizes shared hashes happen to land on different lattice points and hide the fault). Scale 4 gives
        # ~1000 features, so the correlation of independent fields is ~0.02 whatever the interpolation curve: over
        # 800 seeds the mean |r| was about 0.016 (gradient) and 0.024 (value), the worst 40-seed mean 0.030 and the
        # worst pair 0.099. The bound is twice the worst mean; shared randomness gives 1.0.
        correlations = []
        for seed in range(40):
            coarse = fractal_noise((128, 128), 4, seed, octaves=2, persistence=1e-6, lacunarity=1, kind=kind)
            fine = fractal_noise((128, 128), 4, seed, octaves=2, persistence=1e6, lacunarity=1, kind=kind)
            correlations.append(abs(_correlation(coarse, fine)))
        assert np.mean(correlations) < 0.06


def _crease_ratio(field: np.ndarray, axis: int) -> float:
    """Largest third difference along `axis` relative to its root mean square.

    The module promises fields without creases: the interpolation between lattice points has a continuous first and
    second derivative. A crease (jump in the first derivative, as in linear interpolation) or a kink (jump in the
    second derivative, as in smoothstep) shows as a spike in the third difference on every lattice line, whereas
    a smooth field has a third difference without outliers. Using a ratio makes the statistic independent of the
    field's amplitude, and it needs no knowledge of where the lattice lines fall. Measured at scale 32: 3.2-5.5 for
    smooth fields, 8.5-14.9 for linear and smoothstep interpolation, and for either applied along one axis only.
    """
    third = np.diff(field.astype(np.float64), n=3, axis=axis)
    return float(np.abs(third).max() / np.sqrt((third**2).mean()))


@pytest.mark.parametrize("kind", KINDS)
@pytest.mark.parametrize("seed", [0, 1, 2])
@pytest.mark.parametrize("axis", [0, 1])
def test_fields_have_no_creases_along_either_axis(kind, seed, axis):
    field = fractal_noise((200, 300), 32, seed, kind=kind)
    assert _crease_ratio(field, axis) < 7.0


def test_crease_ratio_flags_linear_interpolation_of_a_lattice():
    lattice = np.random.default_rng(0).random((9, 12))
    creased = ndimage.zoom(lattice, 32, order=1)
    smooth = ndimage.zoom(lattice, 32, order=3)
    for axis in (0, 1):
        assert _crease_ratio(creased, axis) > 8.0
        assert _crease_ratio(smooth, axis) < 7.0


class TestArgumentTypes:
    @pytest.mark.parametrize("lacunarity", [0.99, 0, -2.0, float("nan"), float("inf")])
    def test_lacunarity_must_be_at_least_one_and_finite(self, lacunarity):
        with pytest.raises(ValueError, match="lacunarity"):
            fractal_noise((8, 8), 4, 1, octaves=2, lacunarity=lacunarity)

    def test_lacunarity_of_exactly_one_is_allowed(self):
        assert fractal_noise((8, 8), 4, 1, octaves=2, lacunarity=1).shape == (8, 8)

    @pytest.mark.parametrize("octaves", [2.5, 2.0, True, "3"])
    def test_octaves_must_be_an_integer(self, octaves):
        with pytest.raises(ValueError, match="octaves"):
            fractal_noise((8, 8), 4, 1, octaves=octaves)  # type: ignore[arg-type]

    @pytest.mark.parametrize("bad", [True, "8", None])
    def test_scale_and_cells_must_be_numbers(self, bad):
        with pytest.raises(ValueError, match="scale"):
            fractal_noise((8, 8), bad, 1)  # type: ignore[arg-type]
        with pytest.raises(ValueError, match="cells"):
            worley((8, 8), bad, 1)  # type: ignore[arg-type]


def test_extreme_scale_and_lacunarity_do_not_overflow():
    # lacunarity ** octave alone would exceed the float range at octave 2.
    field = fractal_noise((8, 8), 1e308, 1, octaves=3, lacunarity=1e300)
    assert field.shape == (8, 8)
    assert 0.0 <= field.min() and field.max() <= 1.0


class TestExtremePersistence:
    # Weights are persistence ** octave, which leaves the float range for large persistence: the documented
    # promise (finite, in [0, 1]) must hold for every accepted value, and 1e6 is what the tests above use.

    @pytest.mark.parametrize("kind", KINDS)
    @pytest.mark.parametrize("persistence", [1e-300, 1e-6, 1e6, 1e300, 1e308])
    def test_field_is_finite_and_in_range_with_the_most_octaves(self, kind, persistence):
        with warnings.catch_warnings():
            warnings.simplefilter("error")
            field = fractal_noise((24, 40), 1e9, 3, octaves=32, persistence=persistence, kind=kind)
        assert np.isfinite(field).all()
        assert field.min() >= 0.0
        assert field.max() <= 1.0

    def test_a_dominant_last_octave_still_looks_like_that_octave(self):
        # With persistence 1e300 the finest octave carries the whole field, as with 1e6 (the field is finite, not
        # a NaN or a constant): its roughness matches a single octave at the finest scale.
        dominant = fractal_noise((128, 128), 64, 4, octaves=3, persistence=1e300)
        reference = fractal_noise((128, 128), 16, 4)
        assert _relative_roughness(dominant) == pytest.approx(_relative_roughness(reference), rel=0.35)
