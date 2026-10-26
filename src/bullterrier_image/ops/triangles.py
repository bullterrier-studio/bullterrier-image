"""A picture cut into triangles: the Delaunay triangles of some sites and the picture's four corners.

Coordinates as in `ops/voronoi.py`: a point is an `(x, y)` pair in px from the picture's top left corner, and the
pixel in row `i`, column `j` has its centre at `(j + 0.5, i + 0.5)`.
"""

from typing import NamedTuple

import numpy as np
from scipy.spatial import Delaunay

# constant: pixels placed in a triangle at once; bounds the working memory and never changes the result
_BAND_PIXELS = 2**16


class Pieces(NamedTuple):
    """The triangles of a picture."""

    owner: np.ndarray  # intp (height, width): the index of the triangle that holds each pixel's centre
    border_distance: np.ndarray  # float32 (height, width): px from each pixel's centre to the border of its triangle
    vertices: np.ndarray  # float64 (count, 3, 2): the `(x, y)` of each triangle's three corners


def _border(shape: tuple[int, int], spacing: float) -> np.ndarray:
    """Points round the picture's edge, going clockwise from the top left corner: each side divided into as many equal
    parts as are about `spacing` px long (at least one), so the four corners are among them."""
    height, width = shape
    across, down = max(1, round(width / spacing)), max(1, round(height / spacing))
    top = np.stack([width * np.arange(across) / across, np.zeros(across)], axis=1)
    right = np.stack([np.full(down, width), height * np.arange(down) / down], axis=1)
    bottom = np.stack([width - width * np.arange(across) / across, np.full(across, height)], axis=1)
    left = np.stack([np.zeros(down), height - height * np.arange(down) / down], axis=1)
    return np.vstack([top, right, bottom, left])


def triangles(shape: tuple[int, int], sites: np.ndarray, *, spacing: float) -> Pieces:
    """The Delaunay triangles of `sites` (an `(n, 2)` float array of `(x, y)` inside a picture of `shape`, n at least 1)
    and of points round the picture's edge, `spacing` px apart or a little more or less (each side is cut into equal
    parts, at least one), which are added so that the triangles cover every pixel and none fans out of a corner.

    A pixel's border distance is its distance to the nearest side its triangle shares with another triangle, so the
    picture's own edge is no border; it is the distance to the side's line, which is the distance to the side itself
    except beside the picture's edge, where it may come out a little short.
    """
    height, width = shape
    points = np.vstack([np.asarray(sites, np.float64), _border(shape, spacing)])
    mesh = Delaunay(points)
    vertices = points[mesh.simplices]  # (count, 3, 2)
    # the altitude from each corner of a triangle to the side opposite it: twice the area over that side's length
    sides = vertices[:, [1, 2, 0]] - vertices  # side i runs from corner i to the next corner
    opposite = np.linalg.norm(sides[:, [1, 2, 0]], axis=2)  # corner i is opposite side i + 1, from corner i + 1 on
    twice_area = np.abs(sides[:, 0, 0] * sides[:, 1, 1] - sides[:, 0, 1] * sides[:, 1, 0])
    altitude = twice_area[:, None] / opposite
    reach_of = np.where(mesh.neighbors >= 0, altitude, np.inf)  # a side on the picture's edge is no border
    owner = np.empty((height, width), np.intp)
    border = np.empty((height, width), np.float32)
    # Each pixel depends only on its own place, so they are done a band of rows at a time: the same result as all at
    # once, with the temporaries of one band instead of the whole picture's.
    band = max(1, _BAND_PIXELS // width)
    for top in range(0, height, band):
        y, x = np.mgrid[top : min(top + band, height), 0:width]
        centres = np.stack([x.ravel() + 0.5, y.ravel() + 0.5], axis=1)
        found = mesh.find_simplex(centres)
        # barycentric coordinates of each pixel in its triangle, from scipy's affine transform of the triangles
        transform = mesh.transform[found]  # (pixels, 3, 2): the 2 x 2 inverse, then the third corner
        first_two = np.einsum("pij,pj->pi", transform[:, :2], centres - transform[:, 2])
        bary = np.concatenate([first_two, 1 - first_two.sum(axis=1, keepdims=True)], axis=1)
        owner[top : top + band] = found.reshape(y.shape)
        border[top : top + band] = (reach_of[found] * bary).min(axis=1).reshape(y.shape)
    return Pieces(owner, border, vertices)
