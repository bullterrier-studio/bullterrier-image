"""How a recipe frames the photo before its steps run and cuts the picture out after them: the `Framing` model.

A `Framing` is a recipe's geometry, declared like an effect's parameters (every field has a unit, a range and a
description: `Framing.documentation()` lists them). The core applies it to the photo and its maps (the subject, the
depth, any map handed in) together before the first step, in one fixed order (flip, rotate, crop, pad; the recipe's
`size` follows), and cuts the picture out after the last one (a circle, rounded corners, or the subject as the
picture's only opaque part). Effects never see any of it: they keep working on a three-channel picture of any size.

Promises: the default `Framing()` changes nothing; every value is checked when the framing is made, a `RecipeError`
naming the field and the value wherever it came from (a `[frame]` table, Python); all lengths are shares of the
picture, so the same look comes out at any resolution.

Dependency direction: imports `params` only.
"""

from typing import Any, Literal

from .params import EffectParams, param
from .values import Color

__all__ = ["Framing"]


class Framing(EffectParams):
    """How the photo is framed before the steps and cut out after them: flip, rotate, crop, pad, transparency."""

    flip: Literal["none", "x", "y", "both"] = param(
        "none",
        doc="Mirror the photo before anything else: 'none' leaves it, 'x' swaps left and right, 'y' swaps top and "
        "bottom, 'both' does both (a half turn).",
    )
    angle: float = param(
        0.0,
        unit="degrees",
        min=-180,
        max=180,
        cyclic=True,
        doc="Turn the photo about its centre, clockwise when positive (-180 to 180; any other angle is the same turn "
        "inside that range, 270 is -90). A quarter turn moves every pixel exactly; any other angle resamples the "
        "picture.",
    )
    fit: Literal["crop", "expand"] = param(
        "crop",
        advanced=True,
        doc="What a turn does to the frame: 'crop' keeps the largest rectangle of the photo's own aspect that the "
        "turned photo covers everywhere, so no `fill` shows (a quarter turn then shows a smaller part: use 'expand' to "
        "lay a photo on its side); 'expand' makes the picture as large as the whole turned photo needs and shows "
        "`fill` in the corners.",
    )
    resample: Literal["nearest", "bilinear", "bicubic"] = param(
        "bicubic",
        advanced=True,
        doc="How a turn by an angle that is not a quarter turn reads the photo between its pixels: 'nearest' is "
        "blocky and exact, 'bilinear' is smooth and soft, 'bicubic' is smooth and sharper.",
    )
    aspect: float | None = param(
        None,
        unit="ratio",
        min=0.1,
        max=10,
        doc="Crop to this width over height (1 is a square, 1.778 is 16:9, 0.8 is 4:5; 0.1 to 10), or leave the "
        "photo's own shape when not set. Where the crop sits is `anchor`.",
    )
    anchor: Literal["center", "top", "bottom", "left", "right", "subject"] = param(
        "center",
        doc="Where the crop sits: 'center' is the middle, 'top', 'bottom', 'left' and 'right' put it against that side "
        "(it can only move where the aspect leaves room), 'subject' is the smallest crop of the `aspect` that holds "
        "the subject and its `margin` (the whole subject's box when no `aspect` is set), centred on it and kept inside "
        "the photo. Needs the subject.",
    )
    margin: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="Room kept around the subject by a crop on the 'subject' anchor, as a share of the longer side of the "
        "subject's box (0 to 1; 0.1 is a tenth). Raising it shows more of the surroundings. Unused by other anchors.",
    )
    pad_top: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=2,
        doc="Border added above the cropped photo, in `fill`, as a share of the picture's shorter side (0 to 2). "
        "Raising it makes room above the subject.",
    )
    pad_right: float = param(
        0.0, unit="fraction", min=0, max=2, doc="Border added to the right, as `pad_top` (a share of the shorter side)."
    )
    pad_bottom: float = param(
        0.0, unit="fraction", min=0, max=2, doc="Border added below, as `pad_top` (a share of the shorter side)."
    )
    pad_left: float = param(
        0.0, unit="fraction", min=0, max=2, doc="Border added to the left, as `pad_top` (a share of the shorter side)."
    )
    fill: Color = param(
        "#000000",
        doc="Colour of the borders added by `pad_*` and of the corners a turn leaves with `fit` 'expand': '#rrggbb', "
        "'#rgb' or three numbers 0..1.",
    )
    transparent: Literal["none", "background", "circle", "rounded"] = param(
        "none",
        doc="Cut the finished picture out, so it has an alpha channel (a PNG or WebP, not a JPEG): 'none' keeps it "
        "opaque, 'background' makes the subject map the alpha (needs the subject), 'circle' keeps the circle "
        "inscribed in the picture, 'rounded' rounds its corners by `corner_radius`.",
    )
    corner_radius: float = param(
        0.2,
        unit="fraction",
        min=0,
        max=0.5,
        doc="How round the corners of `transparent` 'rounded' are, as a share of the picture's shorter side (0 to 0.5: "
        "0 is square, 0.5 is as round as the picture allows). Unused by the other cuts.",
    )

    def __init__(self, /, **values: Any) -> None:
        """The framing of `values` (field name -> value, all optional); RecipeError naming each bad field, the
        constraint and the value, and listing the fields when an unknown one is given."""
        self._init_checked("", **values)

    @property
    def has_alpha(self) -> bool:
        """True when the framing cuts the picture out after the last step, giving it an alpha channel (so a render
        of its recipe is RGBA, for a PNG or WebP, not a JPEG)."""
        return self.transparent != "none"
