"""Behaviour of the `long_shadow` effect."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "long_shadow"
GREY = 0.8
SIZE = 41
CENTRE = 20


def grey(height: int = SIZE, width: int = SIZE) -> np.ndarray:
    return np.full((height, width, 3), GREY, np.float32)


def one_pixel(row: int = CENTRE, col: int = CENTRE, size: int = SIZE) -> np.ndarray:
    matte = np.zeros((size, size), np.float32)
    matte[row, col] = 1.0
    return matte


def solid(**kwargs: object) -> dict[str, object]:
    """A black shadow that does not fade and is fully opaque: every shadow pixel comes out black."""
    return {"color": "#000000", "opacity": 1.0, "fade": 0.0} | kwargs


@pytest.mark.parametrize(
    ("angle", "length", "step", "copies"),
    [
        pytest.param(0, 6, (0, 1), 6, id="0 degrees is to the right, one copy per pixel up to the length"),
        pytest.param(90, 6, (1, 0), 6, id="90 degrees is down"),
        pytest.param(180, 6, (0, -1), 6, id="180 degrees is to the left"),
        pytest.param(-90, 6, (-1, 0), 6, id="-90 degrees is up"),
        pytest.param(
            45, 10, (1, 1), 7, id="45 degrees is the lower right; 7 diagonal steps are 9.9 px, 8 would be 11.3"
        ),
        pytest.param(-135, 10, (-1, -1), 7, id="-135 degrees is the upper left"),
        pytest.param(45, 11, (1, 1), 7, id="an 8th diagonal copy would be 11.3 px out, past the length of 11"),
    ],
)
def test_the_shadow_is_the_subject_repeated_along_the_angle_up_to_the_length(
    angle: float, length: float, step: tuple[int, int], copies: int
) -> None:
    out = registry.apply(NAME, grey(), matte=one_pixel(), angle=angle, length=length, **solid())
    expected = grey()
    for k in range(1, copies + 1):  # the copy 0 is the subject itself
        expected[CENTRE + k * step[0], CENTRE + k * step[1]] = 0.0
    np.testing.assert_allclose(out, expected, atol=1e-6)


@pytest.mark.parametrize(
    ("fade", "expected_coverage"),
    [
        pytest.param(0.0, [1.0, 1.0, 1.0, 1.0], id="fade 0 is solid to the end"),
        pytest.param(1.0, [0.75, 0.5, 0.25, 0.0], id="fade 1 falls linearly to nothing at the length"),
        pytest.param(0.5, [0.875, 0.75, 0.625, 0.5], id="fade 0.5 falls to half"),
    ],
)
def test_fade_lowers_the_shadow_in_a_straight_line_to_the_end(fade: float, expected_coverage: list[float]) -> None:
    """Copies 5, 10, 15 and 20 px out of a length of 20: a shadow pixel keeps `GREY x (1 - coverage)`."""
    out = registry.apply(NAME, grey(), matte=one_pixel(col=2), angle=0, length=20, **solid(fade=fade))
    seen = [GREY - out[CENTRE, 2 + d, 0] for d in (5, 10, 15, 20)]
    np.testing.assert_allclose(seen, [GREY * c for c in expected_coverage], atol=1e-6)


def test_fade_follows_the_distance_along_the_ray_not_the_number_of_copies() -> None:
    """On the diagonal a copy is 1.414 px from the last, so the 7th is 9.9 px out of 20 and keeps 1 - 9.9/20."""
    out = registry.apply(NAME, grey(), matte=one_pixel(2, 2), angle=45, length=20, **solid(fade=1.0))
    assert (GREY - out[2 + 7, 2 + 7, 0]) / GREY == pytest.approx(1 - 7 * np.sqrt(2) / 20, abs=1e-6)


def test_a_slanted_shadow_runs_along_the_angle_and_stops_at_the_length() -> None:
    """A 7 px block cast at 30 degrees below the right: its centre is covered all along the ray, and nothing lies
    beyond the length (here 30 px: 25 px across and 15 px down)."""
    matte = np.zeros((60, 100), np.float32)
    matte[3:10, 3:10] = 1.0
    out = registry.apply(NAME, grey(60, 100), matte=matte, angle=30, length=30, **solid())
    covered = (GREY - out[..., 0]) / GREY
    for across in range(8, 26):  # the centre of the block is at (6, 6); one px across moves tan 30 = 0.577 px down
        assert covered[round(6 + across * np.tan(np.radians(30))), 6 + across] > 0.99
    rows, cols = np.nonzero(covered > 0.5)
    assert cols.max() <= 9 + 25 + 1  # the block's last column, moved by the last copy (25 px across), and a pixel
    assert rows.max() <= 9 + 15 + 1  # its last row, moved by 15 px down, and a pixel


def test_no_opacity_leaves_the_picture_as_it_was(photo: np.ndarray, matte: np.ndarray) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo, matte=matte, opacity=0.0), photo)


def test_length_zero_casts_nothing_beyond_the_subject_itself(photo: np.ndarray, matte: np.ndarray) -> None:
    """The silhouette is its own first copy, hidden behind the subject; only where the matte is soft does it leave a
    faint fringe, so the identity is exact for a cut-out with a hard edge."""
    hard = (matte > 0.5).astype(np.float32)
    np.testing.assert_array_equal(registry.apply(NAME, photo, matte=hard, length=0.0), photo)


def test_the_subject_is_never_changed(photo: np.ndarray, matte: np.ndarray) -> None:
    for angle in (0, 45, 100, -170):
        out = registry.apply(NAME, photo, matte=matte, angle=angle, length=40, opacity=1.0, color="#ff0000")
        np.testing.assert_array_equal(out[matte == 1], photo[matte == 1])


def test_opacity_and_colour_mix_the_shadow_into_the_picture() -> None:
    out = registry.apply(
        NAME, grey(), matte=one_pixel(), angle=0, length=5, **solid(opacity=0.5, color="#ff0000", fade=0.0)
    )
    np.testing.assert_allclose(out[CENTRE, CENTRE + 3], [0.9, 0.4, 0.4], atol=1e-6)


def test_a_subject_cut_by_the_border_casts_the_shadow_of_the_part_beyond_it() -> None:
    """The subject fills rows 2 to 4 of the six left columns, so it goes on beyond the border. Cast to the lower right,
    the picture must show what a picture 10 px wider on the left, with the subject carried on into it, shows."""
    matte = np.zeros((14, 12), np.float32)
    matte[2:5, :6] = 1.0
    wide = np.zeros((14, 22), np.float32)
    wide[2:5, :16] = 1.0
    out = registry.apply(NAME, grey(14, 12), matte=matte, angle=45, length=14, **solid())
    out_wide = registry.apply(NAME, grey(14, 22), matte=wide, angle=45, length=14, **solid())
    np.testing.assert_allclose(out, out_wide[:, 10:], atol=1e-6)
    assert out[10, 0, 0] == 0.0  # in the shadow of the part that is out of the picture
    np.testing.assert_array_equal(out[:2], grey(14, 12)[:2])  # above the subject, where nothing casts a shadow


def test_without_a_subject_there_is_no_shadow(photo: np.ndarray) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo), photo)
