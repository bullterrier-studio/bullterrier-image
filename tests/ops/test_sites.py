"""Tests of `ops/sites.py`: where the sites of a picture's pieces are placed, evenly or where there is detail."""

import numpy as np
import pytest

from bullterrier_image.ops.sites import detail_weights, sites

SHAPE = (40, 60)


def test_without_weights_sites_lie_anywhere_in_the_picture_and_follow_the_generator() -> None:
    placed = sites(SHAPE, 500, np.random.default_rng(1), weights=None)
    assert placed.shape == (500, 2)
    assert (placed >= 0).all()
    assert (placed[:, 0] < SHAPE[1]).all()
    assert (placed[:, 1] < SHAPE[0]).all()
    again = sites(SHAPE, 500, np.random.default_rng(1), weights=None)
    np.testing.assert_array_equal(placed, again)
    assert not np.array_equal(placed, sites(SHAPE, 500, np.random.default_rng(2), weights=None))


def test_weights_on_one_pixel_put_every_site_in_that_pixel() -> None:
    weights = np.zeros(SHAPE)
    weights[5, 7] = 1.0
    placed = sites(SHAPE, 50, np.random.default_rng(0), weights=weights)
    assert (placed[:, 0] >= 7).all()  # x runs along the columns, y down the rows
    assert (placed[:, 0] < 8).all()
    assert (placed[:, 1] >= 5).all()
    assert (placed[:, 1] < 6).all()
    assert len(np.unique(placed, axis=0)) == 50  # inside the pixel they still differ


def test_sites_fall_in_proportion_to_the_weights() -> None:
    weights = np.ones(SHAPE)
    weights[:, SHAPE[1] // 2 :] = 3.0  # the right half three times as likely per pixel
    placed = sites(SHAPE, 8000, np.random.default_rng(3), weights=weights)
    assert (placed[:, 0] >= SHAPE[1] // 2).mean() == pytest.approx(0.75, abs=0.02)


def step_edge() -> np.ndarray:
    """Black left of column 10, white from it on: the only detail is the step between columns 9 and 10."""
    rgb = np.zeros((*SHAPE, 3), np.float32)
    rgb[:, 10:] = 1.0
    return rgb


def test_full_detail_weights_follow_the_edges_and_sum_to_one() -> None:
    weights = detail_weights(step_edge(), detail=1.0, edge="reflect")
    assert weights.sum() == pytest.approx(1.0)
    assert weights[:, [9, 10]].sum() == pytest.approx(1.0)  # all of it on the two columns beside the step
    np.testing.assert_allclose(weights[:, 9], weights[:, 10])
    np.testing.assert_allclose(weights[:, 9], weights[0, 9])  # and the same all the way down


@pytest.mark.parametrize("detail", [0.25, 0.5, 0.75])
def test_partial_detail_mixes_an_even_share_with_the_edge_share(detail: float) -> None:
    weights = detail_weights(step_edge(), detail=detail, edge="reflect")
    even = (1 - detail) / weights.size
    np.testing.assert_allclose(weights[:, 20], even)  # a flat pixel keeps only its even share
    np.testing.assert_allclose(weights[:, 9], even + detail / (2 * SHAPE[0]))  # the step pixels add the edge share
    assert weights.sum() == pytest.approx(1.0)


def test_no_detail_or_no_edges_give_even_weights() -> None:
    even = np.full(SHAPE, 1 / (SHAPE[0] * SHAPE[1]))
    np.testing.assert_allclose(detail_weights(step_edge(), detail=0.0, edge="reflect"), even)
    flat = np.full((*SHAPE, 3), 0.4, np.float32)
    np.testing.assert_allclose(detail_weights(flat, detail=1.0, edge="reflect"), even)


def test_the_picture_is_not_modified() -> None:
    rgb = np.random.default_rng(4).random((*SHAPE, 3), dtype=np.float32)
    before = rgb.copy()
    detail_weights(rgb, detail=0.7, edge="wrap")
    np.testing.assert_array_equal(rgb, before)


@pytest.mark.parametrize("channel", [0, 1, 2], ids=["red", "green", "blue"])
def test_an_edge_in_any_one_colour_counts_as_detail(channel: int) -> None:
    rgb = np.zeros((*SHAPE, 3), np.float32)
    rgb[:, 10:, channel] = 1.0
    weights = detail_weights(rgb, detail=1.0, edge="reflect")
    assert weights[:, [9, 10]].sum() == pytest.approx(1.0)


def test_a_faint_edge_counts_as_much_as_a_sharp_one() -> None:
    """Only where the edges are matters, not how strong they are in all: the weights do not depend on the contrast."""
    faint, sharp = np.zeros((*SHAPE, 3), np.float32), np.zeros((*SHAPE, 3), np.float32)
    faint[:, 10:] = 1e-4
    sharp[:, 10:] = 1.0
    np.testing.assert_allclose(
        detail_weights(faint, detail=1.0, edge="reflect"), detail_weights(sharp, detail=1.0, edge="reflect"), atol=1e-6
    )
