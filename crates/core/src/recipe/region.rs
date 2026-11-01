//! The regions of a recipe: named selections and partitions of the picture the steps can aim at.
//!
//! A [`Region`] is one of thirteen kinds, each a struct of typed, documented fields with Python's defaults; [`Regions`]
//! keeps a recipe's regions by name in the order they were declared. They are read from a recipe's `regions` (a
//! `[region.NAME]` table in a file, with a `kind` key naming the kind) and written back with every field, a field left
//! unset (`fit`, `preset`, `per`) left out and a count of shares (`columns = 3`) written as the list of shares. Reading
//! checks the shape; the ranges are the rules of [`Recipe::checked`](crate::Recipe::checked), which names the region,
//! the field, the value and the rule.
//!
//! ```
//! use bullterrier_image::region::{Grid, Shares};
//! use bullterrier_image::{Recipe, Step};
//!
//! let recipe = Recipe::new([Step::new("grade")])
//!     .with_region("panes", Grid { columns: Shares::equal(3), ..Grid::default() });
//! assert_eq!(recipe.regions.iter().count(), 1);
//! ```

mod check;
mod derived;
mod laid;

use std::collections::HashMap;
use std::fmt;

use serde::de::{Error as _, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

pub use derived::{Band, Bands, Combine, CombineOp, Given, LinearGradient, Pick, Piece, RadialGradient, Subject};
pub use laid::{Curve, Cut, Cuts, Edge, EdgeStyle, Grid, Line, Path, Shards, Shares, Svg};

use super::built_in::{BuiltIn, Origin};
use super::read;
use super::references::Form;

/// One region of a recipe. The kinds are told apart by the key `kind` in a recipe's JSON and TOML forms
/// (`linear_gradient`), which the variants' names spell in snake case.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Region {
    /// Rectangular pieces in columns and rows: a partition.
    Grid(Grid),
    /// Irregular pieces round scattered seeds: a partition.
    Shards(Shards),
    /// The pieces that straight and curved cuts divide the frame into: a partition.
    Cuts(Cuts),
    /// Everything inside a closed path: a selection.
    Path(Path),
    /// The shapes of an SVG file: a partition.
    Svg(Svg),
    /// The pixels of some pieces of a partition: a selection.
    Pick(Pick),
    /// The subject, of the picture or of each piece of a partition: a selection.
    Subject(Subject),
    /// Where another selection lies between two values: a selection.
    Band(Band),
    /// Equal ranges of a selection's scale: a partition.
    Bands(Bands),
    /// Selections put together: a selection.
    Combine(Combine),
    /// A value rising across the picture: a selection.
    LinearGradient(LinearGradient),
    /// A value rising from a point: a selection.
    RadialGradient(RadialGradient),
    /// A map the caller hands in under the region's name: a selection.
    Given(Given),
}

/// The kinds, as a recipe file writes them. [`Region`]'s variants spell the same names in snake case.
const KINDS: [&str; 13] = [
    "grid",
    "shards",
    "cuts",
    "path",
    "svg",
    "pick",
    "subject",
    "band",
    "bands",
    "combine",
    "linear_gradient",
    "radial_gradient",
    "given",
];

/// `From<Kind> for Region` for each kind, so `Recipe::with_region` takes a kind as it is.
macro_rules! regions_from {
    ($($kind:ident),+) => {$(
        impl From<$kind> for Region {
            fn from(kind: $kind) -> Self {
                Self::$kind(kind)
            }
        }
    )+};
}

regions_from!(
    Grid,
    Shards,
    Cuts,
    Path,
    Svg,
    Pick,
    Subject,
    Band,
    Bands,
    Combine,
    LinearGradient,
    RadialGradient,
    Given
);

impl Region {
    /// What this region is: a partition into pieces, or a selection of pixels.
    pub(crate) fn form(&self) -> Form {
        match self {
            Self::Grid(_) | Self::Shards(_) | Self::Cuts(_) | Self::Svg(_) | Self::Bands(_) => Form::Partition,
            Self::Path(_)
            | Self::Pick(_)
            | Self::Subject(_)
            | Self::Band(_)
            | Self::Combine(_)
            | Self::LinearGradient(_)
            | Self::RadialGradient(_)
            | Self::Given(_) => Form::Selection,
        }
    }

    /// What of this region the core cannot make yet, if anything: its kind, or a torn edge. `start` of a render refuses
    /// a recipe whose steps use such a region, before anything runs; one declared and unused renders.
    pub(crate) fn not_yet(&self) -> Option<&'static str> {
        let torn = |edge: &Edge| (edge.style == EdgeStyle::Torn).then_some("a torn edge");
        match self {
            Self::Grid(Grid { edge, .. }) => torn(edge),
            Self::Shards(_) => Some("kind shards"),
            Self::Cuts(_) => Some("kind cuts"),
            Self::Path(_) => Some("kind path"),
            Self::Svg(_) => Some("kind svg"),
            Self::Pick(_)
            | Self::Bands(_)
            | Self::Subject(_)
            | Self::Band(_)
            | Self::Combine(_)
            | Self::LinearGradient(_)
            | Self::RadialGradient(_)
            | Self::Given(_) => None,
        }
    }

    /// The regions this one is made from, each as (the field that names it, its name, the form it must have): the one
    /// a kind is laid over (`fit`), the one a `Pick` picks from, the ones a `Band`, `Bands` or `Combine` is cut or put
    /// together from, the one a `Subject` is found in each piece of.
    pub(crate) fn refers_to(&self) -> Vec<(&'static str, &str, Form)> {
        fn laid(fit: Option<&String>) -> Vec<(&'static str, &str, Form)> {
            fit.into_iter()
                .map(|fit| ("fit", fit.as_str(), Form::Selection))
                .collect()
        }
        match self {
            Self::Grid(Grid { fit, .. })
            | Self::Shards(Shards { fit, .. })
            | Self::Cuts(Cuts { fit, .. })
            | Self::Path(Path { fit, .. })
            | Self::Svg(Svg { fit, .. }) => laid(fit.as_ref()),
            Self::Pick(pick) => vec![("of", pick.of.as_str(), Form::Partition)],
            Self::Subject(subject) => subject
                .per
                .iter()
                .map(|per| ("per", per.as_str(), Form::Partition))
                .collect(),
            Self::Band(Band { of, .. }) | Self::Bands(Bands { of, .. }) => vec![("of", of.as_str(), Form::Selection)],
            Self::Combine(combine) => combine
                .of
                .iter()
                .map(|of| ("of", of.as_str(), Form::Selection))
                .collect(),
            Self::LinearGradient(_) | Self::RadialGradient(_) | Self::Given(_) => Vec::new(),
        }
    }
}

impl<'de> Deserialize<'de> for Region {
    /// A table whose `kind` names the kind, with that kind's fields beside it; every field has a default.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let Value::Object(mut table) = Value::deserialize(deserializer)? else {
            return Err(D::Error::custom(
                "a region is a table: its `kind` and the fields of that kind",
            ));
        };
        let kinds = || format!("the kinds are: {}", KINDS.join(", "));
        let kind = match table.shift_remove("kind") {
            None => return Err(D::Error::custom(format!("`kind` is missing; {}", kinds()))),
            Some(Value::String(kind)) => kind,
            Some(other) => {
                return Err(D::Error::custom(format!(
                    "`kind` must be text, got {other}; {}",
                    kinds()
                )));
            }
        };
        let table = Value::Object(table);
        let region = match kind.as_str() {
            "grid" => read(table).map(Self::Grid),
            "shards" => read(table).map(Self::Shards),
            "cuts" => read(table).map(Self::Cuts),
            "path" => read(table).map(Self::Path),
            "svg" => read(table).map(Self::Svg),
            "pick" => read(table).map(Self::Pick),
            "subject" => read(table).map(Self::Subject),
            "band" => read(table).map(Self::Band),
            "bands" => read(table).map(Self::Bands),
            "combine" => read(table).map(Self::Combine),
            "linear_gradient" => read(table).map(Self::LinearGradient),
            "radial_gradient" => read(table).map(Self::RadialGradient),
            "given" => read(table).map(Self::Given),
            _ => {
                return Err(D::Error::custom(format!(
                    "`kind` '{kind}' is no region kind; {}",
                    kinds()
                )));
            }
        };
        region.map_err(D::Error::custom)
    }
}

/// The regions of a recipe by name, in the order they were declared (a file's order is data: it is the order a
/// refusal lists them in and the order a trace shows them in).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Regions {
    /// The regions in the order they were declared.
    list: Vec<(String, Region)>,
    /// Where in `list` each name is: derived from it, so that a lookup is one hash, not a scan of every region.
    at: HashMap<String, usize>,
}

impl Regions {
    /// No regions.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Declares `region` under `name`, after the ones declared so far; one already under that name is replaced where
    /// it stands and returned.
    pub fn insert(&mut self, name: impl Into<String>, region: impl Into<Region>) -> Option<Region> {
        let (name, region) = (name.into(), region.into());
        if let Some(&at) = self.at.get(&name) {
            return Some(std::mem::replace(&mut self.list[at].1, region));
        }
        self.at.insert(name.clone(), self.list.len());
        self.list.push((name, region));
        None
    }

    /// The region called `name`, if one is declared.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Region> {
        self.at.get(name).map(|&at| &self.list[at].1)
    }

    /// The names of what the region `name` is made from, which must exist before it can be made: the regions its fields
    /// name (see [`Region::refers_to`]), the built-in `subject` for a `Subject` found without `per` and for the built-in
    /// `background` (1 - the subject). Nothing for any other name: the other built-in regions are maps. The one
    /// definition of "made from": checking for cycles, making the regions and deciding when each is last needed all use it.
    pub(crate) fn made_from(&self, name: &str) -> Vec<&str> {
        match self.get(name) {
            Some(Region::Subject(Subject { per: None, .. })) => vec![BuiltIn::Subject.name()],
            Some(region) => region
                .refers_to()
                .into_iter()
                .map(|(_, made_from, _)| made_from)
                .collect(),
            None => match BuiltIn::named(name).map(BuiltIn::origin) {
                Some(Origin::Inverse(of)) => vec![of.name()],
                _ => Vec::new(),
            },
        }
    }

    /// The regions with their names, in the order they were declared.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Region)> {
        self.list.iter().map(|(name, region)| (name.as_str(), region))
    }

    /// Whether no region is declared.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

impl Serialize for Regions {
    /// A table from name to region, in declaration order.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut table = serializer.serialize_map(Some(self.list.len()))?;
        for (name, region) in &self.list {
            table.serialize_entry(name, region)?;
        }
        table.end()
    }
}

impl<'de> Deserialize<'de> for Regions {
    /// A table from name to region, kept in the order it is written in.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(RegionsVisitor)
    }
}

struct RegionsVisitor;

impl<'de> Visitor<'de> for RegionsVisitor {
    type Value = Regions;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a table from region name to region")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut entries: A) -> Result<Regions, A::Error> {
        let mut regions = Regions::new();
        while let Some(name) = entries.next_key::<String>()? {
            let region = entries.next_value::<Region>()?;
            regions.insert(name, region);
        }
        Ok(regions)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::Recipe;

    /// The borders of an untouched kind, written.
    fn straight() -> Value {
        json!({"style": "straight", "amount": 4.8, "scale": 24.0, "seed": 0, "octaves": 6, "persistence": 0.55})
    }

    /// `text` as the region it writes, or the message that refuses it.
    fn read(region: &Value) -> Result<Region, String> {
        Recipe::from_json(&json!({"steps": [], "regions": {"panes": region}}).to_string())
            .map_err(|error| error.to_string())
            .and_then(|recipe| {
                recipe
                    .regions
                    .get("panes")
                    .cloned()
                    .ok_or_else(|| "no region".to_owned())
            })
    }

    /// Every kind with nothing set but its `kind`, and the fields it is written with: the defaults of the Python
    /// declarations, by hand.
    #[test]
    fn a_kind_alone_is_read_as_the_defaults_and_written_with_every_field_that_is_set() {
        let rows = [
            ("grid", json!({"edge": straight(), "columns": [1.0], "rows": [1.0]})),
            (
                "shards",
                json!({"edge": straight(), "count": 12, "jitter": 0.5, "impact": [0.5, 0.5], "focus": 0.0, "seed": 0}),
            ),
            (
                "cuts",
                json!({"edge": straight(), "lines": [{"x": 0.5, "y": 0.5, "angle": 0.0}]}),
            ),
            (
                "path",
                json!({"edge": straight(), "d": "M 0 0 H 1 V 1 H 0 Z", "samples": 4}),
            ),
            (
                "svg",
                json!({"edge": straight(), "file": "", "max_shapes": 1000, "max_kilobytes": 1024}),
            ),
            ("pick", json!({"of": "", "pieces": [0]})),
            ("subject", json!({"context": 0.25, "threshold": 0.9})),
            ("band", json!({"of": "", "low": 0.5, "high": 1.0, "feather": 0.1})),
            ("bands", json!({"of": "", "count": 3})),
            ("combine", json!({"op": "multiply", "of": ["", ""]})),
            ("linear_gradient", json!({"angle": 270.0})),
            ("radial_gradient", json!({"x": 0.5, "y": 0.5, "radius": 0.5})),
            ("given", json!({})),
        ];
        assert_eq!(
            rows.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(),
            KINDS.to_vec(),
            "every kind has a row, in the order of KINDS"
        );
        for (kind, fields) in rows {
            let mut expected = fields;
            expected["kind"] = json!(kind);
            let region = read(&json!({"kind": kind}));
            let written = region
                .as_ref()
                .ok()
                .and_then(|region| serde_json::to_value(region).ok());
            assert_eq!(written, Some(expected), "{kind}: {region:?}");
        }
    }

    /// A region of every kind with no field at its default (a distinct value in each), declared in an order that is not
    /// the alphabet's nor `KINDS`'.
    #[expect(clippy::too_many_lines, reason = "a table, one kind each")]
    fn every_kind() -> Vec<(&'static str, Region)> {
        let torn = Edge {
            style: EdgeStyle::Torn,
            amount: 12.0,
            scale: 96.0,
            seed: 7,
            octaves: 3,
            persistence: 0.25,
        };
        vec![
            ("given_map", Given {}.into()),
            (
                "panes",
                Grid {
                    fit: Some("subject".to_owned()),
                    edge: torn.clone(),
                    columns: Shares(vec![1.0, 2.5, 1.0]),
                    rows: Shares::equal(2),
                }
                .into(),
            ),
            (
                "glass",
                Shards {
                    fit: Some("panes_pick".to_owned()),
                    edge: torn,
                    count: 40,
                    jitter: 0.75,
                    impact: [0.25, 0.875],
                    focus: 2.5,
                    seed: 11,
                    preset: Some("glass".to_owned()),
                }
                .into(),
            ),
            (
                "slits",
                Cuts {
                    lines: vec![
                        Line {
                            x: 0.25,
                            y: 0.75,
                            angle: 45.0,
                        }
                        .into(),
                        Curve {
                            path: "M 0.38 0 C 0.15 0.45, 0.85 0.55, 0.62 1".to_owned(),
                        }
                        .into(),
                    ],
                    ..Cuts::default()
                }
                .into(),
            ),
            (
                "blob",
                Path {
                    d: "M 0.1 0.4 C 0.1 0.2, 0.4 0.2, 0.4 0.4 Z".to_owned(),
                    samples: 9,
                    ..Path::default()
                }
                .into(),
            ),
            (
                "logo",
                Svg {
                    file: "shapes.svg".to_owned(),
                    max_shapes: 50,
                    max_kilobytes: 64,
                    ..Svg::default()
                }
                .into(),
            ),
            (
                "panes_pick",
                Pick {
                    of: "panes".to_owned(),
                    pieces: vec![0.into(), "star".into(), 3.into()],
                }
                .into(),
            ),
            (
                "found",
                Subject {
                    per: Some("panes".to_owned()),
                    context: 0.5,
                    threshold: 0.625,
                }
                .into(),
            ),
            (
                "near",
                Band {
                    of: "depth".to_owned(),
                    low: 0.25,
                    high: 0.75,
                    feather: 0.5,
                }
                .into(),
            ),
            (
                "layers",
                Bands {
                    of: "depth".to_owned(),
                    count: 5,
                }
                .into(),
            ),
            (
                "both",
                Combine {
                    op: CombineOp::Max,
                    of: vec!["near".to_owned(), "subject".to_owned(), "light".to_owned()],
                }
                .into(),
            ),
            ("ramp", LinearGradient { angle: 725.0 }.into()),
            (
                "spot",
                RadialGradient {
                    x: 0.25,
                    y: 0.625,
                    radius: 1.5,
                }
                .into(),
            ),
        ]
    }

    #[test]
    fn regions_of_every_kind_with_every_field_set_are_written_and_read_back_equal_in_declaration_order() {
        let mut recipe = Recipe::new([crate::Step::new("grade")]);
        for (name, region) in every_kind() {
            recipe.regions.insert(name, region);
        }
        let text = serde_json::to_string(&recipe).unwrap_or_default();
        let back = Recipe::from_json(&text);
        assert_eq!(back.as_ref().ok(), Some(&recipe), "{text}");
        let names: Vec<&str> = recipe.regions.iter().map(|(name, _)| name).collect();
        assert_eq!(
            names,
            [
                "given_map",
                "panes",
                "glass",
                "slits",
                "blob",
                "logo",
                "panes_pick",
                "found",
                "near",
                "layers",
                "both",
                "ramp",
                "spot"
            ]
        );
    }

    #[test]
    fn a_toml_file_writes_the_same_regions_in_its_own_forms() {
        let text = r#"
format = 3

[region.given_map]
kind = "given"

[region.panes]
kind = "grid"
fit = "subject"
columns = [1, 2.5, 1]
rows = 2
[region.panes.edge]
style = "torn"
amount = 12
scale = 96
seed = 7
octaves = 3
persistence = 0.25

[region.glass]
kind = "shards"
fit = "panes_pick"
count = 40
jitter = 0.75
impact = [0.25, 0.875]
focus = 2.5
seed = 11
preset = "glass"
edge = { style = "torn", amount = 12, scale = 96, seed = 7, octaves = 3, persistence = 0.25 }

[region.slits]
kind = "cuts"
lines = [{ x = 0.25, y = 0.75, angle = 45 }, { path = "M 0.38 0 C 0.15 0.45, 0.85 0.55, 0.62 1" }]

[region.blob]
kind = "path"
d = "M 0.1 0.4 C 0.1 0.2, 0.4 0.2, 0.4 0.4 Z"
samples = 9

[region.logo]
kind = "svg"
file = "shapes.svg"
max_shapes = 50
max_kilobytes = 64

[region.panes_pick]
kind = "pick"
of = "panes"
pieces = [0, "star", 3]

[region.found]
kind = "subject"
per = "panes"
context = 0.5
threshold = 0.625

[region.near]
kind = "band"
of = "depth"
low = 0.25
high = 0.75
feather = 0.5

[region.layers]
kind = "bands"
of = "depth"
count = 5

[region.both]
kind = "combine"
op = "max"
of = ["near", "subject", "light"]

[region.ramp]
kind = "linear_gradient"
angle = 725

[region.spot]
kind = "radial_gradient"
x = 0.25
y = 0.625
radius = 1.5

[[step]]
effect = "grade"
"#;
        let read = Recipe::from_toml(text).map(|recipe| recipe.regions);
        let mut expected = Regions::new();
        for (name, region) in every_kind() {
            expected.insert(name, region);
        }
        assert_eq!(read.ok(), Some(expected));
    }

    #[test]
    fn a_count_of_shares_is_that_many_equal_shares_and_is_written_as_the_list() {
        let grid = |columns: Value| read(&json!({"kind": "grid", "columns": columns}));
        let expected = Region::Grid(Grid {
            columns: Shares(vec![1.0; 3]),
            ..Grid::default()
        });
        assert_eq!(grid(json!(3)), Ok(expected.clone()));
        assert_eq!(grid(json!([1, 1, 1.0])), Ok(expected.clone()));
        let written = serde_json::to_value(&expected)
            .ok()
            .and_then(|region| region.get("columns").cloned());
        assert_eq!(written, Some(json!([1.0, 1.0, 1.0])));
        assert_eq!(
            grid(json!(1000)).map(|region| match region {
                Region::Grid(grid) => grid.columns.0.len(),
                _ => 0,
            }),
            Ok(1000)
        );
        // a count outside 1 to 1000 is no count of columns; nothing is allocated for it
        for (what, columns) in [
            ("none", json!(0)),
            ("a negative count", json!(-2)),
            ("one past the most", json!(1001)),
            ("an enormous count", json!(4_000_000_000_u64)),
            ("a decimal", json!(2.5)),
            ("text", json!("3")),
            ("a switch", json!(true)),
        ] {
            let refused = grid(columns).err();
            assert!(
                refused
                    .as_deref()
                    .is_some_and(|message| message.contains("columns") && message.contains("count")),
                "{what}: {refused:?}"
            );
        }
    }

    #[test]
    fn a_cut_is_a_curve_when_it_has_a_path_and_a_line_otherwise() {
        let cuts = read(&json!({"kind": "cuts", "lines": [{}, {"x": 0.25}, {"path": "M 0 0 L 1 1"}]}));
        let lines = match cuts {
            Ok(Region::Cuts(cuts)) => Some(cuts.lines),
            _ => None,
        };
        let expected: Vec<Cut> = vec![
            Line::default().into(),
            Line {
                x: 0.25,
                ..Line::default()
            }
            .into(),
            Curve::default().into(),
        ];
        assert_eq!(lines, Some(expected));
        // a cut with fields of both is neither: the unknown one is named
        for (what, cut, field) in [
            ("a line with a path", json!({"x": 0.5, "path": "M 0 0 L 1 1"}), "x"),
            (
                "a curve with an angle",
                json!({"path": "M 0 0 L 1 1", "angle": 4}),
                "angle",
            ),
            ("a line with a stray key", json!({"slope": 4}), "slope"),
        ] {
            let refused = read(&json!({"kind": "cuts", "lines": [cut]})).err();
            assert!(
                refused.as_deref().is_some_and(|message| message.contains(field)),
                "{what}: {refused:?}"
            );
        }
    }

    #[test]
    fn a_piece_is_a_number_or_the_id_of_a_shape() {
        let pick = |pieces: Value| read(&json!({"kind": "pick", "of": "p", "pieces": pieces}));
        let good = match pick(json!([0, "star", 4_294_967_295_u64, "0"])) {
            Ok(Region::Pick(pick)) => Some(pick.pieces),
            _ => None,
        };
        let expected: Vec<Piece> = vec![0.into(), "star".into(), u32::MAX.into(), "0".into()];
        assert_eq!(good, Some(expected));
        for (what, pieces, said) in [
            ("a negative number", json!([1, -1]), "count from 0"),
            ("a number beyond a partition", json!([4_294_967_296_u64]), "beyond"),
            ("a decimal", json!([1.5]), "piece"),
            ("a switch", json!([true]), "piece"),
            ("a list", json!([[1]]), "piece"),
        ] {
            let refused = pick(pieces).err();
            assert!(
                refused.as_deref().is_some_and(|message| message.contains(said)),
                "{what}: {refused:?}"
            );
        }
    }

    #[test]
    #[expect(clippy::too_many_lines, reason = "a table of rows, one rule each")]
    fn a_region_that_is_not_a_table_of_a_known_kind_is_refused_naming_what_is_wrong() {
        let kinds = KINDS.join(", ");
        for (what, region, said) in [
            (
                "no kind",
                json!({"columns": 2}),
                vec!["`kind` is missing".to_owned(), kinds.clone()],
            ),
            (
                "an empty table",
                json!({}),
                vec!["`kind` is missing".to_owned(), kinds.clone()],
            ),
            (
                "an unknown kind",
                json!({"kind": "gird"}),
                vec!["'gird'".to_owned(), kinds.clone()],
            ),
            (
                "a kind in capitals",
                json!({"kind": "Grid"}),
                vec!["'Grid'".to_owned(), kinds.clone()],
            ),
            (
                "a kind that is no text",
                json!({"kind": 3}),
                vec!["`kind` must be text".to_owned(), kinds],
            ),
            ("text", json!("grid"), vec!["table".to_owned()]),
            ("a list", json!(["grid"]), vec!["table".to_owned()]),
            ("nothing", json!(null), vec!["table".to_owned()]),
            (
                "a field of another kind",
                json!({"kind": "grid", "count": 3}),
                vec!["`count`".to_owned()],
            ),
            (
                "a field that is nobody's",
                json!({"kind": "given", "of": "x"}),
                vec!["`of`".to_owned()],
            ),
            (
                "a misspelt field",
                json!({"kind": "band", "lo": 0.1}),
                vec!["`lo`".to_owned()],
            ),
            (
                "a number as text",
                json!({"kind": "band", "low": "0.1"}),
                vec!["low".to_owned()],
            ),
            (
                "a share as text",
                json!({"kind": "grid", "columns": [1, "2"]}),
                vec!["columns".to_owned()],
            ),
            (
                "a whole number as a decimal",
                json!({"kind": "shards", "count": 2.5}),
                vec!["count".to_owned()],
            ),
            (
                "a negative seed",
                json!({"kind": "shards", "seed": -1}),
                vec!["seed".to_owned()],
            ),
            (
                "a seed beyond 32 bits",
                json!({"kind": "shards", "seed": 4_294_967_296_u64}),
                vec!["seed".to_owned()],
            ),
            (
                "impact of three",
                json!({"kind": "shards", "impact": [0, 0, 0]}),
                vec!["impact".to_owned()],
            ),
            (
                "an edge that is no table",
                json!({"kind": "grid", "edge": "torn"}),
                vec!["edge".to_owned()],
            ),
            (
                "an edge style that is none of the two",
                json!({"kind": "grid", "edge": {"style": "wavy"}}),
                vec!["style".to_owned()],
            ),
            (
                "an edge field that is not one",
                json!({"kind": "grid", "edge": {"width": 3}}),
                vec!["`width`".to_owned()],
            ),
            (
                "a combine op that is none of three",
                json!({"kind": "combine", "op": "xor"}),
                vec!["op".to_owned()],
            ),
        ] {
            let refused = read(&region).err();
            assert!(
                refused
                    .as_deref()
                    .is_some_and(|message| said.iter().all(|word| message.contains(word.as_str()))),
                "{what}: wanted {said:?}, got {refused:?}"
            );
        }
    }

    #[test]
    fn a_refusal_in_a_field_names_the_region_and_the_field() {
        let refused = read(&json!({"kind": "grid", "columns": [1, "x"]}))
            .err()
            .unwrap_or_default();
        assert!(
            refused.contains("regions.panes") && refused.contains("columns"),
            "{refused}"
        );
        let toml = Recipe::from_toml(
            "format = 2\n[region.panes]\nkind = \"grid\"\ncolumns = \"x\"\n[[step]]\neffect = \"a\"\n",
        )
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
        assert!(toml.contains("region 'panes'") && toml.contains("columns"), "{toml}");
    }

    #[test]
    fn a_region_replaces_the_one_of_its_name_where_it_stands() {
        let mut regions = Regions::new();
        assert!(regions.is_empty());
        assert_eq!(regions.insert("b", Given {}), None);
        assert_eq!(regions.insert("a", Band::default()), None);
        let replaced = regions.insert("b", Bands::default());
        assert_eq!(replaced, Some(Region::Given(Given {})));
        let names: Vec<&str> = regions.iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["b", "a"]);
        assert_eq!(regions.get("b"), Some(&Region::Bands(Bands::default())));
        assert_eq!(regions.get("c"), None);
    }

    #[test]
    fn a_recipe_built_in_code_declares_its_regions_in_order() {
        let recipe = Recipe::new([crate::Step::new("grade")])
            .with_region("z", Given {})
            .with_region(
                "a",
                Grid {
                    columns: Shares::equal(3),
                    ..Grid::default()
                },
            );
        let names: Vec<&str> = recipe.regions.iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["z", "a"]);
        assert!(matches!(recipe.regions.get("a"), Some(Region::Grid(grid)) if grid.columns.0.len() == 3));
    }
}
