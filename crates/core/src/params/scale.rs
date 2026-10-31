//! Design pixels to working pixels: the same look at any size.

use serde_json::{Map, Value};

use super::{Field, Kind, Scalar, Table, Unit};

impl Table {
    /// `params` (complete and checked, as [`Table::check`] returns them) for a picture of `ratio` working pixels per
    /// design pixel (the working size over 480): the same look at another size.
    ///
    /// Every pixel value (a field with the unit `px`) is multiplied by `ratio`; every `density` (marks per 1000 design
    /// px squared) is divided by `ratio` squared, as an area grows with the square of a length. A whole number stays
    /// whole: it is rounded, halves to the even number (as Python's `round` does). When shrinking (a ratio below 1) a
    /// pixel value stops at the field's `min`, which keeps a small picture valid (a cell of at least 3 pixels);
    /// growing has no limit at either end, since a range is in design pixels and the result is not checked against
    /// it. Every other unit is unchanged, and a ratio of 1 changes nothing.
    pub(crate) fn scaled(&self, params: Map<String, Value>, ratio: f64) -> Map<String, Value> {
        params
            .into_iter()
            .map(
                |(name, value)| match self.fields.iter().find(|field| field.name == name) {
                    Some(field) => (name, field.scaled(value, ratio)),
                    None => (name, value),
                },
            )
            .collect()
    }
}

impl Field {
    /// `value` of this field for a picture of `ratio` working pixels per design pixel.
    fn scaled(&self, value: Value, ratio: f64) -> Value {
        let Some(number) = value.as_f64() else {
            return value;
        };
        let scaled = match self.unit {
            Some(Unit::Px) => number * ratio,
            Some(Unit::Density) => number / (ratio * ratio),
            _ => return value,
        };
        let scaled = match self.kind {
            Kind::Int => scaled.round_ties_even(),
            _ => scaled,
        };
        let scaled = match (self.unit, self.min.and_then(Scalar::number)) {
            (Some(Unit::Px), Some(min)) if ratio < 1.0 => scaled.max(min),
            _ => scaled,
        };
        match self.kind {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "the value is whole; a ratio so large that it leaves the i64 range is no picture"
            )]
            Kind::Int => Value::from(scaled as i64),
            _ => Value::from(scaled),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::{Map, Value, json};

    use crate::params::Declared;
    use crate::registry::Registry;
    use crate::testing::{Measured, Recorder, registry};
    use crate::{Error, Options, Photo, Picture, Presets, Recipe, Step};

    const RATIO: &[crate::Input] = &[crate::Input::SizeRatio];

    /// A registry with a `measure` effect that describes `Measured` and records what it receives, and a `loose` one
    /// that declares the size ratio and describes no parameters.
    fn measuring() -> Result<(Registry, Arc<Recorder>, Arc<Recorder>), Error> {
        let registry = registry()?;
        let measure = Arc::new(Recorder::new("measure", &[], &[]).with_table(Measured::table()));
        let loose = Arc::new(Recorder::new("loose", RATIO, &[]));
        registry.register(measure.clone())?;
        registry.register(loose.clone())?;
        Ok((registry, measure, loose))
    }

    /// Renders `step` at `ratio` working pixels per design pixel, so that `received` of the effect's recorder reads the
    /// parameters it reached the effect with. The ratio is that of the picture, its longest side over 480: a picture
    /// one row high and `480 x ratio` pixels wide.
    fn render_at(registry: &Registry, step: Step, ratio: f64) -> Result<(), Error> {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the ratios of the tests are small"
        )]
        let width = (480.0 * ratio) as usize;
        let values = vec![0.5_f32; width * 3];
        Photo::new(Picture::new(1, width, &values)?).render_in(
            registry,
            &Recipe::new([step]),
            &Presets::new(),
            Options::default(),
        )?;
        Ok(())
    }

    /// The value of `field` that a `measure` step with `written` for it reaches its effect with at `ratio`.
    fn measured(field: &str, written: &Value, ratio: f64) -> Result<Value, Error> {
        let (registry, measure, _) = measuring()?;
        render_at(&registry, Step::new("measure").param(field, written.clone()), ratio)?;
        let received = measure.received();
        Ok(received[0].params[field].clone())
    }

    #[test]
    fn a_pixel_value_is_multiplied_by_the_ratio_and_a_density_divided_by_its_square() -> Result<(), Error> {
        // (ratio, field, written, the value the effect receives): a decimal stays a decimal, a whole number a whole
        // number; 10 x 2 = 20, 10 x 1.5 = 15, 10 x 0.5 = 5; 8 / 2^2 = 2, 8 / 1.5^2 = 3.5555.., 8 / 0.5^2 = 32, 8 / 3^2 = 0.888..
        let rows = [
            (2.0, "length", json!(10.0), json!(20.0)),
            (1.5, "length", json!(10.0), json!(15.0)),
            (0.5, "length", json!(10.0), json!(5.0)),
            (2.0, "cell", json!(10), json!(20)),
            (0.5, "cell", json!(10), json!(5)),
            (2.0, "marks", json!(8.0), json!(2.0)),
            (1.5, "marks", json!(8.0), json!(8.0 / 2.25)),
            (0.5, "marks", json!(8.0), json!(32.0)),
            // growing may take a density below its minimum of 2: only a pixel value is held at its minimum
            (3.0, "marks", json!(8.0), json!(8.0 / 9.0)),
        ];
        for (ratio, field, written, expected) in rows {
            let received = measured(field, &written, ratio)?;
            assert_eq!(received, expected, "{field} {written} at {ratio}");
        }
        Ok(())
    }

    #[test]
    fn every_other_unit_is_left_as_it_is() -> Result<(), Error> {
        for (field, written) in [
            ("copies", json!(7)),
            ("share", json!(0.25)),
            ("turn", json!(45.0)),
            ("gain", json!(3.0)),
            ("stops", json!(-1.5)),
            ("laps", json!(2.5)),
        ] {
            for ratio in [0.5, 1.0, 3.0] {
                assert_eq!(measured(field, &written, ratio)?, written, "{field} at {ratio}");
            }
        }
        Ok(())
    }

    #[test]
    fn a_whole_number_of_pixels_is_rounded_halves_to_the_even_number() -> Result<(), Error> {
        // 11 x 0.5 = 5.5 -> 6 and 9 x 0.5 = 4.5 -> 4 (to the even); 7 x 1.5 = 10.5 -> 10 and 5 x 1.5 = 7.5 -> 8;
        // 10 x 1.26 = 12.6 -> 13; 10 x 1.24 = 12.4 -> 12
        for (ratio, written, expected) in [
            (0.5, 11, 6),
            (0.5, 9, 4),
            (1.5, 7, 10),
            (1.5, 5, 8),
            (1.26, 10, 13),
            (1.24, 10, 12),
        ] {
            assert_eq!(
                measured("cell", &json!(written), ratio)?,
                json!(expected),
                "{written} at {ratio}"
            );
        }
        Ok(())
    }

    #[test]
    fn shrinking_stops_at_the_minimum_and_growing_has_no_limit() -> Result<(), Error> {
        // length: min 2, max 100; cell: min 3, max 100. 3 x 0.5 = 1.5 -> 2; 10 x 0.1 = 1 -> 2; 5 x 0.5 = 2.5 -> 2.5 (kept)
        // cell 5 x 0.5 = 2.5 -> rounds to 2 -> 3; 4 x 0.1 = 0.4 -> 0 -> 3
        // 100 x 3 = 300 and 100 x 1.5 = 150, past the declared maximum of 100
        let rows = [
            (0.5, "length", json!(3.0), json!(2.0)),
            (0.1, "length", json!(10.0), json!(2.0)),
            (0.5, "length", json!(5.0), json!(2.5)),
            (0.5, "cell", json!(5), json!(3)),
            (0.1, "cell", json!(4), json!(3)),
            (3.0, "length", json!(100.0), json!(300.0)),
            (1.5, "cell", json!(100), json!(150)),
            // shift: min -50. Growing never stops at the minimum: -40 x 2 = -80, past -50; 20 x 2 = 40
            (2.0, "shift", json!(-40.0), json!(-80.0)),
            (2.0, "shift", json!(20.0), json!(40.0)),
            // shrinking moves a negative value towards 0, away from the minimum: -40 x 0.5 = -20, -10 x 0.1 = -1
            (0.5, "shift", json!(-40.0), json!(-20.0)),
            (0.1, "shift", json!(-10.0), json!(-1.0)),
            // a ratio of 1 changes nothing, whatever the minimum: length 2 stays 2, shift -50 stays -50
            (1.0, "length", json!(2.0), json!(2.0)),
            (1.0, "shift", json!(-50.0), json!(-50.0)),
            // growing a positive value from the minimum: length 2 x 3 = 6
            (3.0, "length", json!(2.0), json!(6.0)),
        ];
        for (ratio, field, written, expected) in rows {
            let received = measured(field, &written, ratio)?;
            assert_eq!(received, expected, "{field} {written} at {ratio}");
        }
        Ok(())
    }

    #[test]
    fn a_ratio_of_one_leaves_every_declared_default_as_it_is() -> Result<(), Error> {
        let (registry, measure, _) = measuring()?;
        render_at(&registry, Step::new("measure"), 1.0)?;
        render_at(&registry, Step::new("measure"), 2.0)?;
        let received = measure.received();
        let at_one = received[0].params.as_object().cloned().unwrap_or_default();
        let at_two = received[1].params.as_object().cloned().unwrap_or_default();
        let expected = json!({
            "length": 10.0, "cell": 10, "marks": 8.0, "copies": 5, "share": 0.5, "turn": 90.0, "gain": 2.0,
            "stops": 1.0, "laps": 1.5, "shift": -10.0,
        });
        assert_eq!(Value::Object(at_one), expected);
        // and a ratio of 2 really moves the four that scale, so the two renders differ in exactly those
        let mut moved: Vec<&str> = at_two
            .iter()
            .filter(|(name, value)| expected[name.as_str()] != **value)
            .map(|(name, _)| name.as_str())
            .collect();
        moved.sort_unstable();
        assert_eq!(moved, ["cell", "length", "marks", "shift"]);
        Ok(())
    }

    #[test]
    fn an_effect_that_describes_no_parameters_gets_the_values_as_written_and_the_ratio_if_it_declares_it()
    -> Result<(), Error> {
        let (registry, _, loose) = measuring()?;
        render_at(&registry, Step::new("loose").param("cell", 10), 2.5)?;
        let received = loose.received();
        assert_eq!(received[0].params, json!({"cell": 10}));
        assert!((received[0].size_ratio - 2.5).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    fn scaling_keeps_the_parameters_it_does_not_scale_as_the_step_wrote_them() -> Result<(), Error> {
        // a map of the received parameters has every declared field and no other
        let (registry, measure, _) = measuring()?;
        render_at(&registry, Step::new("measure").param("copies", 3), 2.0)?;
        let received: Map<String, Value> = measure.received()[0].params.as_object().cloned().unwrap_or_default();
        assert_eq!(received.len(), 10);
        assert_eq!(received["copies"], json!(3));
        Ok(())
    }
}
