//! `disperse`: pieces of the picture detach one after another, drift on the wind, fade and go. What it does is documented
//! once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::cast::single;
use crate::ops::cells::Grid;
use crate::ops::edge::EdgeOrConstant;
use crate::ops::glyphs::cell_size;
use crate::ops::length::float;
use crate::ops::outward::{Measure, outward};
use crate::ops::random::{mix, unit};
use crate::params::{FieldName, Params};
use crate::{Footprint, Image, Inputs, Picture};

/// Where `disperse` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Subject,
    opacity: 1.0,
    inputs: &[],
};

/// What `disperse` reads around a pixel: the grid of cells is anchored at the picture's corner, the order pieces leave in
/// is scaled by the extremes over the whole figure, and the distance from its outline reaches across the picture.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// One moment of the picture dissolving into drifting pieces (evaporate, dissolve, disintegrate, wind-blown, halo).
///
/// `progress` 0 = untouched, 1 = finished. The presets `evaporate`, `halo`, `windblown` and `disintegrate` are stills of
/// those looks, part-way through. The picture is cut into character-sized pieces. Black is background, so use it on a
/// cut-out on black. Each piece has a fixed, random-ish fate set by `seed`: at a given `progress` it winds up, detaches,
/// drifts, wobbles, glows, fades or is gone, and the same `seed` keeps every piece's fate the same at every `progress`.
///
/// When a piece detaches is set by where it is (`top`, `outline`, `sweep`, with `jitter`); `reach` is how much of the
/// figure ever goes. Where it goes: on the wind (`wind`, `angle`), and away from its edge with `outward`. `hover` makes
/// the edges loosen all round first; `ambient` with `ambient_phase` makes the pieces that stay shimmer.
///
/// Three layers, as motion design has them. Primary: a wind front crosses the picture and pieces drift away on it.
/// Secondary: the pieces near the outline float up slowly first, each piece pulls back slightly before it goes
/// (anticipation) and glows briefly as it leaves. Ambient: the pieces that stay shimmer in a wave travelling against the
/// wind, set by `ambient_phase`. Every time-like parameter is a share of `progress`, so the look is the same however
/// `progress` is driven.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The figure that `outline` and `hover` work from: `subject` (the subject's silhouette), `light` (the bright
    /// parts), `depth` (the near parts), or a region of the recipe (text, a logo, a drawn shape). A piece belongs to it
    /// when the field's average over the piece is above `field_threshold`.
    #[param(default = "subject")]
    pub field: FieldName,
    /// Character width in px; a piece is one character (its height comes from the bundled font). Use the same value as
    /// the ascii step before this one, so whole characters move.
    #[param(default = 5, unit = px, min = 1, max = 64)]
    pub cell: usize,
    /// The moment shown: 0 = untouched, 1 = finished (every piece that goes has gone).
    #[param(default = 0.0, unit = fraction, min = 0, max = 1)]
    pub progress: f32,
    /// Weight of height in the order pieces leave: higher pieces go first. 0 = height does not matter.
    #[param(default = 1.0, unit = ratio, min = 0, max = 10)]
    pub top: f32,
    /// Weight of closeness to the outline (and the left, right and top borders) in the order: pieces near the edge go
    /// before the middle. 0 = closeness does not matter.
    #[param(default = 1.0, unit = ratio, min = 0, max = 10)]
    pub outline: f32,
    /// Weight of side in the order: positive starts at the left, negative at the right; its size is how much it matters
    /// next to `top` and `outline`.
    #[param(default = 0.0, unit = ratio, min = -1, max = 1)]
    pub sweep: f32,
    /// How much of the figure ever goes, in leaving order: 0.4 = the first 40% disperses and the rest stays, 1 =
    /// everything goes.
    #[param(default = 1.0, unit = fraction, min = 0, max = 1)]
    pub reach: f32,
    /// The farthest any piece is carried by the time it vanishes, in px: positive blows right, negative left. Keep it
    /// under a third of the picture width (160 px at the design size 480), or pieces cross too much of the picture in
    /// one go.
    #[param(default = 96.0, unit = px, min = -480, max = 480)]
    pub wind: f32,
    /// Direction the wind blows, in degrees clockwise from right: 0 = right, 90 = down, 180 = left, 270 = up (the
    /// default 350 is 10 degrees above level). A negative `wind` mirrors it left to right: the same tilt, blowing left.
    #[param(default = 350.0, unit = degrees, min = 0, max = 360, cyclic)]
    pub angle: f32,
    /// Share of its journey a piece floats slowly along the wind before it goes (about 0..0.3 looks right). The outline
    /// starts at once, deeper pieces later (see `hover_spread`). 0 = off.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1, advanced)]
    pub hover: f32,
    /// How much a piece is also thrown away from the edge it came from (by up to `outward_distance`); pieces then fly
    /// in different directions, so leave it at 0 when everything should travel together on the wind.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1, advanced)]
    pub outward: f32,
    /// Depth of the slow shimmer of the pieces that stay, as a share of their brightness (about 0..0.1 looks right;
    /// brightening stops at white). 0 = off.
    #[param(default = 0.0, unit = ratio, min = 0, max = 1, advanced)]
    pub ambient: f32,
    /// Random seed: each piece's fate is fixed by it, so one seed always gives the same pieces the same fates.
    #[param(default = 0, unit = count, min = 0, max = 2147483647)]
    pub seed: u32,
    /// Distance from the outline, in px, beyond which a piece counts as fully inside, for `outline` and `hover_spread`.
    /// Raising it makes closeness to the edge matter deeper in.
    #[param(default = 72.0, unit = px, min = 5, max = 480, advanced)]
    pub outline_depth: f32,
    /// Random share of each piece's place in the leaving order: 0 = a clean front, raising it makes it ragged.
    #[param(default = 0.2, unit = fraction, min = 0, max = 1, advanced)]
    pub jitter: f32,
    /// Wind-up time before a piece goes, as a share of `progress`: it eases back by `pullback` over this time. The
    /// first piece goes only after it, so every piece has one.
    #[param(default = 0.06, unit = fraction, min = 0.01, max = 0.2, advanced)]
    pub anticipation: f32,
    /// How far a piece eases back against its journey before it goes (anticipation), as a share of the journey. 0 = no
    /// wind-up.
    #[param(default = 0.15, unit = fraction, min = 0, max = 1, advanced)]
    pub pullback: f32,
    /// Average time from detaching to vanishing, as a share of `progress`. Shorter flights keep fewer pieces in the air
    /// at once (about 0.1 keeps it to a third on a side-to-side sweep).
    #[param(default = 0.2, unit = fraction, min = 0.01, max = 0.5, advanced)]
    pub flight: f32,
    /// How much each piece's flight differs from `flight`: 0.4 = from 0.6x to 1.4x. 0 = all the same.
    #[param(default = 0.4, unit = fraction, min = 0, max = 0.9, advanced)]
    pub flight_variation: f32,
    /// The progress by which the last piece has vanished (a little under 1 leaves nothing behind at the end). If
    /// `anticipation` and the longest flight do not fit before it, all pieces go at once.
    #[param(default = 0.98, unit = fraction, min = 0.5, max = 1, advanced)]
    pub finish: f32,
    /// With `hover`: how far into `progress` a piece `outline_depth` inside starts to float (the outline starts at 0).
    /// 0 = everything floats at once.
    #[param(default = 0.45, unit = fraction, min = 0, max = 1, advanced)]
    pub hover_spread: f32,
    /// Height of the gentle up-and-down of a hovering piece, in px.
    #[param(default = 2.0, unit = px, min = 0, max = 24, advanced)]
    pub bob: f32,
    /// Number of ups and downs while a piece hovers.
    #[param(default = 3.0, unit = cycles, min = 0, max = 20, advanced)]
    pub bob_cycles: f32,
    /// Range of each piece's own direction around the wind's `angle`, in degrees (it differs by up to half of this).
    /// Not a direction itself.
    #[param(default = 12.0, unit = degrees, min = 0, max = 180, advanced)]
    pub angle_spread: f32,
    /// The shortest journey as a share of the longest (`wind`), so speeds differ; 1 = all travel equally far.
    #[param(default = 0.35, unit = fraction, min = 0, max = 1, advanced)]
    pub distance_minimum: f32,
    /// How far `outward` 1 throws a piece away from its edge, in px.
    #[param(default = 58.0, unit = px, min = 0, max = 480, advanced)]
    pub outward_distance: f32,
    /// How a piece speeds up on its way (an exit accelerates): it ends 1 + 2 x acceleration times as fast as it starts.
    /// 0 = steady speed.
    #[param(default = 0.5, unit = ratio, min = 0, max = 10, advanced)]
    pub acceleration: f32,
    /// Largest sideways wobble of a flying piece, in px; it grows during the flight. 0 = none.
    #[param(default = 6.0, unit = px, min = 0, max = 96, advanced)]
    pub sway: f32,
    /// Number of wobbles during a piece's flight.
    #[param(default = 1.5, unit = cycles, min = 0, max = 20, advanced)]
    pub sway_cycles: f32,
    /// How a flying piece fades: brightness is (1 - share of flight)^falloff. 0 = no fade (it just vanishes at the
    /// end), 1 = even, above 1 fades quickly at first.
    #[param(default = 1.5, unit = ratio, min = 0, max = 10, advanced)]
    pub falloff: f32,
    /// Extra brightness at the peak of the glow as a piece leaves (0.4 = 1.4x); brightening stops at white. 0 = no
    /// glow.
    #[param(default = 0.4, unit = ratio, min = 0, max = 2, advanced)]
    pub flare: f32,
    /// Share of a piece's flight that the glow lasts; it peaks halfway through that.
    #[param(default = 0.2, unit = fraction, min = 0.01, max = 1, advanced)]
    pub flare_span: f32,
    /// Where the shimmer wave is in its cycle: 0 and 1 are the same picture, 0.5 is half a cycle on. Raising it moves
    /// the wave against the wind; with `ambient` at 0 it does nothing.
    #[param(default = 0.0, unit = cycles, min = 0, max = 1, advanced)]
    pub ambient_phase: f32,
    /// Length of one shimmer wave, in px; a shorter one packs more waves across the picture.
    #[param(default = 96.0, unit = px, min = 5, max = 960, advanced)]
    pub ambient_wavelength: f32,
    /// True: the bottom border is not an edge for `outline` and `hover` (a portrait is cut off there). False: it counts
    /// like the other borders.
    #[param(default = true, advanced)]
    pub open_bottom: bool,
    /// Blur, in characters (a Gaussian sigma), of the outline before `outward` reads its direction: raising it makes
    /// neighbouring pieces fly in more similar directions; 0 uses the raw outline.
    #[param(default = 1.0, unit = count, min = 0, max = 10, advanced)]
    pub normal_smoothing: f32,
    /// Where the `normal_smoothing` blur is cut off, in sigmas: it reaches this many times `normal_smoothing`
    /// characters from the outline, so the pieces' direction `outward` is read from that far out. Larger is more exact
    /// and slower, smaller shortens the reach. Nothing changes with `normal_smoothing` at 0.
    #[param(default = 4.0, unit = ratio, min = 1, max = 10, advanced)]
    pub normal_truncate: f32,
    /// What the `normal_smoothing` blur sees beyond the picture's border: 'reflect' (mirrored, border row repeated; a
    /// border stays an edge), 'mirror' (mirrored about the border row), 'nearest' (the border row repeated), 'wrap'
    /// (the opposite border), 'constant' (open space, as if nothing were there, which blurs the outline toward the
    /// border).
    #[param(default = Reflect, advanced)]
    pub edge: EdgeOrConstant,
    /// The field's average over a piece, 0 to 1, above which the piece belongs to the figure that `outline` and `hover`
    /// work from; 0.5 = more than half. Lowering it takes in more of a soft edge (hair, fur), raising it less.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1, advanced)]
    pub field_threshold: f32,
    /// How bright, 0 to 1, some pixel of a piece must be for the piece to count as picture rather than black
    /// background: a piece no brighter never flies and is drawn black once the dispersal (or the shimmer) has begun.
    /// Raising it drops dim, noisy pieces.
    #[param(default = 0.02, unit = fraction, min = 0, max = 1, advanced)]
    pub background_threshold: f32,
}

// constant: the numbers of the six random draws that make a piece's fate, fixed so that a seed keeps its look
const DRAW_ORDER: u64 = 0;
const DRAW_FLIGHT: u64 = 1;
const DRAW_SPEED: u64 = 2;
const DRAW_ANGLE: u64 = 3;
const DRAW_SWAY: u64 = 4;
const DRAW_PHASE: u64 = 5;

/// A piece's `part`-th random number, 0..1: a hash of the seed and its place, so that it is the same at every `progress`.
fn noise(seed: u32, cell: usize, part: u64) -> f64 {
    unit(mix(seed, &[cell as u64, part])) // constant: a cell number fits 64 bits
}

/// The position `at` (0..1) in `low..high`.
fn lerp(low: f64, high: f64, at: f64) -> f64 {
    low + (high - low) * at
}

/// The smoothstep polynomial 3t^2 - 2t^3 of `t` in 0..1.
fn smooth(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t) // constant: smoothstep polynomial 3t^2 - 2t^3
}

/// Where in the leaving order each piece is, 0 = first, 1 = last: a mix of its height, its closeness to the outline and
/// its side, spread to the extremes of the pieces that count and then loosened by `jitter`.
fn leaving_order(params: &Params, grid: &Grid, present: &[bool], depth: &[f32], size: (usize, usize)) -> Vec<f64> {
    let (top, outline, sweep) = (f64::from(params.top), f64::from(params.outline), f64::from(params.sweep));
    let total = top + outline + sweep.abs();
    let mut order: Vec<f64> = (0..grid.len())
        .map(|cell| {
            if total <= 0.0 {
                return 0.0;
            }
            let (row, column) = (cell / grid.columns, cell % grid.columns);
            let y = (float(row) + 0.5) * float(grid.cell_height) / float(size.0);
            let x = (float(column) + 0.5) * float(grid.cell_width) / float(size.1);
            let closeness = (f64::from(depth[cell]) / f64::from(params.outline_depth)).clamp(0.0, 1.0);
            let from_side = if sweep >= 0.0 { x } else { 1.0 - x };
            (top * y + outline * closeness + sweep.abs() * from_side) / total
        })
        .collect();
    let counted = || order.iter().zip(present).filter(|(_, here)| **here).map(|(value, _)| *value);
    let (low, high) = counted().fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), value| {
        (low.min(value), high.max(value))
    });
    let jitter = f64::from(params.jitter);
    for (cell, value) in order.iter_mut().enumerate() {
        if low <= high {
            *value = (*value - low) / (high - low).max(1e-6); // constant: guards a flat order
        }
        *value = (*value * (1.0 - jitter) + noise(params.seed, cell, DRAW_ORDER) * jitter).clamp(0.0, 1.0);
    }
    order
}

/// A piece that has left its place: where it has got to and how bright it is.
struct Mover {
    cell: usize,
    /// Rows and columns moved, whole px.
    shift: (isize, isize),
    brightness: f32,
}

/// `value` as whole px, halves to the even one.
#[expect(clippy::cast_possible_truncation, reason = "a shift in px; the cast saturates")]
fn whole_shift(value: f64) -> isize {
    value.round_ties_even() as isize
}

/// Where one piece is in its fate.
struct Fate {
    cell: usize,
    /// <= 0: not yet gone, 0..1: in flight, >= 1: gone.
    age: f64,
    /// When it detaches, in `progress` units.
    detach_at: f64,
    /// When it starts to float.
    hover_start: f64,
    /// The wind's direction in degrees, 0..360.
    heading: f64,
    /// The unit vector out of the figure at its place.
    direction: [f32; 2],
}

/// How a piece that is winding up or in flight has moved and how bright it is.
fn fly(params: &Params, fate: &Fate) -> Mover {
    let Fate {
        cell,
        age,
        detach_at,
        hover_start,
        heading,
        ..
    } = *fate;
    let progress = f64::from(params.progress);
    let anticipation = f64::from(params.anticipation);
    let wind = f64::from(params.wind);
    // Primary layer: pieces that are winding up or in flight.
    let t = age.clamp(0.0, 1.0); // how far through its flight
    let speed = lerp(f64::from(params.distance_minimum), 1.0, noise(params.seed, cell, DRAW_SPEED));
    let direction = (heading + (0.5 - noise(params.seed, cell, DRAW_ANGLE)) * f64::from(params.angle_spread)).to_radians();
    let journey = wind.abs() * speed; // this piece's whole journey
    let thrown = f64::from(params.outward) * f64::from(params.outward_distance);
    let [normal_x, normal_y] = fate.direction.map(f64::from);
    let journey_x = wind.signum() * journey * direction.cos() + thrown * normal_x;
    let journey_y = journey * direction.sin() + thrown * normal_y;
    let acceleration = f64::from(params.acceleration);
    // an exit: starts gently, speeds up, ends at 1
    let path = (t + acceleration * t * t) / (1.0 + acceleration);
    // Before it goes: float slowly along the journey (hover), then ease back a little against it (anticipation).
    let hover_time = (detach_at - hover_start).max(1e-9); // constant: guards a zero hover
    let floating = ((progress - hover_start) / hover_time).clamp(0.0, 1.0);
    let windup = ((progress - (detach_at - anticipation)) / anticipation).clamp(0.0, 1.0);
    let (hover, pullback) = (f64::from(params.hover), f64::from(params.pullback));
    let along = if age > 0.0 {
        let start = hover - pullback; // where flight begins, as a share of the journey
        start + (1.0 - start) * path
    } else {
        hover * smooth(floating) - pullback * (1.0 - (1.0 - windup) * (1.0 - windup))
    };
    let phase = noise(params.seed, cell, DRAW_PHASE);
    let tau = 2.0 * std::f64::consts::PI; // constant: a full turn in radians
    // the bob only exists while pieces hover
    let bob = if params.hover > 0.0 && age <= 0.0 {
        f64::from(params.bob)
            * (std::f64::consts::PI * floating).sin()
            * (tau * (f64::from(params.bob_cycles) * floating + phase)).sin()
    } else {
        0.0
    };
    let sway = f64::from(params.sway)
        * noise(params.seed, cell, DRAW_SWAY)
        * t
        * (tau * (f64::from(params.sway_cycles) * t + phase)).sin();
    // Secondary layer: a brief glow just after leaving, then the fade.
    let glow = 1.0 + f64::from(params.flare) * (std::f64::consts::PI * (t / f64::from(params.flare_span)).clamp(0.0, 1.0)).sin();
    let dimmed = if age > 0.0 { (1.0 - t).powf(f64::from(params.falloff)) } else { 1.0 };
    #[expect(clippy::cast_possible_truncation, reason = "a brightness in f32")]
    Mover {
        cell,
        shift: (whole_shift(along * journey_y + bob), whole_shift(along * journey_x + sway)),
        brightness: (dimmed * glow) as f32,
    }
}

/// The picture with its pieces in the state `progress` says.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let pixels = picture.pixels();
    // `Inputs::declared_by` refuses an offer that lacks a declared field, so this branch is out of reach: `render` cannot
    // return an error, and a field cannot be read without an Option
    let Some(field) = inputs.field("field") else {
        return Image::from_pixels(height, width, pixels.to_vec());
    };
    if params.progress <= 0.0 && params.ambient <= 0.0 {
        return Image::from_pixels(height, width, pixels.to_vec());
    }
    let (cell_width, cell_height) = cell_size(params.cell);
    let grid = Grid::new((height, width), (cell_width, cell_height));
    let present = grid.lit(pixels, params.background_threshold);
    let figure = grid.figure(field.values(), params.field_threshold);
    let edges = outward(
        &figure,
        (grid.rows, grid.columns),
        &Measure {
            sampling: (float(cell_height), float(cell_width)),
            open_bottom: params.open_bottom,
            smoothing: f64::from(params.normal_smoothing),
            truncate: f64::from(params.normal_truncate),
            edge: params.edge.extension(0.0), // constant: open space beyond the border is nothing
        },
    );
    let order = leaving_order(params, &grid, &present, &edges.distance, (height, width));

    let progress = f64::from(params.progress);
    let (anticipation, flight, variation) = (
        f64::from(params.anticipation),
        f64::from(params.flight),
        f64::from(params.flight_variation),
    );
    // the last piece has vanished by `finish`
    let span = (f64::from(params.finish) - anticipation - flight * (1.0 + variation)).max(0.0);
    let reach = f64::from(params.reach);
    let wind = f64::from(params.wind);
    let heading = f64::from(params.angle).rem_euclid(360.0); // constant: a turn, so any real angle is a direction
    let against = if wind >= 0.0 { 1.0 } else { -1.0 }; // constant: the sign of a direction

    let mut out = vec![[0.0_f32; 3]; height * width];
    let mut movers = Vec::new();
    for cell in (0..grid.len()).filter(|&cell| present[cell]) {
        let column = cell % grid.columns;
        let depth = (f64::from(edges.distance[cell]) / f64::from(params.outline_depth)).clamp(0.0, 1.0);
        let lifetime = flight * ((1.0 - variation) + 2.0 * variation * noise(params.seed, cell, DRAW_FLIGHT));
        let detach_at = anticipation + order[cell] / reach.max(1e-6) * span; // constant: guards reach == 0
        let age = (progress - detach_at) / lifetime; // <= 0: not yet, 0..1: in flight, >= 1: gone
        let goes = order[cell] <= reach;
        let mut hover_start = detach_at - anticipation;
        if params.hover > 0.0 {
            // the outline starts to float at once, deeper pieces later (but never later than that wind-up)
            hover_start = hover_start.min(depth * f64::from(params.hover_spread));
        }
        let hover_start = hover_start.max(0.0);
        let winding = goes && age <= 0.0 && progress > hover_start;
        let flying = goes && age > 0.0 && age < 1.0;
        let staying = (!goes || age <= 0.0) && !winding;
        if staying {
            // Ambient layer: a slow wave through the pieces that stay, moving against the wind.
            let shimmer = if params.ambient > 0.0 {
                let x = (float(column) + 0.5) * float(cell_width);
                let phase = f64::from(params.ambient_phase) + against * x / f64::from(params.ambient_wavelength);
                1.0 + f64::from(params.ambient) * (2.0 * std::f64::consts::PI * phase).sin()
            } else {
                1.0
            };
            for (y, x) in grid.pixels(cell) {
                let pixel = pixels[y * width + x];
                out[y * width + x] = pixel.map(|level| single((f64::from(level) * shimmer).clamp(0.0, 1.0)));
            }
        }
        if !(winding || flying) {
            continue;
        }
        movers.push(fly(
            params,
            &Fate {
                cell,
                age,
                detach_at,
                hover_start,
                heading,
                direction: edges.direction[cell],
            },
        ));
    }
    // The glow and the shimmer brighten; on what is already white they would leave the 0..1 range, so they stop there.
    for mover in &movers {
        for (y, x) in grid.pixels(mover.cell) {
            let (Some(to_y), Some(to_x)) = (y.checked_add_signed(mover.shift.0), x.checked_add_signed(mover.shift.1)) else {
                continue;
            };
            if to_y >= height || to_x >= width {
                continue;
            }
            let shaded = pixels[y * width + x].map(|level| (level * mover.brightness).min(1.0));
            let target = &mut out[to_y * width + to_x];
            *target = [0, 1, 2].map(|channel| target[channel].max(shaded[channel]));
        }
    }
    Image::from_pixels(height, width, out)
}
