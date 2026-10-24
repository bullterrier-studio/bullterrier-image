"""Behaviour of the `voronoi_cells` effect: random cells, each filled from a colour ramp, in place of the picture."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "voronoi_cells"
HEIGHT, WIDTH = 40, 60
WHITE = np.ones(3)


def render(shape: tuple[int, int] = (HEIGHT, WIDTH), **settings: object) -> np.ndarray:
    """The cells over a picture of `shape` (h, w): flat colours, no borders, unless `settings` say otherwise."""
    settings = {"shade": 0, "line_width": 0, **settings}
    return registry.apply(NAME, np.zeros((*shape, 3), np.float32), **settings)


def sites(cells: int, seed: int, shape: tuple[int, int] = (HEIGHT, WIDTH)) -> np.ndarray:
    """The cell sites in px (x, y) as the effect draws them: default_rng(seed).random((cells, 2)) as fractions of the
    width and the height. Not promised to users; pinned here so that a seed keeps its picture from release to release
    and the test can know where the sites are."""
    return np.random.default_rng(seed).random((cells, 2)) * (shape[1], shape[0])


def distances_to_sites(points: np.ndarray, site_list: np.ndarray) -> np.ndarray:
    """(h, w, cells): the distance from every pixel centre to every site, brute force."""
    y, x = np.mgrid[0 : points.shape[0], 0 : points.shape[1]] + 0.5
    return np.hypot(x[..., None] - site_list[:, 0], y[..., None] - site_list[:, 1])


def distances_to_cell_border(distance: np.ndarray, site_list: np.ndarray) -> np.ndarray:
    """(h, w): how far each pixel centre is from the border of its cell. A cell is convex: the intersection of the
    half-planes closer to its own site than to each other site, so the distance to its border is the least distance
    to a bisector line, (d_other**2 - d_own**2) / (2 |own - other|), over every other site."""
    own = distance.argmin(axis=-1)
    d_own = np.take_along_axis(distance, own[..., None], axis=-1)[..., 0]
    least = np.full(own.shape, np.inf)
    for other in range(len(site_list)):
        gap = np.linalg.norm(site_list[own] - site_list[other], axis=-1)
        line = (distance[..., other] ** 2 - d_own**2) / (2 * np.maximum(gap, 1e-9))
        least = np.minimum(least, np.where(own == other, np.inf, line))
    return least


@pytest.mark.parametrize(("cells", "seed"), [(6, 3), (25, 1)])
def test_every_pixel_belongs_to_the_cell_of_its_nearest_site_and_each_cell_is_one_colour(cells: int, seed: int) -> None:
    out = render(cells=cells, seed=seed)
    nearest = distances_to_sites(out, sites(cells, seed)).argmin(axis=-1)
    for site in range(cells):
        colours = out[nearest == site]
        assert len(colours) > 0
        np.testing.assert_array_equal(colours, np.tile(colours[0], (len(colours), 1)))
    per_cell = np.array([out[nearest == site][0] for site in range(cells)])
    assert len(np.unique(per_cell, axis=0)) == cells  # and no two cells share a colour


def test_cells_are_coloured_through_the_ramp_by_their_own_random_value() -> None:
    """Between red and blue a cell at value v on the ramp is (1 - v, 0, v). The values are drawn after the sites,
    one per site, from the same seeded generator."""
    cells, seed = 20, 7
    out = render(cells=cells, seed=seed, colors=["#ff0000", "#0000ff"])
    generator = np.random.default_rng(seed)
    generator.random((cells, 2))  # the sites
    values = generator.random(cells)
    nearest = distances_to_sites(out, sites(cells, seed)).argmin(axis=-1)
    expected = np.stack([1 - values[nearest], np.zeros(nearest.shape), values[nearest]], axis=-1)
    np.testing.assert_allclose(out, expected, atol=1e-6)


@pytest.mark.parametrize("shade", [0.5, 1.0])
def test_shade_darkens_a_pixel_by_how_far_it_is_from_its_site_toward_the_border(shade: float) -> None:
    """From no darkening at the site to `shade` of the way to black at the border: the distance to the site over
    the distance to the site plus the distance to the border."""
    flat, shaded = render(cells=8, seed=2), render(cells=8, seed=2, shade=shade)
    distance = distances_to_sites(flat, sites(8, 2))
    own = distance.min(axis=-1)
    dome = own / (own + distances_to_cell_border(distance, sites(8, 2)))
    np.testing.assert_allclose(shaded, flat * (1 - shade * dome[..., None]), atol=1e-4)


@pytest.mark.parametrize("seed", [3, 4, 9])
def test_borders_are_drawn_line_width_wide_about_the_cell_borders(seed: int) -> None:
    """A line 6 px wide is solid within 3 px of a border and fades out over the next pixel (1 px ramp across its edge,
    half a pixel each side of 3 px), so a pixel a distance b from the border is covered by clip(3 - b + 0.5, 0, 1)."""
    flat = render(cells=6, seed=seed)
    out = render(cells=6, seed=seed, line_width=6, line_color="#ffffff")
    border = distances_to_cell_border(distances_to_sites(flat, sites(6, seed)), sites(6, seed))
    coverage = np.clip(3 - border + 0.5, 0, 1)[..., None]
    np.testing.assert_allclose(out, flat + (WHITE - flat) * coverage, atol=1e-3)
    assert (border < 2).any()


def test_a_line_thinner_than_a_pixel_is_fainter_in_proportion() -> None:
    """A 0.5 px line can colour no more than half of a pixel: the same ramp as above, half as strong."""
    flat = render(cells=6, seed=3)
    out = render(cells=6, seed=3, line_width=0.5, line_color="#ffffff")
    border = distances_to_cell_border(distances_to_sites(flat, sites(6, 3)), sites(6, 3))
    coverage = 0.5 * np.clip(0.25 - border + 0.5, 0, 1)[..., None]
    np.testing.assert_allclose(out, flat + (WHITE - flat) * coverage, atol=1e-3)


def test_line_width_zero_draws_no_border() -> None:
    out = render(cells=30, line_width=0, line_color="#ffffff", colors=["#000000", "#808080"])
    assert out.max() < 0.51  # no white anywhere


def test_the_seed_decides_the_cells() -> None:
    np.testing.assert_array_equal(render(cells=12, seed=5), render(cells=12, seed=5))
    assert not np.array_equal(render(cells=12, seed=5), render(cells=12, seed=6))
