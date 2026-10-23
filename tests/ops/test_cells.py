import numpy as np
import pytest

from photo_fx.ops.cells import cut, figure, join, lit

# (height, width, cell_w, cell_h): multiples, non-multiples, and a cell larger than the picture.
SIZES = [(96, 128, 8, 16), (97, 131, 5, 11), (10, 7, 3, 4), (5, 9, 16, 12), (1, 1, 1, 1), (33, 20, 7, 1)]


def random_image(height: int, width: int, seed: int = 0) -> np.ndarray:
    return np.random.default_rng(seed).random((height, width, 3), dtype=np.float32)


@pytest.mark.parametrize(("height", "width", "cell_w", "cell_h"), SIZES)
def test_join_undoes_cut_exactly(height, width, cell_w, cell_h):
    image = random_image(height, width)
    restored = join(cut(image, cell_w, cell_h), height, width)
    assert restored.dtype == np.float32
    np.testing.assert_array_equal(restored, image)


@pytest.mark.parametrize(("height", "width", "cell_w", "cell_h"), SIZES)
def test_cut_pieces_hold_the_pixels_at_their_place_and_nothing_else(height, width, cell_w, cell_h):
    image = random_image(height, width, seed=1)
    pieces = cut(image, cell_w, cell_h)
    rows, cols = -(-height // cell_h), -(-width // cell_w)
    assert pieces.shape == (rows, cols, cell_h, cell_w, 3)
    for r, c, i, j in [(0, 0, 0, 0), (rows - 1, cols - 1, cell_h - 1, cell_w - 1), (rows - 1, 0, cell_h - 1, 0)]:
        y, x = r * cell_h + i, c * cell_w + j
        expected = image[y, x] if y < height and x < width else np.zeros(3)
        np.testing.assert_array_equal(pieces[r, c, i, j], expected)


def test_cut_pads_with_black_to_whole_cells():
    image = np.ones((5, 7, 3), dtype=np.float32)
    pieces = cut(image, cell_w=4, cell_h=3)  # 2 x 2 pieces covering 6 x 8
    assert pieces.shape == (2, 2, 3, 4, 3)
    assert pieces.sum() == 5 * 7 * 3  # every input pixel once, padding contributes nothing
    np.testing.assert_array_equal(pieces[1, 1, 2:, :], 0.0)  # padded rows of the bottom pieces
    np.testing.assert_array_equal(pieces[1, 1, :, 3:], 0.0)  # padded column of the right pieces


def test_cut_and_join_leave_their_input_untouched():
    image = random_image(20, 30)
    before = image.copy()
    pieces = cut(image, 6, 7)
    pieces_before = pieces.copy()
    join(pieces, 20, 30)
    np.testing.assert_array_equal(image, before)
    np.testing.assert_array_equal(pieces, pieces_before)


@pytest.mark.parametrize(("cell_w", "cell_h"), [(0, 4), (4, 0), (-1, 4), (4, -3)])
def test_cut_rejects_cell_sizes_that_are_not_positive(cell_w, cell_h):
    with pytest.raises(ValueError, match="positive"):
        cut(np.zeros((8, 8, 3), dtype=np.float32), cell_w, cell_h)


def test_lit_is_true_exactly_for_pieces_with_a_value_above_threshold():
    pieces = np.zeros((2, 3, 4, 5, 3), dtype=np.float32)
    pieces[0, 0, 0, 0, 0] = 0.0201  # one channel of one pixel is enough
    pieces[0, 1, 3, 4, 2] = 0.02  # exactly at the threshold: not lit
    pieces[0, 2, 1, 1, 1] = 0.019
    pieces[1, 0] = 0.5
    pieces[1, 2, 2, 3, 0] = 1.0
    expected = np.array([[True, False, False], [True, False, True]])
    result = lit(pieces, threshold=0.02)
    assert result.dtype == bool
    np.testing.assert_array_equal(result, expected)


def test_lit_follows_the_image_through_cut():
    image = np.zeros((20, 30, 3), dtype=np.float32)
    image[13, 22] = 0.9  # row 13 // 5 = 2, col 22 // 6 = 3
    result = lit(cut(image, cell_w=6, cell_h=5), threshold=0.02)
    assert result.shape == (4, 5)
    assert np.argwhere(result).tolist() == [[2, 3]]


def test_figure_with_a_matte_marks_a_piece_only_when_more_than_half_of_it_is_inside():
    cell_w, cell_h, height, width = 4, 4, 8, 12  # 2 x 3 pieces of 16 pixels
    matte = np.zeros((height, width), dtype=np.float32)
    matte[0:4, 0:4] = 1.0  # piece (0, 0): all 16
    matte[0:4, 4:8].flat[:8] = 1.0  # piece (0, 1): exactly half, 8 of 16
    matte[0:4, 8:12].flat[:9] = 1.0  # piece (0, 2): 9 of 16
    matte[4, 0] = 1.0  # piece (1, 0): 1 of 16
    lit_pieces = np.zeros((2, 3), dtype=bool)  # the matte decides: dark parts of the figure still count
    result = figure(lit_pieces, matte, cell_w, cell_h, height, width, threshold=0.5)
    np.testing.assert_array_equal(result, [[True, False, True], [False, False, False]])


def test_figure_with_a_matte_counts_the_padding_of_edge_pieces_as_outside():
    cell_w, cell_h, height, width = 4, 4, 6, 6  # padded to 8 x 8: the corner piece holds only 2 x 2 real pixels
    matte = np.ones((height, width), dtype=np.float32)
    lit_pieces = np.zeros((2, 2), dtype=bool)
    result = figure(lit_pieces, matte, cell_w, cell_h, height, width, threshold=0.5)
    # corner piece: 4 of 16 pixels inside; edge pieces: 8 of 16 (not more than half); first piece: full.
    np.testing.assert_array_equal(result, [[True, False], [False, False]])


def test_figure_with_a_matte_ignores_which_pieces_are_lit():
    matte = np.zeros((8, 8), dtype=np.float32)
    matte[:4, :4] = 1.0
    result = figure(np.ones((2, 2), dtype=bool), matte, 4, 4, 8, 8, threshold=0.5)
    np.testing.assert_array_equal(result, [[True, False], [False, False]])


def test_figure_with_a_matte_uses_cell_width_across_and_cell_height_down():
    # A 5 x 9 picture cut into pieces 4 px wide and 3 px tall: 2 x 3 pieces, padded to 6 x 12.
    cell_w, cell_h, height, width = 4, 3, 5, 9
    matte = np.zeros((height, width), dtype=np.float32)
    matte[0:3, 0:4] = 1.0  # piece (0, 0): all 12 pixels
    matte[0:2, 4:8] = 1.0  # piece (0, 1): 8 of 12
    matte[3:5, 0:4] = 1.0  # piece (1, 0): 8 real pixels of 12 (its third row is padding)
    matte[0:3, 8] = 1.0  # piece (0, 2): 3 of 12; its other 9 pixels are padding
    result = figure(np.zeros((2, 3), dtype=bool), matte, cell_w, cell_h, height, width, threshold=0.5)
    np.testing.assert_array_equal(result, [[True, True, False], [True, False, False]])


def test_figure_without_a_matte_fills_the_hole_of_a_lit_ring():
    ring = np.zeros((7, 7), dtype=bool)
    ring[1:6, 1:6] = True
    ring[2:5, 2:5] = False  # dark centre enclosed by lit pieces
    result = figure(ring, None, cell_w=5, cell_h=5, height=35, width=35, threshold=0.5)
    expected = np.zeros((7, 7), dtype=bool)
    expected[1:6, 1:6] = True
    np.testing.assert_array_equal(result, expected)


def test_figure_without_a_matte_keeps_open_gaps_and_unrelated_dark_pieces_dark():
    c_shape = np.zeros((7, 7), dtype=bool)
    c_shape[1:6, 1:6] = True
    c_shape[2:5, 2:5] = False
    c_shape[3, 5] = False  # a gap in the ring: the centre is no longer enclosed
    result = figure(c_shape, None, cell_w=5, cell_h=5, height=35, width=35, threshold=0.5)
    np.testing.assert_array_equal(result, c_shape)
