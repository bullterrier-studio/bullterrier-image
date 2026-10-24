"""Behaviour of the `evaporate` effect."""

import numpy as np
import pytest

from photo_fx.core import registry
from photo_fx.ops.glyphs import cell_size


def test_evaporate_leaves_everything_below_start_untouched():
    lit = np.ones((96, 128, 3), np.float32)
    out = registry.apply("evaporate", lit, cell=5, start=0.5, ramp=0.3)
    assert not np.array_equal(out, lit)
    first_safe_row = 48 + cell_size(5)[1]  # pieces are whole character rows, so the row straddling `start` may change
    np.testing.assert_array_equal(out[first_safe_row:], lit[first_safe_row:])


def test_evaporate_thins_out_the_top_more_than_the_bottom():
    lit = np.ones((96, 128, 3), np.float32)
    out = registry.apply("evaporate", lit, cell=5, start=0.8, ramp=0.5)
    assert out[:32].mean() < out[64:].mean()
    assert out[64:].mean() > 0.95
    assert out[:20].mean() < 0.5


def test_evaporate_only_moves_pieces_up_and_never_adds_light():
    upper_half = np.zeros((96, 128, 3), np.float32)
    upper_half[:48] = 0.8
    out = registry.apply("evaporate", upper_half, cell=5, start=0.9, ramp=0.5, rise=0.3)
    assert out[48 + cell_size(5)[1] :].max() == 0.0  # nothing sank into the dark lower half
    assert out.max() <= upper_half.max() + 1e-6


def test_evaporate_vanish_controls_how_many_pieces_survive_the_trip():
    lit = np.ones((120, 128, 3), np.float32)
    kwargs = {"cell": 5, "start": 0.9, "ramp": 0.4, "rise": 0.4, "seed": 2}
    keep_all = registry.apply("evaporate", lit, vanish=0.0, **kwargs)
    some = registry.apply("evaporate", lit, vanish=0.5, **kwargs)
    none = registry.apply("evaporate", lit, vanish=1.0, **kwargs)
    assert keep_all.sum() > some.sum() > none.sum()


def _block():
    """A lit 80x140 rectangle on black, standing on the bottom edge of a 200x200 picture."""
    img = np.zeros((200, 200, 3), np.float32)
    img[60:, 60:140] = 1.0
    return img


def test_evaporate_edge_sheds_the_outline_and_keeps_the_inside():
    out = registry.apply("evaporate", _block(), cell=5, start=0, outline=0.1, outward=0.5, seed=1)
    assert (
        out[150:190, 90:110].min() == 1.0
    )  # deep inside, more than `outline` from any edge, and the bottom is no edge
    assert not np.array_equal(out, _block())
    # pieces flew out of both sides
    assert out[:, :55].max() > 0
    assert out[:, 145:].max() > 0


def test_evaporate_outward_sends_pieces_away_from_the_edge_they_came_from():
    sides = lambda img: img[:, :55].sum() + img[:, 145:].sum()  # noqa: E731  (a one-line helper)
    kwargs = {"cell": 5, "start": 0, "outline": 0.1, "seed": 1}
    assert sides(registry.apply("evaporate", _block(), outward=0.8, **kwargs)) > sides(
        registry.apply("evaporate", _block(), outward=0.0, **kwargs)
    )


def test_evaporate_wind_makes_the_outward_drift_lopsided():
    left_and_right = lambda img: (img[:, :55].sum(), img[:, 145:].sum())  # noqa: E731  (a one-line helper)
    out = registry.apply("evaporate", _block(), cell=5, start=0, outline=0.1, outward=0.4, wind=3.0)
    left, right = left_and_right(out)
    assert right > left


def test_evaporate_treats_the_left_right_and_top_borders_as_edges_but_not_the_bottom():
    lit = np.ones((100, 120, 3), np.float32)
    out = registry.apply("evaporate", lit, cell=5, start=0, outline=0.2, seed=2)
    assert out[:5, 50:70].mean() < 1.0
    assert out[40:80, :5].mean() < 1.0
    assert out[40:80, -5:].mean() < 1.0
    assert out[-10:, 50:70].min() == 1.0  # bottom centre: far from the other borders, and the bottom is no edge


def test_evaporate_without_edge_or_outward_ignores_the_outline():
    lit = np.zeros((100, 120, 3), np.float32)
    lit[50:, 30:90] = 1.0  # lit only below `start`
    out = registry.apply("evaporate", lit, cell=5, start=0.4, ramp=0.3)
    first_safe_row = 40 + cell_size(5)[1]
    np.testing.assert_array_equal(out[first_safe_row:], lit[first_safe_row:])


def _figure_with_a_dark_gap():
    """A lit rectangle with a black 'eye' inside it, and the matte of the whole rectangle."""
    img = _block()
    img[110:130, 90:110] = 0.0
    matte = np.zeros((200, 200), np.float32)
    matte[60:, 60:140] = 1.0
    return img, matte


def test_evaporate_does_not_treat_dark_parts_of_the_figure_as_its_edge():
    img, matte = _figure_with_a_dark_gap()
    kwargs = {"cell": 5, "start": 0, "outline": 0.1, "outward": 0.5, "seed": 1}
    for given in (matte, None):  # with the real silhouette, and with the fallback that fills enclosed gaps
        out = registry.apply("evaporate", img, matte=given, **kwargs)
        assert out[100:140, 80:120][img[100:140, 80:120] > 0].min() == 1.0  # lit pieces around the gap survive


def test_evaporate_takes_its_edge_from_the_matte_when_given():
    img = np.ones((200, 200, 3), np.float32)  # everything is lit...
    matte = np.zeros((200, 200), np.float32)
    matte[:, 80:120] = 1.0  # ...but the figure is only a thin column; the rest counts as background
    kwargs = {"cell": 5, "start": 0, "outline": 0.05, "seed": 1}
    with_matte = registry.apply("evaporate", img, matte=matte, **kwargs)
    without = registry.apply("evaporate", img, **kwargs)
    assert with_matte[150:, 20:60].mean() < 1.0  # outside the silhouette: sheds
    assert with_matte[150:, 95:105].min() == 1.0  # the middle of the figure is further than `outline` from its outline
    assert without[150:, 20:60].min() == 1.0  # with no matte there is no such outline (only the picture borders)


def test_evaporate_background_threshold_is_how_bright_a_piece_must_be_to_drift():
    dim = np.zeros((120, 160, 3), np.float32)
    dim[60:100, 60:100] = 0.1  # a dim block on black; everything above it is empty
    everything_detaches = {"start": 1.0, "ramp": 0.01, "vanish": 0.0, "thinning": 0.0, "seed": 1}
    assert registry.apply("evaporate", dim, **everything_detaches)[:60].max() > 0.0  # by default (0.02) it drifts up
    above = registry.apply("evaporate", dim, background_threshold=0.2, **everything_detaches)[:60]
    assert above.max() == 0.0  # no brighter than the threshold: background, nothing drifts


def test_evaporate_matte_threshold_is_how_much_of_a_piece_the_matte_must_cover():
    img = np.ones((200, 200, 3), np.float32)
    matte = np.full((200, 200), 0.4, np.float32)  # a faint matte everywhere,
    matte[:, 80:120] = 1.0  # and a solid column
    kwargs = {"cell": 5, "start": 0, "outline": 0.05, "seed": 1}
    outside = (slice(150, None), slice(20, 60))  # more than `outline` (10 px) from the picture's borders
    assert registry.apply("evaporate", img, matte=matte, **kwargs)[outside].mean() < 1.0  # 0.4 < 0.5: not figure
    assert registry.apply("evaporate", img, matte=matte, matte_threshold=0.3, **kwargs)[outside].min() == 1.0


def test_evaporate_wind_pushes_the_pieces_sideways():
    lit = np.zeros((200, 200, 3), np.float32)
    lit[120:140, 90:110] = 1.0  # a lit block near the middle
    kwargs = {"cell": 5, "start": 1.0, "ramp": 0.5, "rise": 0.5, "seed": 3}
    right = registry.apply("evaporate", lit, wind=0.8, **kwargs)
    left = registry.apply("evaporate", lit, wind=-0.8, **kwargs)
    columns = np.arange(200)
    centre = lambda img: float((img.max(axis=(0, 2)) * columns).sum() / img.max(axis=(0, 2)).sum())  # noqa: E731  (a one-line helper)
    assert centre(right) > centre(left)


def test_evaporate_bottom_border_is_an_edge_only_when_open_bottom_is_off():
    lit = np.ones((100, 120, 3), np.float32)
    kwargs = {"cell": 5, "start": 0, "outline": 0.2, "seed": 2}
    assert registry.apply("evaporate", lit, **kwargs)[-10:, 50:70].min() == 1.0
    assert registry.apply("evaporate", lit, open_bottom=False, **kwargs)[-10:, 50:70].mean() < 1.0


def test_evaporate_without_falloff_keeps_risen_pieces_at_full_brightness():
    lit = np.ones((120, 128, 3), np.float32)
    kwargs = {"cell": 5, "start": 0.9, "ramp": 0.4, "rise": 0.4, "seed": 2}
    dimmed = registry.apply("evaporate", lit, **kwargs)
    undimmed = registry.apply("evaporate", lit, falloff=0.0, **kwargs)
    assert set(np.unique(undimmed)) == {0.0, 1.0}  # every piece is either gone or as bright as it was
    assert ((dimmed > 0) & (dimmed < 1)).any()  # by default they fade with height


def _one_column_of_pieces() -> np.ndarray:
    cell_w, cell_h = cell_size(5)
    img = np.zeros((40 * cell_h, 40 * cell_w, 3), np.float32)
    img[20 * cell_h :, 20 * cell_w : 21 * cell_w] = 1.0  # one character wide, from the middle down
    return img


def test_evaporate_without_sway_pieces_rise_straight_up():
    img = _one_column_of_pieces()
    kwargs = {"cell": 5, "start": 1.0, "ramp": 0.4, "rise": 0.5, "vanish": 0.0, "seed": 1}
    straight = registry.apply("evaporate", img, sway=0.0, sway_minimum=0.0, **kwargs)
    swaying = registry.apply("evaporate", img, **kwargs)
    column = slice(20 * 5, 21 * 5)
    assert straight[: img.shape[0] // 2, column].max() > 0  # pieces did rise...
    assert np.delete(straight, np.s_[column], axis=1).max() == 0.0  # ...and none left their column
    assert np.delete(swaying, np.s_[column], axis=1).max() > 0.0  # by default they sway sideways


def test_evaporate_thinning_and_vanish_decide_how_many_risen_pieces_survive():
    lit = np.ones((120, 128, 3), np.float32)
    kwargs = {"cell": 5, "start": 0.9, "ramp": 0.4, "rise": 0.4, "seed": 2, "vanish": 0.0}
    upper = slice(0, 40)
    every = registry.apply("evaporate", lit, thinning=0.0, **kwargs)[upper].sum()
    thinned = registry.apply("evaporate", lit, thinning=1.0, **kwargs)[upper].sum()
    assert thinned < every


def _one_row_of_pieces(row: int = 30) -> np.ndarray:
    """40 rows of 64 pieces, black but for piece row `row`."""
    cell_w, cell_h = cell_size(5)
    img = np.zeros((40 * cell_h, 64 * cell_w, 3), np.float32)
    img[row * cell_h : (row + 1) * cell_h] = 1.0
    return img


def _rows_with_content(frame: np.ndarray) -> set[int]:
    cell_h = cell_size(5)[1]
    return {int(r) // cell_h for r in np.nonzero(frame.max(axis=(1, 2)) > 0)[0]}


# Every detached piece is visible where it lands: none vanish, none dim, none sway out of their row.
_LANDING = {"cell": 5, "vanish": 0.0, "thinning": 0.0, "falloff": 0.0, "sway": 0.0, "sway_minimum": 0.0, "seed": 1}


def test_evaporate_rise_minimum_one_makes_every_fully_pulled_piece_climb_the_whole_rise():
    img = _one_row_of_pieces(30)
    pulled = {"start": 1.0, "ramp": 0.01, "rise": 0.25, **_LANDING}  # row 30 is above the ramp: every piece detaches
    assert _rows_with_content(registry.apply("evaporate", img, rise_minimum=1.0, **pulled)) == {20}  # 0.25 x 40 rows
    assert len(_rows_with_content(registry.apply("evaporate", img, **pulled))) > 3  # by default climbs differ


def test_evaporate_rise_gamma_zero_lets_weakly_pulled_pieces_climb_as_far_as_strongly_pulled_ones():
    img = _one_row_of_pieces(30)
    weak = {"start": 1.0, "ramp": 1.0, "rise": 0.25, "rise_minimum": 1.0, **_LANDING}  # row 30: pulled at about 0.14
    assert _rows_with_content(registry.apply("evaporate", img, rise_gamma=0.0, **weak)) == {20, 30}
    assert 20 not in _rows_with_content(registry.apply("evaporate", img, **weak))  # default gamma: they climb less


def test_evaporate_rise_skew_above_one_keeps_most_pieces_low():
    img = _one_row_of_pieces(30)
    pulled = {"start": 1.0, "ramp": 0.01, "rise": 0.5, "rise_minimum": 0.0, **_LANDING}

    def mean_climb(skew: float) -> float:
        """Average height climbed by the lit pixels, in px."""
        profile = registry.apply("evaporate", img, rise_skew=skew, **pulled).max(axis=(1, 2))
        rows = np.arange(len(profile))
        return 30 * cell_size(5)[1] - float((rows * profile).sum() / profile.sum())

    assert mean_climb(4.0) < mean_climb(1.0) < mean_climb(0.25)


def _ragged_picture() -> np.ndarray:
    """Lit and dark pieces at random, so the outline turns at every piece."""
    cell_w, cell_h = cell_size(5)
    lit = np.random.default_rng(0).random((20, 40)) < 0.5
    return np.kron(lit, np.ones((cell_h, cell_w)))[..., None].repeat(3, axis=2).astype(np.float32)


def test_evaporate_normal_smoothing_changes_which_way_pieces_are_thrown():
    kwargs = {"cell": 5, "start": 0, "outline": 0.1, "outward": 1.0, "seed": 1}
    sharp = registry.apply("evaporate", _ragged_picture(), normal_smoothing=1.0, **kwargs)
    blurred = registry.apply("evaporate", _ragged_picture(), normal_smoothing=8.0, **kwargs)
    assert not np.array_equal(sharp, blurred)
    for out in (sharp, blurred):
        assert out.min() >= 0.0
        assert out.max() <= 1.0


@pytest.mark.parametrize(
    ("field", "value"),
    [
        pytest.param("normal_truncate", 1.0, id="a shorter cutoff changes the direction read from the blurred outline"),
        pytest.param("edge", "wrap", id="reading the opposite border beyond the picture changes it"),
        pytest.param("edge", "nearest", id="repeating the border row beyond the picture changes it"),
        pytest.param("edge", "constant", id="open space beyond the picture changes it"),
    ],
)
def test_evaporate_normal_blur_options_change_which_way_pieces_are_thrown(field, value):
    kwargs = {"cell": 5, "start": 0, "outline": 0.1, "outward": 1.0, "seed": 1, "normal_smoothing": 3.0}
    default = registry.apply("evaporate", _ragged_picture(), **kwargs)
    changed = registry.apply("evaporate", _ragged_picture(), **kwargs, **{field: value})
    assert not np.array_equal(default, changed)
