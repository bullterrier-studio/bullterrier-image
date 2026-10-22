import numpy as np
import pytest

from photo_fx.checks import MotionMeter, matte_warnings, still_warnings


def disc(share, size=100):
    """A matte whose subject covers roughly `share` of the picture."""
    y, x = np.mgrid[0:size, 0:size]
    radius = np.sqrt(share * size * size / np.pi)
    return (np.hypot(x - size / 2, y - size / 2) <= radius).astype(np.float32)


def test_a_plausible_cutout_raises_no_warning():
    assert matte_warnings(disc(0.3)) == []


def test_a_cutout_that_missed_the_subject_is_flagged_with_its_share():
    matte = np.zeros((100, 100), np.float32)
    matte.flat[:50] = 1.0  # exactly 0.5% of the picture
    (warning,) = matte_warnings(matte)
    assert "only" in warning and "0.5%" in warning


def test_a_cutout_that_took_everything_is_flagged():
    (warning,) = matte_warnings(np.ones((100, 100), np.float32))
    assert "almost no background" in warning and "100.0%" in warning


def test_matte_share_limits_can_be_changed():
    assert matte_warnings(disc(0.3)) == []  # fine with the default limits...
    (warning,) = matte_warnings(disc(0.3), share_range=(0.5, 0.9))  # ...but 30% is too small for these
    assert "only" in warning
    (warning,) = matte_warnings(disc(0.3), share_range=(0.05, 0.2))  # ...and too large for these
    assert "almost no background" in warning


def test_still_warnings(photo):
    assert still_warnings(photo) == []
    assert "blank" in still_warnings(np.zeros((8, 8, 3), np.float32))[0]
    assert "white" in still_warnings(np.ones((8, 8, 3), np.float32))[0]
    assert "flat" in still_warnings(np.full((8, 8, 3), 0.5, np.float32))[0]
    assert "invalid" in still_warnings(np.full((8, 8, 3), np.nan, np.float32))[0]


def meter_fed(changes, shape=(24, 32)) -> MotionMeter:
    """A meter that has seen a clip of flat grey frames whose brightness changes by each of `changes` in turn (up and
    down alternately, so it stays in range): the mean change between frames is exactly that amount."""
    meter, level = MotionMeter(), 0.0
    meter.add(np.full((*shape, 3), level, np.float32))
    for i, change in enumerate(changes):
        level += change if i % 2 == 0 else -change
        meter.add(np.full((*shape, 3), level, np.float32))
    return meter


def test_a_clip_that_keeps_moving_has_no_frozen_tail():
    steady = meter_fed(np.linspace(0.5, 0.25, 60))
    assert steady.frozen_share() == 0.0 and steady.warnings() == []


def test_a_clip_whose_motion_stops_early_is_flagged():
    meter = meter_fed([0.5] * 40 + [0.0005] * 60)  # moves for 40%, then nothing but a faint shimmer
    assert meter.frozen_share() == pytest.approx(0.6)
    (warning,) = meter.warnings(frozen_tail=0.3)
    assert "motion stops early" in warning and "60%" in warning
    assert meter.warnings(frozen_tail=0.7) == []  # unless that much stillness is expected


def test_a_clip_where_nothing_moves_is_flagged_and_very_short_clips_are_not_judged():
    assert "nothing moves" in meter_fed([0.0] * 30).warnings()[0]
    assert meter_fed([0.5, 0.0, 0.0]).warnings() == []


def test_the_meter_reports_the_frame_size_it_saw():
    assert meter_fed([0.5] * 3, shape=(48, 64)).shape == (48, 64)


def test_a_wipe_that_stops_halfway_through_the_clip_is_caught():
    meter = MotionMeter()
    frame = np.zeros((64, 64, 3), np.float32)
    for i in range(32):
        frame[:, : 4 * min(i, 15)] = 1.0  # a wipe that moves 4 columns a frame for 15 frames, then stands still
        meter.add(frame.copy())
    assert meter.frozen_share() == pytest.approx(16 / 31)  # 31 changes between frames; the last 16 are none
    assert "motion stops early" in meter.warnings()[0]
