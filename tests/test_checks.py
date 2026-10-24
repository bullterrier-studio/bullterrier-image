import numpy as np

from bullterrier_image.checks import matte_warnings, still_warnings


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
    assert "only" in warning
    assert "0.5%" in warning


def test_a_cutout_that_took_everything_is_flagged():
    (warning,) = matte_warnings(np.ones((100, 100), np.float32))
    assert "almost no background" in warning
    assert "100.0%" in warning


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


def test_still_warnings_judge_the_colour_of_a_picture_with_transparency_not_its_alpha(photo):
    clear = np.dstack([photo, np.zeros(photo.shape[:2], np.float32)])  # good colour, nothing opaque
    assert still_warnings(clear) == []
    black = np.dstack([np.zeros_like(photo), np.ones(photo.shape[:2], np.float32)])  # bad colour, fully opaque
    assert "blank" in still_warnings(black)[0]
