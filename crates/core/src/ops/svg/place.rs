//! Where things go: affine maps of the plane, and the map that fits a view box into a rectangle (SVG 1.1, 7.8).

use svgtypes::{Align, AspectRatio, Transform, ViewBox};

/// An affine map of the plane, `(x, y)` to `(a x + c y + e, b x + d y + f)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Affine {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl Affine {
    /// The map that leaves every point where it is.
    pub(super) const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// The map that moves every point by `(x, y)`.
    pub(super) const fn translate(x: f64, y: f64) -> Self {
        Self {
            e: x,
            f: y,
            ..Self::IDENTITY
        }
    }

    /// The map that does `inner` first and then this one.
    pub(super) fn after(&self, inner: &Self) -> Self {
        Self {
            a: self.a * inner.a + self.c * inner.b,
            b: self.b * inner.a + self.d * inner.b,
            c: self.a * inner.c + self.c * inner.d,
            d: self.b * inner.c + self.d * inner.d,
            e: self.a * inner.e + self.c * inner.f + self.e,
            f: self.b * inner.e + self.d * inner.f + self.f,
        }
    }

    /// Where the map takes the point `(x, y)`.
    pub(super) fn apply(&self, x: f64, y: f64) -> [f64; 2] {
        [self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f]
    }
}

impl From<Transform> for Affine {
    fn from(t: Transform) -> Self {
        Self {
            a: t.a,
            b: t.b,
            c: t.c,
            d: t.d,
            e: t.e,
            f: t.f,
        }
    }
}

/// A rectangle as `[left, top, width, height]`.
pub(super) type Rectangle = [f64; 4]; // constant: left, top, width, height

/// The map that fits the view box `view` into the rectangle `[left, top, width, height]`: scaled by `aspect` (to fit inside
/// or to cover, or stretched when it has no alignment) and moved to its alignment.
pub(super) fn fit(view: ViewBox, aspect: AspectRatio, target: Rectangle) -> Affine {
    let [left, top, width, height] = target;
    let (across, down) = (width / view.w, height / view.h);
    let (sx, sy) = match (aspect.align, aspect.slice) {
        (Align::None, _) => (across, down),
        (_, false) => (across.min(down), across.min(down)),
        (_, true) => (across.max(down), across.max(down)),
    };
    // the share of the free room that goes before the box: none, half or all of it
    let (before_x, before_y) = match aspect.align {
        Align::None | Align::XMinYMin => (0.0, 0.0),
        Align::XMidYMin => (0.5, 0.0),
        Align::XMaxYMin => (1.0, 0.0),
        Align::XMinYMid => (0.0, 0.5),
        Align::XMidYMid => (0.5, 0.5),
        Align::XMaxYMid => (1.0, 0.5),
        Align::XMinYMax => (0.0, 1.0),
        Align::XMidYMax => (0.5, 1.0),
        Align::XMaxYMax => (1.0, 1.0),
    };
    let room = [width - view.w * sx, height - view.h * sy];
    Affine {
        a: sx,
        d: sy,
        e: left + room[0] * before_x - view.x * sx,
        f: top + room[1] * before_y - view.y * sy,
        ..Affine::IDENTITY
    }
}
