"""Voronoi cells over a picture: which site each pixel belongs to and how far it is from the edge of its cell.

Coordinates: a picture of `shape = (height, width)` is the rectangle `0 <= x < width`, `0 <= y < height`, and the pixel
in row `i`, column `j` has its centre at `(x, y) = (j + 0.5, i + 0.5)`. A site is an `(x, y)` pair; `sites` is a
`(count, 2)` float array of at least four sites that are not all on one line.
"""

from typing import NamedTuple

import numpy as np
from scipy.spatial import Delaunay, cKDTree


def centres(shape: tuple[int, int]) -> tuple[np.ndarray, np.ndarray]:
    """The `(x, y)` of every pixel centre of a picture of `shape` (height, width), as float32 arrays that broadcast
    against each other: `x` is `(1, width)` and `y` is `(height, 1)`."""
    height, width = shape
    return (np.arange(width) + 0.5).astype(np.float32)[None, :], (np.arange(height) + 0.5).astype(np.float32)[:, None]


def nearest(sites: np.ndarray, x: np.ndarray, y: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """For every coordinate pair `(x, y)` (arrays that broadcast against each other, in px) the distance in px to the
    closest of `sites` and that site's index, as arrays of the broadcast shape. The coordinates need not be a grid:
    a torn edge classifies the pixels by coordinates it has bent. A pixel exactly between two sites goes to one of
    them, the same one every time."""
    x, y = np.broadcast_arrays(x, y)
    distance, index = cKDTree(sites).query(np.stack([x.ravel(), y.ravel()], axis=1))
    return distance.reshape(x.shape), index.reshape(x.shape)


class Cells(NamedTuple):
    """The cells of a set of sites over a picture, each array of shape (height, width)."""

    owner: np.ndarray  # intp: the index of the site nearest each pixel centre (the pixel's cell)
    site_distance: np.ndarray  # float64: the distance in px from each pixel centre to its own site
    border_distance: np.ndarray  # float32: the distance in px from each pixel centre to the border of its own cell


def cells(shape: tuple[int, int], sites: np.ndarray) -> Cells:
    """The Voronoi cells of `sites` over a picture of `shape` (height, width): for every pixel its cell, its distance to
    the cell's site and its distance to the cell's border (the cell is convex, so that is the least distance to one of
    the bisector lines that bound it, which are those between the site and the sites its Delaunay triangles join it
    to; it is exact, also for a pixel near the edge of the picture)."""
    x, y = centres(shape)
    own_distance, own = (a.ravel() for a in nearest(sites, x, y))
    px, py = (a.ravel() for a in np.broadcast_arrays(x, y))
    # Neighbours are the sites the Delaunay triangulation joins to a site; short lists are padded by repeating one,
    # which changes no minimum.
    indptr, indices = Delaunay(sites).vertex_neighbor_vertices
    slots = np.arange(np.diff(indptr).max())
    slot = np.minimum(slots[None, :], np.diff(indptr)[:, None] - 1)
    neighbour = indices[indptr[:-1, None] + slot]  # (sites, most neighbours of any site)
    away = sites[neighbour] - sites[:, None, :]  # from each site to each of its neighbours
    normal = (away / np.linalg.norm(away, axis=-1, keepdims=True)).astype(np.float32)  # unit, pointing out of the cell
    offset = (normal * (sites[:, None, :] + sites[neighbour]) / 2).sum(axis=-1).astype(np.float32)  # normal . midpoint
    border = np.full(px.shape, np.inf, np.float32)
    for k in slots:
        np.minimum(border, offset[own, k] - normal[own, k, 0] * px - normal[own, k, 1] * py, out=border)
    return Cells(own.reshape(shape), own_distance.reshape(shape), border.reshape(shape))


def line_coverage(border_distance: np.ndarray, width: float) -> np.ndarray:
    """How much of each pixel a line `width` px wide along the cell borders covers, 0..1, for the pixels' distances
    `border_distance` (from `cells`): a one-pixel ramp across the line's edge, and fainter than 1 px for a line
    thinner than a pixel. Width 0 covers nothing."""
    # constant: a pixel is one px wide, so its edge ramp spans 1 px (half of it each side)
    return np.clip(width / 2 - border_distance + 0.5, 0.0, 1.0) * min(width, 1.0)
