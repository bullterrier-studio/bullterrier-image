//! A parameter that varies across the picture.

use serde_json::{Map, Value, json};

use crate::Error;

/// A parameter that varies across the picture: its value at a pixel is `start` where the map `by` is 0 and `end` where it
/// is 1, and in between along `levels` evenly spaced values.
///
/// A step writes it as the value of one of its effect's drivable parameters:
///
/// ```text
/// Step::new("bloom").param("radius", Drive::new("depth", 0.0, 30.0).levels(8))
/// ```
///
/// The effect is run once at each level the picture needs (so a driven step costs up to `levels` times the effect) and
/// every pixel takes the two runs either side of its map value, mixed linearly. In a recipe's JSON the drive is the
/// object `{"by": .., "start": .., "end": .., "levels": ..}`; building and reading check nothing but the shape, and
/// [`Recipe::checked`](crate::Recipe::checked) the rules: `by` names a map, `start` and `end` lie in the parameter's
/// range, `levels` is a whole number from 2 to 16, and all the drives of a step share one `by` and one `levels`.
#[derive(Clone, Debug, PartialEq)]
pub struct Drive {
    /// The name of the map that drives the parameter: `subject`, `depth`, `light` or a region.
    pub by: String,
    /// The parameter's value where the map is 0, in the parameter's own unit.
    pub start: f64,
    /// The parameter's value where the map is 1, in the parameter's own unit.
    pub end: f64,
    /// How many evenly spaced values between `start` and `end` the effect is run at (2 to 16): more is smoother and costs
    /// more runs; 2 mixes the effect at `start` with the effect at `end`.
    pub levels: u8,
}

/// The levels of a drive that writes none.
const DEFAULT_LEVELS: u8 = 5; // constant: the default `levels` of the Python `Drive`

impl Drive {
    /// The drive of a parameter from `start` where the map `by` is 0 to `end` where it is 1, at 5 levels.
    #[must_use]
    pub fn new(by: impl Into<String>, start: f64, end: f64) -> Self {
        Self {
            by: by.into(),
            start,
            end,
            levels: DEFAULT_LEVELS,
        }
    }

    /// This drive at `levels` levels (2 to 16, checked by `Recipe::checked`).
    #[must_use]
    pub const fn levels(mut self, levels: u8) -> Self {
        self.levels = levels;
        self
    }

    /// The parameter's value at level `level`: `start` at level 0, `end` exactly at the last level (a parameter at one of
    /// its bounds must not miss it by a rounding), evenly spaced between.
    #[must_use]
    pub fn value(&self, level: u8) -> f64 {
        let last = self.levels.saturating_sub(1);
        if level == last {
            self.end
        } else {
            self.start + (self.end - self.start) * f64::from(level) / f64::from(last)
        }
    }

    /// The drive a table writes, or what is wrong with it.
    pub(crate) fn read(table: &Map<String, Value>) -> Result<Self, String> {
        if let Some(key) = table
            .keys()
            .find(|key| !["by", "start", "end", "levels"].contains(&key.as_str()))
        {
            return Err(format!(
                "`{key}` is not a key of a drive; a drive has by, start, end and levels"
            ));
        }
        let number = |key: &str| match table.get(key) {
            None => Err(format!("`{key}` is required")),
            Some(value) => value
                .as_f64()
                .ok_or_else(|| format!("`{key}` must be a number, got {value}")),
        };
        let by = match table.get("by") {
            Some(Value::String(by)) if !by.is_empty() => by.clone(),
            other => {
                return Err(format!(
                    "`by` names the field that drives it, got {}",
                    other.unwrap_or(&Value::Null)
                ));
            }
        };
        let levels = match table.get("levels") {
            None => Some(DEFAULT_LEVELS),
            Some(value) => value.as_i64().and_then(|levels| u8::try_from(levels).ok()),
        };
        match levels {
            Some(levels @ 2..=16) => Ok(Self {
                by,
                start: number("start")?,
                end: number("end")?,
                levels,
            }),
            _ => Err(format!(
                "`levels` must be a whole number from 2 to 16, got {}",
                table.get("levels").unwrap_or(&Value::Null)
            )),
        }
    }
}

impl From<Drive> for Value {
    /// The drive as a recipe's JSON writes it, to be a step's parameter (`Step::param`).
    fn from(drive: Drive) -> Self {
        json!({"by": drive.by, "start": drive.start, "end": drive.end, "levels": drive.levels})
    }
}

/// The parameters in `params` that are drives (a table with `by`), with their names, once their shape and the sharing
/// of one `by` and one `levels` are checked.
pub(crate) fn in_params(effect: &str, params: &Map<String, Value>, place: &str) -> Result<Vec<(String, Drive)>, Error> {
    let mut drives = Vec::new();
    for (name, value) in params {
        let Some(table) = value.as_object().filter(|table| table.contains_key("by")) else {
            continue;
        };
        let drive = Drive::read(table)
            .map_err(|problem| Error::Recipe(format!("{place}: effect '{effect}': `{name}`: {problem}")))?;
        drives.push((name.clone(), drive));
    }
    if let [(_, first), rest @ ..] = drives.as_slice() {
        if rest.iter().any(|(_, other)| other.by != first.by) {
            return Err(Error::Recipe(format!(
                "{place}: effect '{effect}': the drives of one step must vary by one field"
            )));
        }
        if rest.iter().any(|(_, other)| other.levels != first.levels) {
            return Err(Error::Recipe(format!(
                "{place}: effect '{effect}': the drives of one step must vary by one number of levels"
            )));
        }
    }
    Ok(drives)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_drive_is_start_at_level_0_end_exactly_at_the_last_and_even_steps_between() {
        // from 10 to 30 in 5 levels: 10, 15, 20, 25, 30
        let drive = Drive::new("depth", 10.0, 30.0);
        let values: Vec<f64> = (0..5).map(|level| drive.value(level)).collect();
        assert_eq!(values, [10.0, 15.0, 20.0, 25.0, 30.0]);
        // downwards, in 2 levels: the two ends
        let falling = Drive::new("depth", 1.0, -1.0).levels(2);
        assert_eq!((falling.value(0), falling.value(1)), (1.0, -1.0));
        // 0.3 + (0.9 - 0.3) x 3 / 3 is 0.9000000000000001 in floating point: the last level is `end` itself
        let last = Drive::new("depth", 0.3, 0.9).levels(4);
        assert_eq!(last.value(3).to_bits(), 0.9_f64.to_bits());
    }

    #[test]
    fn a_drive_is_written_as_the_object_a_recipe_holds_and_read_back_equal() -> Result<(), String> {
        let drive = Drive::new("light", 0.25, 4.0).levels(8);
        let written = Value::from(drive.clone());
        assert_eq!(written, json!({"by": "light", "start": 0.25, "end": 4.0, "levels": 8}));
        let table = written.as_object().ok_or("an object")?;
        assert_eq!(Drive::read(table)?, drive);
        // `levels` left out is the default of 5
        let bare = json!({"by": "light", "start": 0, "end": 1});
        assert_eq!(Drive::read(bare.as_object().ok_or("an object")?)?.levels, 5);
        Ok(())
    }
}
