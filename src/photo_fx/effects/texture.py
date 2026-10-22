"""Effects that add a surface or light to the picture: dots, glow, film grain."""

import numpy as np
from scipy import ndimage

from ._common import Color, ink_amount, luminance, parse_color, vivid
from ._registry import effect


@effect(category="stylised")
def halftone(
    rgb: np.ndarray,
    cell: int = 14,
    angle: float = 45.0,
    fg: Color | None = None,
    bg: Color = "#000000",
) -> np.ndarray:
    """Print-style dot screen: tone becomes dot size. `cell` = distance between dots in px, `angle` = screen rotation.

    `fg` = one colour for every dot; leave it out to take each dot's colour from the photo.
    """
    height, width = rgb.shape[:2]
    bg_color = parse_color(bg)
    smooth = np.stack([ndimage.gaussian_filter(rgb[..., c], cell / 2.0) for c in range(3)], axis=-1)

    y, x = np.mgrid[0:height, 0:width].astype(np.float32)
    cos, sin = np.cos(np.radians(angle)), np.sin(np.radians(angle))
    u, v = (x * cos + y * sin) / cell, (-x * sin + y * cos) / cell  # position in the rotated dot grid, in cells
    cu, cv = np.floor(u) + 0.5, np.floor(v) + 0.5  # centre of the dot this pixel belongs to
    dist = np.hypot(u - cu, v - cv)

    # Look the picture up at the dot's centre, mapped back to image coordinates.
    sx = np.clip(np.rint((cu * cos - cv * sin) * cell), 0, width - 1).astype(int)
    sy = np.clip(np.rint((cu * sin + cv * cos) * cell), 0, height - 1).astype(int)
    sampled = smooth[sy, sx]

    radius = np.sqrt(np.clip(ink_amount(luminance(sampled), bg_color), 0, 1)) * 0.7071  # 0.7071: dots just merge
    coverage = np.clip((radius - dist) * cell + 0.5, 0.0, 1.0)[..., None]  # one pixel of anti-aliasing
    dots = vivid(sampled) if fg is None else parse_color(fg)
    return (bg_color * (1.0 - coverage) + dots * coverage).astype(np.float32)


@effect(category="light")
def glow(rgb: np.ndarray, radius: float = 24.0, strength: float = 0.6, threshold: float = 0.55) -> np.ndarray:
    """Soft bloom around the bright parts. `radius` in px, `strength` 0..1+, `threshold` = where glow starts."""
    bright = np.clip((rgb - threshold) / (1.0 - threshold), 0.0, 1.0)
    halo = ndimage.gaussian_filter(bright, sigma=(radius, radius, 0))
    return (1.0 - (1.0 - rgb) * (1.0 - np.clip(strength * halo, 0, 1))).astype(np.float32)  # screen blend


@effect(category="texture")
def grain(rgb: np.ndarray, amount: float = 0.08, size: float = 1.0, seed: int = 0) -> np.ndarray:
    """Film grain: brightness noise of strength `amount` (0.08 is visible, 0.2 is heavy), clumped to `size` px.

    The same `seed` always gives the same grain, so renders are repeatable.
    """
    noise = np.random.default_rng(seed).standard_normal(rgb.shape[:2]).astype(np.float32)
    if size > 1.0:
        noise = ndimage.gaussian_filter(noise, size / 2.0)
        noise /= max(float(noise.std()), 1e-6)
    return np.clip(rgb + amount * noise[..., None], 0.0, 1.0).astype(np.float32)
