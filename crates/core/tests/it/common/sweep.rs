//! The parameter settings an effect is tried with across the integration tests: the defaults, then each parameter alone
//! at each end of its range.

use bullterrier_image::params::{Kind, Scalar, Table, Unit};
use serde_json::{Map, Value, json};

/// The values of one step's parameters as written.
pub(crate) type Setting = Map<String, Value>;

/// A distance in design px that is neither nothing nor the picture.
const TYPICAL_PX: f64 = 12.0;

pub(crate) fn number(bound: Option<Scalar>) -> Option<f64> {
    match bound? {
        Scalar::Int(whole) => i32::try_from(whole).ok().map(f64::from),
        Scalar::Float(decimal) => Some(decimal),
        _ => None,
    }
}

/// The parameters to try: none given (the defaults), then each parameter alone at each end of its range (a direction
/// also at the quarter turns, a distance also at 12 px), at each choice and at the other position of a switch. Some may be refused by the table (a pair that crosses): those are
/// left out when they are checked.
#[allow(clippy::cast_possible_truncation, reason = "limits are small whole numbers")]
pub(crate) fn variants(table: &Table) -> Vec<Setting> {
    let mut cases = vec![Setting::new()];
    for field in &table.fields {
        let values: Vec<Value> = match field.kind {
            Kind::Float | Kind::Int => {
                let (low, high) = (number(field.min), number(field.max));
                let mut bounds: Vec<f64> = [low, high].into_iter().flatten().collect();
                if let (Some(low), Some(high)) = (low, high) {
                    if field.cyclic {
                        // a direction is tried at the quarter turns too: at its limits it points along the rows or across them
                        bounds.extend([0.25, 0.5, 0.75].map(|share| low + (high - low) * share));
                    }
                    if field.unit == Some(Unit::Px) {
                        // a distance is tried at 12 px too, a size an effect works at: its limits are beyond the little picture
                        bounds.extend(
                            [-TYPICAL_PX, TYPICAL_PX]
                                .into_iter()
                                .filter(|px| (low..=high).contains(px)),
                        );
                    }
                }
                bounds.into_iter()
            }
            .map(|bound| {
                if field.kind == Kind::Int {
                    json!(bound as i64)
                } else {
                    json!(bound)
                }
            })
            .collect(),
            Kind::Choice => field.choices.iter().map(|choice| json!(choice)).collect(),
            Kind::Bool => vec![json!(true), json!(false)],
            Kind::Picture => vec![json!("asset")],
            _ => Vec::new(),
        };
        cases.extend(
            values
                .into_iter()
                .map(|value| Setting::from_iter([(field.name.to_owned(), value)])),
        );
    }
    cases
}

/// The settings that move things by a fraction of a pixel along no axis: each number alone at two shares of its range that
/// are neither end nor middle (a direction lands off the quarter turns, a distance between whole pixels). Some may be
/// refused by the table: those are left out when they are checked.
#[allow(clippy::cast_possible_truncation, reason = "limits are small whole numbers")]
pub(crate) fn awkward(table: &Table) -> Vec<Setting> {
    let mut cases = Vec::new();
    for field in &table.fields {
        let (Some(low), Some(high)) = (number(field.min), number(field.max)) else {
            continue;
        };
        for share in [0.37, 0.71] {
            let value = match field.kind {
                Kind::Float => json!(low + (high - low) * share),
                Kind::Int => json!((low + (high - low) * share).round() as i64),
                _ => continue,
            };
            cases.push(Setting::from_iter([(field.name.to_owned(), value)]));
        }
    }
    cases
}

/// The settings that move things by a distance too fine for a row to carry: each number alone at 1e-7 and 1e-9 where the
/// range holds them. A position read at such a distance from a far row rounds by the row it is worked out from, so a window
/// that starts low and the whole render read slightly different positions.
pub(crate) fn fine(table: &Table) -> Vec<Setting> {
    let mut cases = Vec::new();
    for field in &table.fields {
        let (Some(low), Some(high)) = (number(field.min), number(field.max)) else {
            continue;
        };
        for value in [1e-7, 1e-9] {
            if field.kind == Kind::Float && (low..=high).contains(&value) {
                cases.push(Setting::from_iter([(field.name.to_owned(), json!(value))]));
            }
        }
    }
    cases
}
