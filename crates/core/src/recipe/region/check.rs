//! The ranges of the fields of a [`Region`]: the rules [`Recipe::checked`](crate::Recipe::checked) applies to each region
//! before it looks at the steps.

use super::super::check::outside;
use super::laid::{MOST_CUTS, MOST_SHARES, MOST_TOTAL};
use super::{Cut, Edge, Piece, Region, Regions, Shares};
use crate::Error;
use crate::ops::outlines;

/// The most pieces a partition made of a grid may have, and the most a `Pick` may list: each piece is a number in a
/// label map and a unit of work for a step that goes piece by piece (an `each` look, a subject found in each piece).
const MOST_PIECES: usize = 10_000; // constant: 100 x 100, a finer mosaic than a picture of 480 px can show

/// The most regions a `Combine` puts together: one pass over the picture for each.
const MOST_COMBINED: usize = MOST_SHARES;

impl Region {
    /// Whether every number of this region, the declared region `name` (one of `declared`), lies within its range (see
    /// the field's documentation), and every piece a `Pick` names by `id` is of an `Svg` region.
    ///
    /// What is not checked here: that a name the region refers to is a region, and an SVG file (`Svg.file`) and a shard
    /// preset (`Shards.preset`), which the runner reads.
    ///
    /// # Errors
    /// [`Error::Recipe`] naming the region, the field (`edge.amount`), the value and the rule.
    pub(crate) fn check(&self, name: &str, declared: &Regions) -> Result<(), Error> {
        self.check_shape(name)?;
        let Self::Pick(pick) = self else { return Ok(()) };
        let at = Field { region: name };
        // only an svg has ids, and a render would find that out at the step that uses the pick
        let id = pick.pieces.iter().find_map(|piece| match piece {
            Piece::Id(id) => Some(id),
            Piece::Number(_) => None,
        });
        match (id, declared.get(&pick.of)) {
            (Some(id), found) if !matches!(found, Some(Self::Svg(_))) => Err(at.refuse(&format!(
                "`pieces` names the id '{id}', but '{}' is not an svg region",
                pick.of
            ))),
            _ => Ok(()),
        }
    }

    /// Whether every number of this region, called `name` in its message, lies within its range (see the field's
    /// documentation): the rules that need no other region. What it names (`of`, `per`, `fit`) and the ids a `Pick` lists are
    /// not looked up: those are the recipe's.
    ///
    /// # Errors
    /// [`Error::Recipe`] naming the region, the field (`edge.amount`), the value and the rule.
    pub fn check_shape(&self, name: &str) -> Result<(), Error> {
        let at = Field { region: name };
        match self {
            Self::Grid(grid) => {
                at.edge(&grid.edge)?;
                at.shares("columns", &grid.columns)?;
                at.shares("rows", &grid.rows)?;
                // each is at most MOST_SHARES, so the product cannot overflow
                let pieces = grid.columns.0.len() * grid.rows.0.len();
                if pieces > MOST_PIECES {
                    return Err(at.refuse(&format!(
                        "`columns` x `rows` make {pieces} pieces, more than the limit of {MOST_PIECES}"
                    )));
                }
                Ok(())
            }
            Self::Shards(shards) => {
                at.edge(&shards.edge)?;
                at.within("count", shards.count, 1, 500)?;
                at.within("jitter", shards.jitter, 0, 1)?;
                at.within("impact[0]", shards.impact[0], 0, 1)?;
                at.within("impact[1]", shards.impact[1], 0, 1)?;
                at.within("focus", shards.focus, 0, 5)
            }
            Self::Cuts(cuts) => {
                at.edge(&cuts.edge)?;
                at.count("lines", cuts.lines.len(), 1, MOST_CUTS)?;
                for (index, cut) in cuts.lines.iter().enumerate() {
                    match cut {
                        Cut::Line(line) => {
                            at.within(&format!("lines[{index}].x"), line.x, 0, 1)?;
                            at.within(&format!("lines[{index}].y"), line.y, 0, 1)?;
                            at.finite(&format!("lines[{index}].angle"), line.angle)?;
                        }
                        Cut::Curve(curve) => match outlines::check_syntax(&curve.path) {
                            Ok(1) => {}
                            Ok(subpaths) => {
                                return Err(at.refuse(&format!(
                                    "`lines[{index}].path` must be one path with a single M (a cut is one curve from border to \
                                     border), got {subpaths} subpaths"
                                )));
                            }
                            Err(said) => return Err(at.refuse(&format!("`lines[{index}].path`: {said}"))),
                        },
                    }
                }
                Ok(())
            }
            Self::Path(path) => {
                at.edge(&path.edge)?;
                at.within("samples", path.samples, 1, 16)?;
                outlines::check_syntax(&path.d)
                    .map(drop)
                    .map_err(|said| at.refuse(&format!("`d`: {said}")))
            }
            Self::Svg(svg) => {
                at.edge(&svg.edge)?;
                at.within("max_shapes", svg.max_shapes, 1, 100_000)?;
                at.within("max_kilobytes", svg.max_kilobytes, 1, 100_000)
            }
            Self::Pick(pick) => at.count("pieces", pick.pieces.len(), 1, MOST_PIECES),
            Self::Subject(subject) => {
                at.within("context", subject.context, 0, 1)?;
                at.within("threshold", subject.threshold, 0, 1)
            }
            Self::Band(band) => {
                at.within("low", band.low, 0, 1)?;
                at.within("high", band.high, 0, 1)?;
                at.within("feather", band.feather, 0, 1)?;
                if band.low > band.high {
                    return Err(at.refuse(&format!(
                        "`low` {} is above `high` {}: low must be at most high",
                        band.low, band.high
                    )));
                }
                Ok(())
            }
            Self::Bands(bands) => at.within("count", bands.count, 1, 100),
            Self::Combine(combine) => at.count("of", combine.of.len(), 2, MOST_COMBINED),
            Self::LinearGradient(gradient) => at.finite("angle", gradient.angle),
            Self::RadialGradient(gradient) => {
                at.within("x", gradient.x, 0, 1)?;
                at.within("y", gradient.y, 0, 1)?;
                at.within("radius", gradient.radius, 0.01, 3)
            }
            Self::Given(_) => Ok(()),
        }
    }
}

/// The declared region whose fields are being checked, and the way to refuse one of them.
struct Field<'a> {
    region: &'a str,
}

impl Field<'_> {
    /// An error about this region.
    fn refuse(&self, said: &str) -> Error {
        Error::Recipe(format!("region '{}': {said}", self.region))
    }

    /// `value`, the number `field`, if it is finite.
    fn finite(&self, field: &str, value: f64) -> Result<(), Error> {
        self.within(field, value, f64::NEG_INFINITY, f64::INFINITY)
    }

    /// `value`, the number `field`, if it is finite and lies from `min` to `max`, both inclusive.
    fn within(
        &self,
        field: &str,
        value: impl Into<f64>,
        min: impl Into<f64>,
        max: impl Into<f64>,
    ) -> Result<(), Error> {
        match outside(value.into(), min.into(), max.into()) {
            None => Ok(()),
            Some(said) => Err(self.refuse(&format!("`{field}` {said}"))),
        }
    }

    /// `number`, how many things the list `field` holds, if it lies from `min` to `max`, both inclusive.
    fn count(&self, field: &str, number: usize, min: usize, max: usize) -> Result<(), Error> {
        if number < min {
            Err(self.refuse(&format!("`{field}` needs at least {min} entries, got {number}")))
        } else if number > max {
            Err(self.refuse(&format!("`{field}` allows at most {max} entries, got {number}")))
        } else {
            Ok(())
        }
    }

    /// 1 to 1000 positive, finite shares that add up to at most [`MOST_TOTAL`].
    fn shares(&self, field: &str, shares: &Shares) -> Result<(), Error> {
        let positive = shares.0.iter().all(|share| *share > 0.0 && share.is_finite());
        if (1..=MOST_SHARES).contains(&shares.0.len()) && positive {
            if shares.0.iter().sum::<f64>() <= MOST_TOTAL {
                return Ok(());
            }
            Err(self.refuse(&format!(
                "`{field}` shares must add up to at most {MOST_TOTAL:e}, got {:?}",
                shares.0
            )))
        } else {
            Err(self.refuse(&format!(
                "`{field}` must be a count or a list of positive numbers, 1 to {MOST_SHARES} of them, got {:?}",
                shares.0
            )))
        }
    }

    /// The numbers of a border.
    fn edge(&self, edge: &Edge) -> Result<(), Error> {
        self.within("edge.amount", edge.amount, 0, 48)?;
        self.within("edge.scale", edge.scale, 1, 480)?;
        self.within("edge.octaves", edge.octaves, 1, 8)?;
        self.within("edge.persistence", edge.persistence, 0, 1)
    }
}
