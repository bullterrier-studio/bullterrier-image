//! The dominant colours of a picture: the one place that finds them, for [`palette`] and for `dither`'s `photo` palette.

use std::borrow::Cow;
use std::cmp::Ordering;

use crate::ops::cluster::{Point, Start, kmeans, mean, nearest};
use crate::ops::colorspace::rgb_to_oklab;
use crate::ops::random::{Rng, mix};
use crate::params::Scalar;
use serde_json::Value;

use crate::{Constraint, Error, Picture};

/// The most colours a palette holds.
const MOST_COLORS: i64 = 256; // constant: k-means labels a pixel with one byte
/// The name of the stream that picks the sample dots; the k-means starts from the stream of the seed itself.
const SAMPLING: u64 = 1; // constant: names the stream of the sample, any number but the seed's own

// What `palette` leaves unasked: the defaults of the `dither` parameters of the same names (documented there).
const ROUNDS: usize = 20; // constant: dither's `iterations` default
const TOLERANCE: f32 = 0.001; // constant: dither's `tolerance` default
const SAMPLE: usize = 10_000; // constant: dither's `sample` default

/// How a search for colours is run: what `dither` exposes as parameters and `palette` keeps fixed.
pub(crate) struct Search {
    /// How many colours to look for.
    pub(crate) colors: usize,
    /// How the first colours are chosen.
    pub(crate) start: Start,
    /// The most k-means rounds.
    pub(crate) rounds: usize,
    /// Stop once no colour moved farther than this in a round.
    pub(crate) tolerance: f32,
    /// How many randomly chosen dots the colours are searched on; a picture with fewer is searched whole.
    pub(crate) sample: usize,
    /// Seeds the choice of sample dots and first colours.
    pub(crate) seed: u32,
}

/// The colours found among `dots` (sRGB; `matched` is the same dots in the space the search measures in) and how many dots
/// are nearest to each: the mean of those dots, in the order of the k-means centres. A centre no dot is nearest to is left
/// out, so there are fewer than `search.colors` when the dots have fewer distinct colours.
pub(crate) fn found(dots: &[Point], matched: &[Point], search: &Search) -> Vec<(Point, usize)> {
    let searched: Cow<'_, [Point]> = if search.sample < matched.len() {
        let stream = (mix(search.seed, &[SAMPLING]) >> 32) as u32; // constant: the hash's upper half
        let chosen = Rng::seeded(stream).sample(matched.len(), search.sample);
        Cow::Owned(chosen.into_iter().map(|at| matched[at]).collect())
    } else {
        Cow::Borrowed(matched)
    };
    let centres = kmeans(
        &searched,
        search.colors,
        search.start,
        search.rounds,
        search.tolerance,
        search.seed,
    );
    let mut sums = vec![[0.0_f64; 3]; centres.len()];
    let mut members = vec![0_usize; centres.len()];
    for (dot, label) in dots.iter().zip(nearest(matched, &centres)) {
        for (total, &value) in sums[usize::from(label)].iter_mut().zip(dot) {
            *total += f64::from(value);
        }
        members[usize::from(label)] += 1;
    }
    sums.iter()
        .zip(&members)
        .filter(|(_, count)| **count > 0)
        .map(|(sum, &count)| (mean(sum, count), count))
        .collect()
}

/// The `colors` dominant colours of `image`, as sRGB `[red, green, blue]` in 0..1, **largest share of the pixels first**.
///
/// The colours are found by clustering the pixels in Oklab, where equal distances look equally different; each colour is the
/// mean of the pixels nearest to its cluster, so a flat block of colour comes back exactly. Of equal shares the lighter
/// colour comes first (higher Oklab lightness, then the lower red, green, blue). Fewer than `colors` come back when the
/// picture has fewer distinct colours. The same picture, `colors` and `seed` always give the same palette on the same
/// machine; another `seed` may find slightly different colours on a picture with more colours than `colors`.
///
/// # Errors
/// [`Error::Invalid`] (kind [`ErrorKind::Recipe`](crate::ErrorKind::Recipe)) naming `colors` and its value unless it is
/// from 1 to 256, and naming `seed` and its value unless it is from 0 to 4294967295.
pub fn palette(image: &Picture<'_>, colors: i64, seed: i128) -> Result<Vec<[f32; 3]>, Error> {
    let invalid = |field: &str, constraint, value: Value| Error::Invalid {
        place: None,
        effect: "palette".to_owned(),
        field: field.to_owned(),
        constraint,
        value: Box::new(value),
    };
    if colors < 1 {
        return Err(invalid("colors", Constraint::AtLeast(Scalar::Int(1)), colors.into()));
    }
    let Some(colors) = usize::try_from(colors).ok().filter(|_| colors <= MOST_COLORS) else {
        return Err(invalid(
            "colors",
            Constraint::AtMost(Scalar::Int(MOST_COLORS)),
            colors.into(),
        ));
    };
    // a number beyond i64 is shown in floating point: it only has to name the value that was refused
    #[expect(clippy::cast_precision_loss, reason = "the value is only shown, in the refusal")]
    let shown = || i64::try_from(seed).map_or_else(|_| Value::from(seed as f64), Value::from);
    let seed = match u32::try_from(seed) {
        Ok(seed) => seed,
        Err(_) if seed < 0 => return Err(invalid("seed", Constraint::AtLeast(Scalar::Int(0)), shown())),
        Err(_) => {
            return Err(invalid(
                "seed",
                Constraint::AtMost(Scalar::Int(i64::from(u32::MAX))),
                shown(),
            ));
        }
    };
    let dots = image.pixels();
    let matched: Vec<Point> = dots.iter().map(|&pixel| rgb_to_oklab(pixel)).collect();
    let search = Search {
        colors,
        start: Start::Spread,
        rounds: ROUNDS,
        tolerance: TOLERANCE,
        sample: SAMPLE,
        seed,
    };
    let mut shares = found(dots, &matched, &search);
    shares.sort_by(|(a, share_a), (b, share_b)| {
        share_b
            .cmp(share_a)
            .then_with(|| rgb_to_oklab(*b)[0].total_cmp(&rgb_to_oklab(*a)[0]))
            .then_with(|| {
                a.iter()
                    .zip(b)
                    .map(|(x, y)| x.total_cmp(y))
                    .find(|o| o.is_ne())
                    .unwrap_or(Ordering::Equal)
            })
    });
    Ok(shares.into_iter().map(|(colour, _)| colour).collect())
}
