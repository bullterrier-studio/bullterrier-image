"""Behaviour of the `torn_edge` effect: the picture's border torn away along a ragged line and replaced by paper."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "torn_edge"
FILL = (1.0, 0.0, 1.0)  # magenta: nothing in the grey test picture can be mistaken for it
MAGENTA = "#ff00ff"


def grey(height: int = 200, width: int = 200) -> np.ndarray:
    return np.full((height, width, 3), 0.5, np.float32)


def torn(settings: dict, height: int = 200, width: int = 200) -> np.ndarray:
    """Which pixels of a grey picture the tear removed (replaced by the magenta fill): (h, w) bool."""
    out = registry.apply(NAME, grey(height, width), fill=MAGENTA, **settings)
    return (out == np.array(FILL, np.float32)).all(axis=-1)


def run_from_top(removed: np.ndarray, columns: slice) -> np.ndarray:
    """In each of `columns`, how many removed pixels there are from the top edge down to the first kept one."""
    return np.argmin(removed[:, columns], axis=0)


def test_roughness_zero_gives_a_straight_edge_of_exactly_the_depth() -> None:
    """Pixels closer than `depth` px to a side are torn away: 8 rows and columns for a depth of 7.5."""
    removed = torn({"depth": 7.5, "roughness": 0.0})
    expected = np.ones((200, 200), bool)
    expected[8:-8, 8:-8] = False
    np.testing.assert_array_equal(removed, expected)


def test_depth_zero_tears_nothing(photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, depth=0.0, roughness=1.0, seed=3)
    np.testing.assert_array_equal(out, photo)
    assert not np.shares_memory(out, photo)


def test_every_pixel_is_either_untouched_or_exactly_the_fill_and_the_middle_is_whole(photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, depth=12.0, roughness=0.5, scale=10.0, fill="#336699", seed=2)
    fill = np.array([0x33, 0x66, 0x99], np.float32) / 255
    kept, replaced = (out == photo).all(axis=-1), (out == fill).all(axis=-1)
    assert (kept | replaced).all()
    assert replaced.any()
    assert replaced[0, 0]  # the corner is torn
    np.testing.assert_array_equal(out[40:-40, 40:-40], photo[40:-40, 40:-40])  # a tear never reaches 40 px here


def test_the_tear_is_one_line_per_side_with_nothing_torn_off_inside_it() -> None:
    """Going down a column from the top the pixels are removed up to the tear and kept after it, never alternating."""
    removed = torn({"depth": 25.0, "roughness": 0.8, "scale": 8.0, "seed": 4}, height=400, width=400)
    middle = removed[:, 120:-120]
    after_first_kept = np.cumsum(~middle, axis=0) > 0
    top_half = slice(0, 200)
    assert not (middle & after_first_kept)[top_half].any()  # no removed pixel below a kept one, in the upper half


@pytest.mark.parametrize("roughness", [0.1, 0.2, 0.4])
def test_the_tear_wanders_about_the_depth_by_the_roughness_times_the_depth(roughness: float) -> None:
    """The tear's distance from the side has mean `depth` and standard deviation `roughness` x `depth` (when it stays
    positive); measured over the columns of a long top side that no other side's tear reaches, with the ~0.3 px a
    one-pixel grid adds (it counts whole pixels)."""
    removed = torn({"depth": 40.0, "roughness": roughness, "scale": 12.0, "seed": 1}, height=300, width=3000)
    distance = run_from_top(removed, slice(300, -300))
    assert distance.mean() == pytest.approx(40.0, rel=0.1)
    assert distance.std() == pytest.approx(40.0 * roughness, rel=0.2)


def test_more_roughness_means_a_more_ragged_tear() -> None:
    spreads = [
        run_from_top(torn({"depth": 40.0, "roughness": r, "seed": 1}, height=300, width=1500), slice(200, -200)).std()
        for r in (0.0, 0.1, 0.3, 0.6)
    ]
    assert spreads == sorted(spreads)
    assert len(set(spreads)) == len(spreads)
    assert spreads[0] == 0.0


def test_a_larger_scale_gives_broader_bays_and_a_smaller_one_a_jagged_edge() -> None:
    def neighbour_correlation(scale: float) -> float:
        tear = run_from_top(
            torn({"depth": 40.0, "roughness": 0.3, "scale": scale, "octaves": 1}, 300, 1500), slice(200, -200)
        )
        return float(np.corrcoef(tear[:-1], tear[1:])[0, 1])

    assert neighbour_correlation(2.0) < neighbour_correlation(10.0) < neighbour_correlation(50.0)
    assert neighbour_correlation(50.0) > 0.99


@pytest.mark.parametrize(
    ("smoother", "rougher"),
    [
        pytest.param({"octaves": 1}, {"octaves": 5, "persistence": 0.8}, id="more octaves add fine jaggedness"),
        pytest.param({"persistence": 0.1}, {"persistence": 0.9}, id="more persistence keeps the fine layers strong"),
        pytest.param({"lacunarity": 1.2}, {"lacunarity": 3.0}, id="more lacunarity makes the layers finer"),
    ],
)
def test_the_tear_layers_are_set_by_octaves_persistence_and_lacunarity(smoother: dict, rougher: dict) -> None:
    base = {"depth": 40.0, "roughness": 0.3, "scale": 40.0, "octaves": 3, "persistence": 0.8, "lacunarity": 2.0}

    def jaggedness(settings: dict) -> float:
        tear = run_from_top(torn({**base, **settings, "seed": 1}, 300, 1500), slice(200, -200)).astype(float)
        return float(np.abs(np.diff(tear)).mean())  # mean step between neighbouring columns

    assert jaggedness(rougher) > 1.2 * jaggedness(smoother)


def test_each_side_has_its_own_tear() -> None:
    removed = torn({"depth": 30.0, "roughness": 0.5, "scale": 10.0, "seed": 1}, 300, 300)
    top = run_from_top(removed, slice(60, -60)).astype(float)
    bottom = run_from_top(removed[::-1], slice(60, -60)).astype(float)
    left = run_from_top(removed.T, slice(60, -60)).astype(float)
    right = run_from_top(removed.T[::-1], slice(60, -60)).astype(float)
    assert max(abs(np.corrcoef(a, b)[0, 1]) for a, b in [(top, bottom), (top, left), (top, right), (left, right)]) < 0.5
