"""How a recipe frames the photo before its steps run and cuts the picture out after them: the `Framing` model.

A `Framing` is a recipe's geometry, declared like an effect's parameters (every field has a unit, a range and a
description: `Framing.documentation()` lists them). The runner applies it to the photo, the subject matte and the depth
map together: `apply` before the first step, in one fixed order (flip, rotate, crop, pad; the recipe's `size` follows),
`cut` after the last one (a circle, rounded corners, or the subject as the picture's only opaque part). Effects never
see any of it: they keep working on a three-channel picture of any size.

Promises: the default `Framing()` changes nothing; every value is checked when the framing is made, a `RecipeError`
naming the field and the value wherever it came from (a `[frame]` table, Python); the same photo, matte and framing
always give the same pixels; the photo and its maps are never modified; all lengths are shares of the picture, so the
same look comes out at any resolution.

Dependency direction: imports `params`, `ops` and `errors` only.
"""

from typing import Any, Literal, overload

import numpy as np

from bullterrier_image.errors import SubjectNotFoundError
from bullterrier_image.ops import frame

from .params import Color, EffectParams, param

__all__ = ["Framing"]

_MATTE = 3  # the position of the subject matte, when given, among the channels it travels with (the depth is after it)


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
        min=-360,
        max=360,
        doc="Turn the photo about its centre, counter-clockwise when positive (-360 to 360). A quarter turn moves "
        "every pixel exactly; any other angle resamples the picture.",
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
        "opaque, 'background' makes the subject's matte the alpha (needs the subject), 'circle' keeps the circle "
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
    def needs_matte(self) -> bool:
        """True when the framing uses the subject: a crop around it or a cut that keeps only it."""
        return self.anchor == "subject" or self.transparent == "background"

    @property
    def has_alpha(self) -> bool:
        """True when `cut` gives the picture an alpha channel."""
        return self.transparent != "none"

    @overload
    def apply(
        self, rgb: np.ndarray, matte: np.ndarray, depth: np.ndarray
    ) -> tuple[np.ndarray, np.ndarray, np.ndarray]: ...

    @overload
    def apply(self, rgb: np.ndarray, matte: np.ndarray, depth: None = None) -> tuple[np.ndarray, np.ndarray, None]: ...

    @overload
    def apply(
        self, rgb: np.ndarray, matte: None = None, *, depth: np.ndarray
    ) -> tuple[np.ndarray, None, np.ndarray]: ...

    @overload
    def apply(
        self, rgb: np.ndarray, matte: np.ndarray | None = None, depth: np.ndarray | None = None
    ) -> tuple[np.ndarray, np.ndarray | None, np.ndarray | None]: ...

    def apply(
        self, rgb: np.ndarray, matte: np.ndarray | None = None, depth: np.ndarray | None = None
    ) -> tuple[np.ndarray, np.ndarray | None, np.ndarray | None]:
        """The photo `rgb` (rows, columns, 3), its subject `matte` and its `depth` (each rows, columns, or None) framed
        together: flipped, turned, cropped, padded, in that order, each map moving exactly as the photo does (a turn or
        a border leaves 0 where the picture is not: background, farthest). New arrays when anything changes, the
        inputs themselves when nothing does; the inputs are never modified. A crop around the subject follows the
        matte alone, never the depth; SubjectNotFoundError when it has no `matte` or no subject to crop around."""
        cropping = self.aspect is not None or self.anchor == "subject"
        padding = (self.pad_top, self.pad_right, self.pad_bottom, self.pad_left)
        if self.flip == "none" and not (self.angle or cropping or any(padding)):
            return rgb, matte, depth
        if self.anchor == "subject" and matte is None:
            raise SubjectNotFoundError("a crop around the subject (`anchor` 'subject') needs the subject's matte")
        maps = [layer[..., None] for layer in (matte, depth) if layer is not None]
        stack = np.concatenate([rgb, *maps], axis=2) if maps else rgb  # the maps ride along, the matte first
        fill = (*self.fill, 0.0, 0.0)[: stack.shape[2]]
        stack = frame.flip(stack, x=self.flip in ("x", "both"), y=self.flip in ("y", "both"))
        if self.angle:
            stack = frame.rotate(stack, self.angle, fit=self.fit, fill=fill, resample=self.resample)
        if cropping:
            stack = self._crop(stack)
        if any(padding):
            short = min(stack.shape[:2])
            top, right, bottom, left = (round(share * short) for share in padding)
            stack = frame.pad(stack, top=top, right=right, bottom=bottom, left=left, fill=fill)
        layers = (np.array(stack[..., channel], order="C") for channel in range(_MATTE, stack.shape[2]))
        framed = np.array(stack[..., :_MATTE], order="C")
        return framed, None if matte is None else next(layers), None if depth is None else next(layers)

    def cut(self, rgb: np.ndarray, matte: np.ndarray | None) -> np.ndarray:
        """`rgb` (rows, columns, 3) as a new RGBA picture (rows, columns, 4, straight alpha) cut out as `transparent`
        says; `matte` (the subject, the picture's size) is needed for 'background'. Only for a framing with
        `has_alpha`."""
        shape = rgb.shape[:2]
        match self.transparent:
            case "background":
                alpha = matte
            case "circle":
                alpha = frame.circle_alpha(shape)
            case "rounded":
                alpha = frame.rounded_alpha(shape, self.corner_radius * min(shape))
            case _:
                raise ValueError("a framing that is not `transparent` has nothing to cut")
        return np.concatenate([rgb, alpha[..., None]], axis=2)  # type: ignore[index]  # 'background' was given its matte

    def _crop(self, stack: np.ndarray) -> np.ndarray:
        shape = stack.shape[:2]
        if self.anchor == "subject":
            box = frame.subject_box(stack[..., _MATTE], self.margin)
            return frame.crop_around(shape, box, self.aspect).of(stack)
        return frame.crop_box(shape, self.aspect, self.anchor).of(stack)  # type: ignore[arg-type]  # `aspect` is set
