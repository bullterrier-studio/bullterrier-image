//! The tables of the numbers of regions, the frame and a step's own that can be set by address: what [`Row`] says of each, with
//! the range its check holds it to (a test holds the two together).

use std::borrow::Cow;

use super::parameters::ParameterKind;

/// A number of a step, region or the frame, by the rest of its address.
#[derive(Clone)]
pub(crate) struct Row {
    pub(crate) field: &'static str,
    pub(crate) kind: ParameterKind,
    pub(crate) nullable: bool,
    pub(crate) unit: Cow<'static, str>,
    pub(crate) doc: Cow<'static, str>,
    pub(crate) whole: bool,
    pub(crate) cyclic: bool,
    pub(crate) min: f64,
    pub(crate) max: f64,
}

const fn real(field: &'static str, unit: &'static str, min: f64, max: f64, doc: &'static str) -> Row {
    Row {
        field,
        kind: ParameterKind::Number,
        nullable: false,
        unit: Cow::Borrowed(unit),
        doc: Cow::Borrowed(doc),
        whole: false,
        cyclic: false,
        min,
        max,
    }
}

const fn whole(field: &'static str, unit: &'static str, min: f64, max: f64, doc: &'static str) -> Row {
    Row {
        field,
        kind: ParameterKind::Number,
        nullable: false,
        unit: Cow::Borrowed(unit),
        doc: Cow::Borrowed(doc),
        whole: true,
        cyclic: false,
        min,
        max,
    }
}

const fn turn(field: &'static str, unit: &'static str, min: f64, max: f64, doc: &'static str) -> Row {
    Row {
        field,
        kind: ParameterKind::Number,
        nullable: false,
        unit: Cow::Borrowed(unit),
        doc: Cow::Borrowed(doc),
        whole: false,
        cyclic: true,
        min,
        max,
    }
}

/// The most a seed is: it is 32 bits.
const SEED: f64 = 4_294_967_295.0; // constant: u32::MAX

/// The numbers of the regions of each kind that can be set (the kind is the `kind` of its file), with the range
/// [`Region::check`](super::Region) holds each to: the table is for a sweep, and a test holds the two together.
pub(crate) const REGION_ROWS: &[(&str, &[Row])] = &[
    (
        "shards",
        &[
            whole(
                "count",
                "count",
                1.0,
                500.0,
                "How many shards (1 to 500): exactly this many seeds are scattered, all inside the `fit` selection when there is one; a picture too small to give each seed a pixel has fewer pieces. Raising it makes smaller shards.",
            ),
            real(
                "jitter",
                "fraction",
                0.0,
                1.0,
                "How far each seed strays from its place in a regular honeycomb, as a share of the spacing (0 to 1): 0 cuts a regular honeycomb of equal pieces, 1 lets every seed anywhere in its own place, an uneven scatter.",
            ),
            real(
                "focus",
                "ratio",
                0.0,
                5.0,
                "How much the shards crowd round `impact` (0 to 5): a seed at a share r of the way to the farthest corner moves to r to the power 1 + focus of it, so 0 leaves them spread evenly and 2.5 makes small splinters at the impact point and big pieces far from it, like cracked glass.",
            ),
            whole(
                "seed",
                "count",
                0.0,
                SEED,
                "Random seed for where the seeds lie: the same seed always cuts the same shards, another seed other ones.",
            ),
        ],
    ),
    (
        "path",
        &[whole(
            "samples",
            "count",
            1.0,
            16.0,
            "How many evenly spaced scanlines measure how much of a border pixel the path covers (1 to 16): 1 looks at the middle of the pixel row only, more give a smoother slanting border and cost more time.",
        )],
    ),
    (
        "svg",
        &[
            whole(
                "max_shapes",
                "count",
                1.0,
                100_000.0,
                "The most shapes the file may hold, counting what each `use` repeats (1 to 100000); a file with more is refused, which guards against a small file that draws a huge number of shapes.",
            ),
            whole(
                "max_kilobytes",
                "count",
                1.0,
                100_000.0,
                "The largest file to read, in kilobytes (1 to 100000); a bigger one is refused, reading no more than that.",
            ),
        ],
    ),
    (
        "subject",
        &[
            real(
                "context",
                "fraction",
                0.0,
                1.0,
                "How much of its surroundings the finder sees round a piece, as a share of the piece's own size added on each side (0 to 1); the view is clipped to the picture. Only the piece's own pixels count. Raising it shows the finder more context, so a subject cut by the piece's border is recognised; 0 shows the bare piece. Only matters with `per`.",
            ),
            real(
                "threshold",
                "fraction",
                0.0,
                1.0,
                "How sure the finder must be of a pixel for it to count as subject (0 to 1): a pixel it gives a value of this or less is dropped. Raising it keeps only what the finder is certain of and drops the faint subjects it invents in a plain piece, but also the uncertain parts of a real one; 0 keeps every pixel the finder gives. Only matters with `per`.",
            ),
        ],
    ),
    (
        "band",
        &[
            real(
                "low",
                "fraction",
                0.0,
                1.0,
                "The lowest value chosen fully (0 to 1, at most `high`): every pixel whose value lies from `low` to `high` is chosen. Raising it leaves out the lower values; 0 reaches down to the bottom of the scale.",
            ),
            real(
                "high",
                "fraction",
                0.0,
                1.0,
                "The highest value chosen fully (0 to 1): see `low`. Lowering it leaves out the higher values; 1 reaches up to the top of the scale.",
            ),
            real(
                "feather",
                "fraction",
                0.0,
                1.0,
                "How far beyond `low` and `high` the choice fades out, as a share of the scale (0 to 1): it falls linearly from 1 at the bound to 0 this far past it. Raising it softens the edge; 0 cuts it hard.",
            ),
        ],
    ),
    (
        "bands",
        &[whole(
            "count",
            "count",
            1.0,
            100.0,
            "How many equal ranges the scale is cut into (1 to 100). Band 0 is the highest, the last the lowest: by value, not in reading order. A value on a border goes to the lower band (0 to the lowest); a band no pixel lies in has no piece and the rest are renumbered, highest first. Raising it makes thinner bands.",
        )],
    ),
    (
        "components",
        &[
            real(
                "threshold",
                "fraction",
                0.0,
                1.0,
                "The value (0 to 1) from which a pixel of the selection is in a piece. Raising it keeps only what the selection is surer of; lowering it takes in more of a soft edge.",
            ),
            real(
                "min_area",
                "fraction",
                0.0,
                1.0,
                "The smallest piece kept, as a share of the frame's area (0 to 1): a smaller one joins the rest. Raising it drops specks; 0 keeps every piece, down to a single pixel.",
            ),
            whole(
                "max_pieces",
                "count",
                1.0,
                10_000.0,
                "The most pieces the selection may have after the small ones are dropped (1 to 10000); more is refused. Raising it allows a busier picture.",
            ),
        ],
    ),
    (
        "color_range",
        &[
            real(
                "tolerance",
                "fraction",
                0.0,
                1.0,
                "How far from `target` a pixel may be and still be chosen fully (0 to 1), as a share of the Oklab distance from black to white. Raising it takes in more shades of the colour; With `feather` 0, 0 chooses only the exact colour; with a feather, nearby shades are still partly chosen.",
            ),
            real(
                "feather",
                "fraction",
                0.0,
                1.0,
                "The extra distance beyond `tolerance` over which the selection falls linearly from 1 to 0 (0 to 1), in the same distance. Raising it softens the edge; 0 cuts hard.",
            ),
        ],
    ),
    (
        "linear_gradient",
        &[turn(
            "angle",
            "degrees",
            0.0,
            360.0,
            "The direction in which the value rises, clockwise from the right (0 to 360): 0 rises to the right, 90 downward, 180 to the left, 270 upward. The value is a share of the picture's extent along that direction, so the far corners are 0 and 1 at any aspect ratio.",
        )],
    ),
    (
        "radial_gradient",
        &[
            real(
                "x",
                "fraction",
                0.0,
                1.0,
                "Where the centre is across the picture, as a share of its width (0 to 1): 0 is the left edge, 1 the right.",
            ),
            real(
                "y",
                "fraction",
                0.0,
                1.0,
                "Where the centre is down the picture, as a share of its height (0 to 1): 0 is the top edge, 1 the bottom.",
            ),
            real(
                "radius",
                "fraction",
                0.01,
                3.0,
                "The distance from the centre at which the value reaches 1, as a share of the picture's shorter side (0.01 to 3). Raising it spreads the gradient wider; 0.5, centred, reaches the midpoints of the shorter sides' edges.",
            ),
        ],
    ),
];

/// The numbers of the frame that can be set, with the range [`Frame::check`](super::Frame) holds each to.
pub(crate) const FRAME_ROWS: &[Row] = &[
    turn(
        "angle",
        "degrees",
        -180.0,
        180.0,
        "Turn the photo about its centre, clockwise when positive (-180 to 180; any other angle is the same turn inside that range, 270 is -90). A quarter turn moves every pixel exactly; any other angle resamples the picture.",
    ),
    real(
        "aspect",
        "ratio",
        0.1,
        10.0,
        "Crop to this width over height (1 is a square, 1.778 is 16:9, 0.8 is 4:5; 0.1 to 10), or leave the photo's own shape when not set. Where the crop sits is `anchor`.",
    ),
    real(
        "margin",
        "fraction",
        0.0,
        1.0,
        "Room kept around the subject by a crop on the 'subject' anchor, as a share of the longer side of the subject's box (0 to 1; 0.1 is a tenth). Raising it shows more of the surroundings. Unused by other anchors.",
    ),
    real(
        "pad_top",
        "fraction",
        0.0,
        2.0,
        "Border added above the cropped photo, in `fill`, as a share of the picture's shorter side (0 to 2). Raising it makes room above the subject.",
    ),
    real(
        "pad_right",
        "fraction",
        0.0,
        2.0,
        "Border added to the right, as `pad_top` (a share of the shorter side).",
    ),
    real(
        "pad_bottom",
        "fraction",
        0.0,
        2.0,
        "Border added below, as `pad_top` (a share of the shorter side).",
    ),
    real(
        "pad_left",
        "fraction",
        0.0,
        2.0,
        "Border added to the left, as `pad_top` (a share of the shorter side).",
    ),
    real(
        "corner_radius",
        "fraction",
        0.0,
        0.5,
        "How round the corners of `transparent` 'rounded' are, as a share of the picture's shorter side (0 to 0.5: 0 is square, 0.5 is as round as the picture allows). Unused by the other cuts.",
    ),
];

/// The opacity of a step, and the numbers of an each step.
pub(super) const OPACITY: Row = real(
    "opacity",
    "fraction",
    0.0,
    1.0,
    "How much of the step shows: 0 none, 1 fully; when none is given, the effect's own default (`registry.opacity`, 1 for most effects, and for an `each` step).",
);
pub(super) const EACH_OPACITY: Row = real(
    "opacity",
    "fraction",
    0.0,
    1.0,
    "How much of the step shows: 0 none, 1 fully; when none is given, the effect's own default (`registry.opacity`, 1 for most effects, and for an `each` step).",
);
pub(super) const FEATHER: Row = real(
    "feather",
    "px",
    0.0,
    24.0,
    "How softly a piece's look fades into its neighbours' across a border, in px (0 to 24): the standard deviation of a Gaussian. 0 gives hard borders; about 0.5 only smooths the stair-steps of the pixels; raising it blends the looks over a wider band.",
);
pub(super) const EACH_SEED: Row = whole(
    "seed",
    "count",
    0.0,
    SEED,
    "Random seed of assign='random': the same seed always gives the same looks to the same pieces.",
);
pub(super) const LEVELS: Row = whole(
    "levels",
    "count",
    2.0,
    16.0,
    "How many evenly spaced values between `start` and `end` the effect is rendered at (2 to 16); each pixel mixes the two nearest. More is smoother and costs more renders; 2 mixes the effect at `start` and at `end`.",
); // constant: the range `Recipe::checked` holds `levels` to
