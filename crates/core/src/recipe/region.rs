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
pub(crate) mod presets;

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
        if kind == "shards" {
            presets::lay_under(&mut table).map_err(D::Error::custom)?;
        }
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
