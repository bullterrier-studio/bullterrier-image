"""The edge rules of the neighbourhood filters (edges, relief, sites): what lies beyond a picture's edge."""

from typing import Literal

# 'reflect' (mirrored, edge pixel repeated), 'nearest' (the edge pixel repeated), 'mirror' (mirrored about the edge
# pixel) or 'wrap' (the opposite edge); `bullterrier_image.ops.blur.NUMPY_PAD` maps them to numpy's names.
Edge = Literal["reflect", "nearest", "mirror", "wrap"]
