//! The ranges of the fields of a [`Region`]: the rules [`Recipe::checked`](crate::Recipe::checked) applies to each region
//! before it looks at the steps.

use std::fmt::Display;

use super::laid::{MOST_CUTS, MOST_SHARES, MOST_TOTAL};
use super::{Cut, Edge, Region, Shares};
use crate::Error;

impl Region {
    /// Whether every number of this region, the declared region `name`, lies within its range (see the field's
    /// documentation).
    ///
    /// What is not checked here: that a name the region refers to is a region, and the syntax of a path (`Path.d`,
    /// `Curve.path`), an SVG file (`Svg.file`) and a shard preset (`Shards.preset`), which the runner reads.
    ///
    /// # Errors
    /// [`Error::Recipe`] naming the region, the field (`edge.amount`), the value and the rule.
    pub(crate) fn check(&self, name: &str) -> Result<(), Error> {
        let at = Field { region: name };
        match self {
            Self::Grid(grid) => {
                at.edge(&grid.edge)?;
                at.shares("columns", &grid.columns)?;
                at.shares("rows", &grid.rows)
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
                    if let Cut::Line(line) = cut {
                        at.within(&format!("lines[{index}].x"), line.x, 0, 1)?;
                        at.within(&format!("lines[{index}].y"), line.y, 0, 1)?;
                        at.finite(&format!("lines[{index}].angle"), line.angle)?;
                    }
                }
                Ok(())
            }
            Self::Path(path) => {
                at.edge(&path.edge)?;
                at.within("samples", path.samples, 1, 16)
            }
            Self::Svg(svg) => {
                at.edge(&svg.edge)?;
                at.within("max_shapes", svg.max_shapes, 1, 100_000)?;
                at.within("max_kilobytes", svg.max_kilobytes, 1, 100_000)
            }
            Self::Pick(pick) => at.count("pieces", pick.pieces.len(), 1, usize::MAX),
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
            Self::Combine(combine) => at.count("of", combine.of.len(), 2, usize::MAX),
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
        if value.is_finite() {
            Ok(())
        } else {
            Err(self.refuse(&format!("`{field}` must be a finite number, got {value}")))
        }
    }

    /// `value`, the number `field`, if it is finite and lies from `min` to `max`, both inclusive.
    fn within<T: Into<f64> + Display + Copy>(
        &self,
        field: &str,
        value: T,
        min: impl Into<f64> + Display + Copy,
        max: impl Into<f64> + Display + Copy,
    ) -> Result<(), Error> {
        self.finite(field, value.into())?;
        if value.into() < min.into() {
            Err(self.refuse(&format!("`{field}` must be >= {min}, got {value}")))
        } else if value.into() > max.into() {
            Err(self.refuse(&format!("`{field}` must be <= {max}, got {value}")))
        } else {
            Ok(())
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

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::super::{Band, Cuts, Grid, Line, LinearGradient, Region, Shares};
    use crate::{Error, Presets, Recipe, Step};

    /// The refusal of the recipe that declares `region` as `r` (and a grid `p`, to pick from) and has one step (no step
    /// uses it), or None.
    fn refusal(region: Region) -> Option<String> {
        let recipe = Recipe::new([Step::new("grade")])
            .with_region("p", Grid::default())
            .with_region("r", region);
        match recipe.checked(&Presets::new()) {
            Err(Error::Recipe(message)) => Some(message),
            other => other.err().map(|error| error.to_string()),
        }
    }

    /// `kind` of the JSON `fields`, read as a region. A kind that must name regions names ones that exist (`p`, `depth`
    /// and `subject`) unless the row says otherwise, so that only its numbers are in question.
    fn region(kind: &str, fields: &Value) -> Region {
        let mut table = fields.clone();
        table["kind"] = json!(kind);
        let existing = match kind {
            "pick" => json!("p"),
            "band" | "bands" => json!("depth"),
            "combine" => json!(["subject", "depth"]),
            _ => Value::Null,
        };
        if !existing.is_null() && table.get("of").is_none() {
            table["of"] = existing;
        }
        serde_json::from_value(table).unwrap_or_else(|error| {
            Region::Grid(Grid {
                fit: Some(error.to_string()),
                ..Grid::default()
            })
        })
    }

    /// (the kind, the fields, what the refusal says; none: accepted). For each number the range, both ends and the first
    /// value beyond each, written in the form `` `field` must be >= min, got value ``. The bounds are the Python
    /// declarations' (`regions.py`), copied here by hand.
    #[expect(clippy::too_many_lines, reason = "a table of rows, one bound each")]
    fn rows() -> Vec<(&'static str, Value, Option<&'static str>)> {
        let edge = |field: &str, value: Value| json!({"edge": {field: value}});
        vec![
            // the borders of every laid kind: `grid` stands for them
            ("grid", edge("amount", json!(0)), None),
            ("grid", edge("amount", json!(48)), None),
            (
                "grid",
                edge("amount", json!(-0.1)),
                Some("`edge.amount` must be >= 0, got -0.1"),
            ),
            (
                "grid",
                edge("amount", json!(48.5)),
                Some("`edge.amount` must be <= 48, got 48.5"),
            ),
            ("grid", edge("scale", json!(1)), None),
            ("grid", edge("scale", json!(480)), None),
            (
                "grid",
                edge("scale", json!(0.5)),
                Some("`edge.scale` must be >= 1, got 0.5"),
            ),
            (
                "grid",
                edge("scale", json!(481)),
                Some("`edge.scale` must be <= 480, got 481"),
            ),
            ("grid", edge("octaves", json!(1)), None),
            ("grid", edge("octaves", json!(8)), None),
            (
                "grid",
                edge("octaves", json!(0)),
                Some("`edge.octaves` must be >= 1, got 0"),
            ),
            (
                "grid",
                edge("octaves", json!(9)),
                Some("`edge.octaves` must be <= 8, got 9"),
            ),
            ("grid", edge("persistence", json!(0)), None),
            ("grid", edge("persistence", json!(1)), None),
            (
                "grid",
                edge("persistence", json!(-0.25)),
                Some("`edge.persistence` must be >= 0, got -0.25"),
            ),
            (
                "grid",
                edge("persistence", json!(1.25)),
                Some("`edge.persistence` must be <= 1, got 1.25"),
            ),
            ("shards", edge("amount", json!(49)), Some("`edge.amount` must be <= 48")),
            ("cuts", edge("amount", json!(49)), Some("`edge.amount` must be <= 48")),
            ("path", edge("amount", json!(49)), Some("`edge.amount` must be <= 48")),
            ("svg", edge("amount", json!(49)), Some("`edge.amount` must be <= 48")),
            // grid: 1 to 1000 positive shares
            ("grid", json!({"columns": [1.0], "rows": [0.001]}), None),
            ("grid", json!({"columns": [5e99, 5e99]}), None),
            (
                "grid",
                json!({"columns": [1e308, 1e308]}),
                Some("`columns` shares must add up to at most 1e100"),
            ),
            (
                "grid",
                json!({"rows": [1e307, 1e307]}),
                Some("`rows` shares must add up to at most 1e100"),
            ),
            (
                "grid",
                json!({"rows": [6e99, 5e99]}),
                Some("`rows` shares must add up to at most 1e100"),
            ),
            ("grid", json!({"columns": vec![1; 1000]}), None),
            ("grid", json!({"rows": vec![1; 1000]}), None),
            (
                "grid",
                json!({"columns": vec![1; 1001]}),
                Some("`columns` must be a count or a list"),
            ),
            (
                "grid",
                json!({"rows": vec![1; 1001]}),
                Some("`rows` must be a count or a list"),
            ),
            (
                "grid",
                json!({"columns": []}),
                Some("`columns` must be a count or a list"),
            ),
            ("grid", json!({"rows": []}), Some("`rows` must be a count or a list")),
            (
                "grid",
                json!({"columns": [1, 0]}),
                Some("`columns` must be a count or a list"),
            ),
            (
                "grid",
                json!({"rows": [2, -1]}),
                Some("`rows` must be a count or a list"),
            ),
            // shards
            ("shards", json!({"count": 1}), None),
            ("shards", json!({"count": 500}), None),
            ("shards", json!({"count": 0}), Some("`count` must be >= 1, got 0")),
            ("shards", json!({"count": 501}), Some("`count` must be <= 500, got 501")),
            ("shards", json!({"jitter": 0}), None),
            ("shards", json!({"jitter": 1}), None),
            (
                "shards",
                json!({"jitter": -0.5}),
                Some("`jitter` must be >= 0, got -0.5"),
            ),
            ("shards", json!({"jitter": 1.5}), Some("`jitter` must be <= 1, got 1.5")),
            ("shards", json!({"impact": [0, 1]}), None),
            ("shards", json!({"impact": [1, 0]}), None),
            (
                "shards",
                json!({"impact": [-0.5, 0.5]}),
                Some("`impact[0]` must be >= 0, got -0.5"),
            ),
            (
                "shards",
                json!({"impact": [1.5, 0.5]}),
                Some("`impact[0]` must be <= 1, got 1.5"),
            ),
            (
                "shards",
                json!({"impact": [0.5, -0.5]}),
                Some("`impact[1]` must be >= 0, got -0.5"),
            ),
            (
                "shards",
                json!({"impact": [0.5, 1.5]}),
                Some("`impact[1]` must be <= 1, got 1.5"),
            ),
            ("shards", json!({"focus": 0}), None),
            ("shards", json!({"focus": 5}), None),
            ("shards", json!({"focus": -0.5}), Some("`focus` must be >= 0, got -0.5")),
            ("shards", json!({"focus": 5.5}), Some("`focus` must be <= 5, got 5.5")),
            // cuts: 1 to 32 of them; a line's point within the frame
            ("cuts", json!({"lines": vec![json!({}); 32]}), None),
            (
                "cuts",
                json!({"lines": vec![json!({}); 33]}),
                Some("`lines` allows at most 32 entries, got 33"),
            ),
            (
                "cuts",
                json!({"lines": []}),
                Some("`lines` needs at least 1 entries, got 0"),
            ),
            ("cuts", json!({"lines": [{"x": 0, "y": 1}]}), None),
            ("cuts", json!({"lines": [{"x": 1, "y": 0}]}), None),
            (
                "cuts",
                json!({"lines": [{"x": -0.5}]}),
                Some("`lines[0].x` must be >= 0, got -0.5"),
            ),
            (
                "cuts",
                json!({"lines": [{}, {"x": 1.5}]}),
                Some("`lines[1].x` must be <= 1, got 1.5"),
            ),
            (
                "cuts",
                json!({"lines": [{"y": -0.5}]}),
                Some("`lines[0].y` must be >= 0, got -0.5"),
            ),
            (
                "cuts",
                json!({"lines": [{"y": 1.5}]}),
                Some("`lines[0].y` must be <= 1, got 1.5"),
            ),
            ("cuts", json!({"lines": [{"angle": 725}, {"angle": -90}]}), None),
            ("cuts", json!({"lines": [{"path": "M 0 0 L 1 1"}]}), None),
            // path, svg
            ("path", json!({"samples": 1}), None),
            ("path", json!({"samples": 16}), None),
            ("path", json!({"samples": 0}), Some("`samples` must be >= 1, got 0")),
            ("path", json!({"samples": 17}), Some("`samples` must be <= 16, got 17")),
            ("svg", json!({"max_shapes": 1, "max_kilobytes": 100_000}), None),
            ("svg", json!({"max_shapes": 100_000, "max_kilobytes": 1}), None),
            (
                "svg",
                json!({"max_shapes": 0}),
                Some("`max_shapes` must be >= 1, got 0"),
            ),
            (
                "svg",
                json!({"max_shapes": 100_001}),
                Some("`max_shapes` must be <= 100000, got 100001"),
            ),
            (
                "svg",
                json!({"max_kilobytes": 0}),
                Some("`max_kilobytes` must be >= 1, got 0"),
            ),
            (
                "svg",
                json!({"max_kilobytes": 100_001}),
                Some("`max_kilobytes` must be <= 100000, got 100001"),
            ),
            // pick: at least one piece
            ("pick", json!({"pieces": [0]}), None),
            (
                "pick",
                json!({"pieces": []}),
                Some("`pieces` needs at least 1 entries, got 0"),
            ),
            // subject
            ("subject", json!({"context": 0, "threshold": 1}), None),
            ("subject", json!({"context": 1, "threshold": 0}), None),
            (
                "subject",
                json!({"context": -0.5}),
                Some("`context` must be >= 0, got -0.5"),
            ),
            (
                "subject",
                json!({"context": 1.5}),
                Some("`context` must be <= 1, got 1.5"),
            ),
            (
                "subject",
                json!({"threshold": -0.5}),
                Some("`threshold` must be >= 0, got -0.5"),
            ),
            (
                "subject",
                json!({"threshold": 1.5}),
                Some("`threshold` must be <= 1, got 1.5"),
            ),
            // band: three shares of the scale and low at most high
            ("band", json!({"low": 0, "high": 1, "feather": 0}), None),
            ("band", json!({"low": 1, "high": 1, "feather": 1}), None),
            ("band", json!({"low": 0.5, "high": 0.5}), None),
            ("band", json!({"low": -0.5}), Some("`low` must be >= 0, got -0.5")),
            (
                "band",
                json!({"low": 1.5, "high": 1}),
                Some("`low` must be <= 1, got 1.5"),
            ),
            (
                "band",
                json!({"high": -0.5, "low": 0}),
                Some("`high` must be >= 0, got -0.5"),
            ),
            ("band", json!({"high": 1.5}), Some("`high` must be <= 1, got 1.5")),
            (
                "band",
                json!({"feather": -0.5}),
                Some("`feather` must be >= 0, got -0.5"),
            ),
            ("band", json!({"feather": 1.5}), Some("`feather` must be <= 1, got 1.5")),
            (
                "band",
                json!({"low": 0.75, "high": 0.5}),
                Some("`low` 0.75 is above `high` 0.5"),
            ),
            (
                "band",
                json!({"low": 0.5, "high": 0.4999}),
                Some("`low` 0.5 is above `high` 0.4999"),
            ),
            // bands, combine
            ("bands", json!({"count": 1}), None),
            ("bands", json!({"count": 100}), None),
            ("bands", json!({"count": 0}), Some("`count` must be >= 1, got 0")),
            ("bands", json!({"count": 101}), Some("`count` must be <= 100, got 101")),
            ("combine", json!({"of": ["subject", "depth"]}), None),
            ("combine", json!({"of": ["subject", "depth", "light"]}), None),
            (
                "combine",
                json!({"of": ["subject"]}),
                Some("`of` needs at least 2 entries, got 1"),
            ),
            (
                "combine",
                json!({"of": []}),
                Some("`of` needs at least 2 entries, got 0"),
            ),
            // gradients
            ("linear_gradient", json!({"angle": 0}), None),
            ("linear_gradient", json!({"angle": 360}), None),
            ("linear_gradient", json!({"angle": 725}), None),
            ("linear_gradient", json!({"angle": -90}), None),
            ("radial_gradient", json!({"x": 0, "y": 1}), None),
            ("radial_gradient", json!({"x": 1, "y": 0}), None),
            (
                "radial_gradient",
                json!({"x": -0.5}),
                Some("`x` must be >= 0, got -0.5"),
            ),
            ("radial_gradient", json!({"x": 1.5}), Some("`x` must be <= 1, got 1.5")),
            (
                "radial_gradient",
                json!({"y": -0.5}),
                Some("`y` must be >= 0, got -0.5"),
            ),
            ("radial_gradient", json!({"y": 1.5}), Some("`y` must be <= 1, got 1.5")),
            ("radial_gradient", json!({"radius": 0.01}), None),
            ("radial_gradient", json!({"radius": 3}), None),
            (
                "radial_gradient",
                json!({"radius": 0.005}),
                Some("`radius` must be >= 0.01, got 0.005"),
            ),
            (
                "radial_gradient",
                json!({"radius": 3.5}),
                Some("`radius` must be <= 3, got 3.5"),
            ),
            ("given", json!({}), None),
        ]
    }

    #[test]
    fn every_number_of_a_region_lies_within_its_range_both_ends_included() {
        for (kind, fields, said) in rows() {
            let refused = refusal(region(kind, &fields));
            match said {
                None => assert_eq!(refused, None, "{kind} {fields} is within its ranges"),
                Some(said) => assert!(
                    refused
                        .as_deref()
                        .is_some_and(|message| message.starts_with("region 'r': ") && message.contains(said)),
                    "{kind} {fields}: wanted {said:?}, got {refused:?}"
                ),
            }
        }
    }

    #[test]
    fn a_number_that_is_not_finite_is_refused_where_a_file_cannot_write_it() {
        let nan = f64::NAN;
        for (what, region, field) in [
            (
                "a share that is NaN",
                Region::Grid(Grid {
                    columns: Shares(vec![1.0, nan]),
                    ..Grid::default()
                }),
                "columns",
            ),
            (
                "an infinite share",
                Region::Grid(Grid {
                    rows: Shares(vec![f64::INFINITY]),
                    ..Grid::default()
                }),
                "rows",
            ),
            (
                "a band that is NaN",
                Region::Band(Band {
                    low: nan,
                    ..Band::default()
                }),
                "low",
            ),
            (
                "a feather that is infinite",
                Region::Band(Band {
                    feather: f64::INFINITY,
                    ..Band::default()
                }),
                "feather",
            ),
            (
                "an angle that is NaN in a line",
                Region::Cuts(Cuts {
                    lines: vec![
                        Line {
                            angle: nan,
                            ..Line::default()
                        }
                        .into(),
                    ],
                    ..Cuts::default()
                }),
                "lines[0].angle",
            ),
            (
                "an infinite gradient angle",
                Region::LinearGradient(LinearGradient { angle: f64::INFINITY }),
                "angle",
            ),
        ] {
            let refused = refusal(region);
            assert!(
                refused
                    .as_deref()
                    .is_some_and(|message| message.contains(field)
                        && (message.contains("finite") || message.contains("positive"))),
                "{what}: {refused:?}"
            );
        }
    }

    #[test]
    fn the_regions_are_checked_before_the_steps_and_whether_or_not_a_step_uses_them() {
        let bad = Region::Bands(super::super::Bands {
            count: 0,
            ..super::super::Bands::default()
        });
        // the step's effect does not exist either, and the region is named first
        let recipe = Recipe::new([Step::new("nope")]).with_region("unused", bad);
        let refused = recipe.checked(&Presets::new()).err().map(|error| error.to_string());
        assert!(
            refused
                .as_deref()
                .is_some_and(|message| message.contains("region 'unused'")),
            "{refused:?}"
        );
    }

    #[test]
    fn of_several_regions_that_break_a_rule_the_first_declared_is_named() {
        let bands = |count| {
            Region::Bands(super::super::Bands {
                count,
                ..super::super::Bands::default()
            })
        };
        let recipe = Recipe::new([Step::new("grade")])
            .with_region("zed", bands(0))
            .with_region("amy", bands(101));
        let refused = recipe.checked(&Presets::new()).err().map(|error| error.to_string());
        assert!(
            refused
                .as_deref()
                .is_some_and(|message| message.contains("region 'zed'")),
            "{refused:?}"
        );
    }
}
