import numpy as np
import pytest

from photo_fx.checks import MotionMeter, matte_warnings, still_warnings


def disc(share, size=100):
    """A matte whose subject covers roughly `share` of the picture."""
    y, x = np.mgrid[0:size, 0:size]
    radius = np.sqrt(share * size * size / np.pi)
    return (np.hypot(x - size / 2, y - size / 2) <= radius).astype(np.float32)


@pytest.mark.parametrize("share, expect", [(0.005, "only"), (0.3, None), (0.99, "almost no background")])
def test_matte_warnings_flag_a_cutout_that_missed_the_subject_or_took_everything(share, expect):
    warnings = matte_warnings(disc(share) if share < 0.9 else np.ones((100, 100), np.float32))
    assert (expect is None and not warnings) or (expect in warnings[0])


def test_matte_share_limits_can_be_changed():
    assert matte_warnings(disc(0.3), share_range=(0.5, 0.9))  # 30% is now too small


def test_still_warnings(photo):
    assert still_warnings(photo) == []
    assert "blank" in still_warnings(np.zeros((8, 8, 3), np.float32))[0]
    assert "white" in still_warnings(np.ones((8, 8, 3), np.float32))[0]
    assert "flat" in still_warnings(np.full((8, 8, 3), 0.5, np.float32))[0]
    assert "invalid" in still_warnings(np.full((8, 8, 3), np.nan, np.float32))[0]


def meter_with(energies):
    meter = MotionMeter()
    meter.energy = list(energies)
    return meter


def test_a_clip_that_keeps_moving_has_no_frozen_tail():
    steady = meter_with(np.linspace(1.0, 0.5, 60))
    assert steady.frozen_share() == 0.0 and steady.warnings() == []


def test_a_clip_whose_motion_stops_early_is_flagged():
    meter = meter_with([1.0] * 40 + [0.001] * 60)  # moves for 40%, then nothing but a faint shimmer
    assert meter.frozen_share() == pytest.approx(0.6)
    assert "motion stops early" in meter.warnings(frozen_tail=0.3)[0] and "60%" in meter.warnings(0.3)[0]
    assert meter.warnings(frozen_tail=0.7) == []  # unless that much stillness is expected


def test_a_clip_where_nothing_moves_is_flagged_and_very_short_clips_are_not_judged():
    assert "nothing moves" in meter_with([0.0] * 30).warnings()[0]
    assert meter_with([1.0, 0.0, 0.0]).warnings() == []


def test_the_meter_measures_real_frames():
    meter = MotionMeter()
    base = np.zeros((64, 64, 3), np.float32)
    for i in range(30):
        frame = base.copy()
        frame[:, : i * 2] = 1.0  # a wipe that stops after 15 frames
        meter.add(frame if i < 15 else np.ones((64, 64, 3), np.float32) * 0 + frame[:, :1].mean())
    assert meter.shape == (64, 64) and len(meter.energy) == 29
