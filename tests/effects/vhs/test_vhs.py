"""Behaviour of the `vhs` effect: tracking bands, chroma bleed, tape noise and wobble of a worn videotape."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "vhs"
OFF = {"tracking": 0.0, "noise": 0.0, "wobble": 0.0, "bleed": 0.0}  # every damage off: the picture as it was
# The FCC YIQ matrix as published (47 CFR 73.682), typed here as the oracle's own copy.
YIQ = np.array([[0.299, 0.587, 0.114], [0.5959, -0.2746, -0.3213], [0.2115, -0.5227, 0.3112]])


def to_yiq(rgb: np.ndarray) -> np.ndarray:
    return rgb @ YIQ.T


def to_rgb(yiq: np.ndarray) -> np.ndarray:
    return yiq @ np.linalg.inv(YIQ).T


def runs(rows: np.ndarray) -> list[np.ndarray]:
    """`rows` (sorted indices) cut into runs of consecutive indices."""
    return np.split(rows, np.flatnonzero(np.diff(rows) > 1) + 1)


def rolled_by(source: np.ndarray, shifted: np.ndarray, atol: float = 1e-5) -> list[int]:
    """The whole-pixel rolls that turn the row `source` into `shifted` (more than one only if the row repeats)."""
    return [s for s in range(len(source)) if np.allclose(np.roll(source, s, axis=0), shifted, atol=atol)]


def bled(chroma: np.ndarray, bleed: int, sigma: float, truncate: float) -> np.ndarray:
    """Brute force: each row of `chroma` (height, width) rolled right by `bleed` px, then convolved with a Gaussian of
    standard deviation `sigma` cut at `truncate` sigmas, the picture taken as repeating at its sides."""
    radius = int(truncate * sigma + 0.5)
    offsets = np.arange(-radius, radius + 1)
    kernel = np.exp(-0.5 * (offsets / sigma) ** 2)
    kernel /= kernel.sum()
    padded = np.pad(np.roll(chroma, bleed, axis=1), ((0, 0), (radius, radius)), mode="wrap")
    return np.array([[row[c : c + len(kernel)] @ kernel for c in range(chroma.shape[1])] for row in padded])


@pytest.fixture
def mid() -> np.ndarray:
    """A noisy colour picture well inside 0..1, so noise and colour changes are never clamped."""
    rng = np.random.default_rng(3)
    return (0.3 + 0.4 * rng.random((48, 64, 3))).astype(np.float32)


def test_every_damage_off_leaves_the_picture_as_it_is(photo) -> None:
    np.testing.assert_allclose(registry.apply(NAME, photo, **OFF), photo, atol=1e-5)


@pytest.mark.parametrize(("bleed", "truncate"), [(2, 4.0), (3, 2.0)])
def test_with_no_tracking_noise_or_wobble_the_colour_trails_the_brightness_by_the_bleed_and_is_smeared(
    mid, bleed, truncate
) -> None:
    """Brightness stays; the two colour axes are each moved `bleed` px right and blurred by a Gaussian of sigma
    `bleed`: worked out here by hand, row by row."""
    out = to_yiq(registry.apply(NAME, mid, **{**OFF, "bleed": bleed}, truncate=truncate, edge="wrap"))
    before = to_yiq(mid)
    np.testing.assert_allclose(out[..., 0], before[..., 0], atol=1e-5)
    for channel in (1, 2):
        np.testing.assert_allclose(out[..., channel], bled(before[..., channel], bleed, bleed, truncate), atol=1e-5)


@pytest.mark.parametrize("seed", [0, 1, 2, 3])
def test_a_tracking_band_slides_whole_rows_sideways_and_rows_outside_every_band_are_unchanged(photo, seed) -> None:
    out = registry.apply(NAME, photo, **{**OFF, "tracking": 0.2}, bands=3, seed=seed, edge="wrap")
    changed = np.flatnonzero(~np.isclose(out, photo, atol=1e-5).all(axis=(1, 2)))
    assert 1 <= len(runs(changed)) <= 3
    for run in runs(changed):
        rolls = [set(rolled_by(photo[row], out[row])) for row in run]
        assert set.intersection(*rolls), "the rows of one band do not slide by the same whole number of pixels"
        slide = min(set.intersection(*rolls), key=lambda s: min(s, 128 - s))
        assert min(slide, 128 - slide) <= round(0.2 * 128)


@pytest.mark.parametrize("settings", [{"bands": 0, "tracking": 0.3}, {"bands": 5, "tracking": 0.0}])
def test_no_bands_or_no_tracking_leaves_every_row_where_it_was(photo, settings) -> None:
    out = registry.apply(NAME, photo, **{**OFF, **settings})
    np.testing.assert_allclose(out, photo, atol=1e-5)


def test_tape_noise_changes_brightness_by_the_asked_deviation_and_leaves_colour_alone(mid) -> None:
    out = registry.apply(NAME, mid, **{**OFF, "noise": 0.05}, seed=2)
    before, after = to_yiq(mid), to_yiq(out)
    np.testing.assert_allclose(after[..., 1:], before[..., 1:], atol=1e-5)
    np.testing.assert_allclose((after[..., 0] - before[..., 0]).std(), 0.05, rtol=0.15)
    np.testing.assert_allclose((after[..., 0] - before[..., 0]).mean(), 0, atol=0.01)


def test_wobble_slides_each_row_sideways_by_a_little_so_a_row_keeps_its_total_and_the_picture_changes(photo) -> None:
    out = registry.apply(NAME, photo, **{**OFF, "wobble": 3.0}, edge="wrap", seed=1)
    assert not np.allclose(out, photo, atol=1e-3)
    np.testing.assert_allclose(out.sum(axis=1), photo.sum(axis=1), atol=1e-3)  # a wrapped slide keeps each row's total


def test_a_larger_wobble_moves_the_rows_further(photo) -> None:
    small = np.abs(registry.apply(NAME, photo, **{**OFF, "wobble": 0.5}, seed=1) - photo).mean()
    large = np.abs(registry.apply(NAME, photo, **{**OFF, "wobble": 6.0}, seed=1) - photo).mean()
    assert 0 < small < large


def test_tracking_bands_slide_both_ways_and_can_reach_the_first_and_the_last_row(photo) -> None:
    """Over many seeds, with tall bands: some slide left and some right, and between them they cover every row."""
    covered = np.zeros(photo.shape[0], bool)
    slides = []
    for seed in range(40):
        out = registry.apply(NAME, photo, **{**OFF, "tracking": 0.3}, bands=4, band_height=0.5, seed=seed, edge="wrap")
        changed = ~np.isclose(out, photo, atol=1e-5).all(axis=(1, 2))
        covered |= changed
        for run in runs(np.flatnonzero(changed)):
            rolls = rolled_by(photo[run[0]], out[run[0]])
            slides += [r if r <= 64 else r - 128 for r in rolls]
    assert covered.all()
    assert min(slides) < 0 < max(slides)


@pytest.mark.parametrize(("wobble", "size", "smooth"), [(2.0, 12.0, True), (4.0, 12.0, True), (2.0, 1.0, False)])
def test_wobble_is_a_drift_of_the_asked_deviation_that_is_smooth_over_the_asked_number_of_rows(
    wobble, size, smooth
) -> None:
    """Rows of a pure ramp (value x / 127) slid by d read x / 127 - d / 127 away from the edges, so each row's slide is
    found from its own values. Their standard deviation over the rows is `wobble`; neighbouring rows slide alike when
    `wobble_size` spans many rows and independently when it is about a pixel."""
    ramp = np.broadcast_to((np.arange(128) / 127)[None, :, None], (200, 128, 3)).astype(np.float32)
    out = registry.apply(NAME, ramp, **{**OFF, "wobble": wobble}, wobble_size=size, edge="nearest", seed=3)
    slides = np.array([np.median(np.arange(30, 98) - 127 * row[30:98, 0]) for row in out])
    np.testing.assert_allclose(slides.std(), wobble, rtol=0.05 if smooth else 0.25)
    correlation = np.corrcoef(slides[:-1], slides[1:])[0, 1]
    assert (correlation > 0.8) == smooth
