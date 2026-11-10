"""The edge rules of the neighbourhood filters: what lies beyond a picture's edge."""

from typing import Literal

# 'reflect' (mirrored, edge pixel repeated), 'nearest' (the edge pixel repeated), 'mirror' (mirrored about the edge
# pixel) or 'wrap' (the opposite edge).
Edge = Literal["reflect", "nearest", "mirror", "wrap"]
