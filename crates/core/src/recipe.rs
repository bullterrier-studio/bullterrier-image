//! A look as data: a [`Recipe`] is an ordered list of [`Entry`]s, each an effect [`Step`] or an [`Each`] (a look for
//! every piece of a partition).
//!
//! The types are the whole model, built in code (`Step::new("bloom").param("radius", 12)`) or read from JSON
//! ([`Recipe::from_json`]); `serde_json` writes them back. A step keeps its parameters as written and says how its
//! result is laid over the picture below it: a `mask`, an `opacity` (none: the effect's own default) and a
//! [`BlendMode`]. The framing is a typed [`Frame`] and the regions typed [`Regions`] of thirteen kinds.
//!
//! Reading and building check nothing but the shape: an unknown key is refused by name, a wrong type too. The rules of
//! a look (known effects, parameters within their ranges, names that resolve) are `Recipe::checked`.

use std::collections::BTreeSet;
use std::fmt;

use serde::de::{Error as _, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use crate::Error;
use crate::params::EffectParams;

pub(crate) mod built_in;
mod check;
pub(crate) mod drive;
pub(crate) mod file;
pub mod frame;
mod load;
mod near;
mod needs;
mod numbers;
mod references;
pub mod region;

pub use drive::Drive;
pub use frame::Frame;
pub use needs::Needs;
pub use numbers::Number;
pub use region::{Region, Regions};

/// The keys of a step besides its effect's parameters, written the same in a file and in code: no parameter of an
/// effect may be named like one.
pub const STEP_KEYS: &[&str] = &[
    "effect",
    "mask",
    "opacity",
    "blend_mode",
    "preset",
    "partition",
    "each",
    "looks",
    "name",
];

/// How a step's result combines with the picture below it: the library's seven blend modes, each defined as in W3C
/// Compositing and Blending Level 1 (which lists more).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum BlendMode {
    /// The result shows as it is.
    #[default]
    Normal,
    /// Darkens.
    Multiply,
    /// Lightens.
    Screen,
    /// Deepens contrast by the picture below.
    Overlay,
    /// Deepens contrast gently.
    SoftLight,
    /// Subtracts.
    Difference,
    /// Takes the result's hue and saturation and keeps the picture's brightness.
    Color,
}

impl BlendMode {
    /// Every mode, in the order recipes list them.
    pub const ALL: &'static [Self] = &[
        Self::Normal,
        Self::Multiply,
        Self::Screen,
        Self::Overlay,
        Self::SoftLight,
        Self::Difference,
        Self::Color,
    ];

    /// The mode's name in recipes, in `snake_case`: `"soft_light"`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Multiply => "multiply",
            Self::Screen => "screen",
            Self::Overlay => "overlay",
            Self::SoftLight => "soft_light",
            Self::Difference => "difference",
            Self::Color => "color",
        }
    }
}

impl crate::params::Choice for BlendMode {
    // constant: the names of `BlendMode::ALL`, in its order (a test holds the two together)
    const CHOICES: &'static [&'static str] = &[
        "normal",
        "multiply",
        "screen",
        "overlay",
        "soft_light",
        "difference",
        "color",
    ];

    fn name(&self) -> &'static str {
        BlendMode::name(*self)
    }
}

/// How the pieces of an [`Each`] get their looks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Assign {
    /// The pieces, piece 0 first, take the looks in turn.
    #[default]
    Cycle,
    /// Each piece draws a look at random; the same seed always draws the same.
    Random,
}

/// One effect with its parameters, laid over the picture below it (its input) as a layer is in an image editor:
/// optionally limited to a selection, combined by a blend mode and mixed in partway.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// The name of a registered effect.
    pub effect: String,
    /// The effect's parameters as written: a number stays whole or decimal as it was written. With a `preset` they lie
    /// over the preset's.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub params: Map<String, Value>,
    /// The name of one of the effect's presets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// For an effect that draws along the borders of a partition: the name of the region whose pieces it uses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partition: Option<String>,
    /// Apply only where this selection says: `subject`, `background`, `depth`, `light` or a region the recipe
    /// declares; none: everywhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<String>,
    /// How much of the step shows, 0 none to 1 fully; none: the effect's own default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f64>,
    /// How the result combines with the step's input.
    #[serde(default, skip_serializing_if = "is_normal")]
    pub blend_mode: BlendMode,
    /// A name that addresses the step's numbers: unique in the recipe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Step {
    /// A step of the effect called `effect`, with no parameters, mask or blend: everything at its default.
    #[must_use]
    pub fn new(effect: impl Into<String>) -> Self {
        Self {
            effect: effect.into(),
            params: Map::new(),
            preset: None,
            partition: None,
            mask: None,
            opacity: None,
            blend_mode: BlendMode::Normal,
            name: None,
        }
    }

    /// A step of the effect the typed `params` belong to, with every parameter written and no preset:
    /// `Step::of(grade::Params { exposure: 1.0, ..Default::default() })` sets all seven of `grade`'s, the six others to
    /// their defaults. A decimal is the number a person would write (`0.2`, not the `f32` widened to
    /// `0.20000000298`). Like any step, it is checked by [`Recipe::checked`].
    #[must_use]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "a `Params { .. }` literal is passed as written, not borrowed"
    )]
    pub fn of<P: EffectParams>(params: P) -> Self {
        Self {
            params: params.values(),
            ..Self::new(P::EFFECT)
        }
    }

    /// This step with the parameter `key` set to `value` (a later call for the same key replaces it).
    #[must_use]
    pub fn param(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }

    /// This step limited to the selection called `mask`.
    #[must_use]
    pub fn mask(mut self, mask: impl Into<String>) -> Self {
        self.mask = Some(mask.into());
        self
    }

    /// This step mixed in by `opacity` (0 to 1).
    #[must_use]
    pub const fn opacity(mut self, opacity: f64) -> Self {
        self.opacity = Some(opacity);
        self
    }

    /// This step combined with its input by `blend_mode`.
    #[must_use]
    pub const fn blend_mode(mut self, blend_mode: BlendMode) -> Self {
        self.blend_mode = blend_mode;
        self
    }
}

/// A look for each piece of a partition: every piece shows one of `looks`, the looks taking turns or drawn at random,
/// and the pieces blend into each other a little at their borders. Its `mask`, `opacity` and `blend_mode` lay the
/// blended picture over the step's input like any step's result (an opacity of none is 1).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Each {
    /// The name of the partition region whose pieces get a look each.
    pub each: String,
    /// The looks: each a step, or none for a piece left as it is.
    pub looks: Vec<Option<Step>>,
    /// How pieces get looks.
    #[serde(default, skip_serializing_if = "is_default")]
    pub assign: Assign,
    /// The seed of [`Assign::Random`].
    #[serde(default, skip_serializing_if = "is_default")]
    pub seed: u32,
    /// How softly a piece's look fades into its neighbours' across a border, in px (0 to 24).
    #[serde(default = "default_feather", skip_serializing_if = "is_default_feather")]
    pub feather: f64,
    /// As on a [`Step`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<String>,
    /// As on a [`Step`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f64>,
    /// As on a [`Step`].
    #[serde(default, skip_serializing_if = "is_normal")]
    pub blend_mode: BlendMode,
    /// As on a [`Step`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Each {
    /// The pieces of the partition called `each` take `looks` in turn; the rest at its defaults.
    #[must_use]
    pub fn new(each: impl Into<String>, looks: impl IntoIterator<Item = Option<Step>>) -> Self {
        Self {
            each: each.into(),
            looks: looks.into_iter().collect(),
            assign: Assign::Cycle,
            seed: 0,
            feather: default_feather(),
            mask: None,
            opacity: None,
            blend_mode: BlendMode::Normal,
            name: None,
        }
    }
}

/// One entry of a recipe's steps.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Entry {
    /// One effect.
    Step(Step),
    /// A look for each piece of a partition.
    Each(Each),
}

impl From<Step> for Entry {
    fn from(step: Step) -> Self {
        Self::Step(step)
    }
}

impl From<Each> for Entry {
    fn from(each: Each) -> Self {
        Self::Each(each)
    }
}

impl<'de> Deserialize<'de> for Entry {
    /// An entry with an `each` key is an [`Each`], one with an `effect` key a [`Step`]; one with neither is refused
    /// saying so.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::Object(Map::deserialize(deserializer)?);
        let entry = if value.get("each").is_some() {
            read(value).map(Self::Each)
        } else if value.get("effect").is_some() {
            read(value).map(Self::Step)
        } else {
            Err("a step needs `effect` (an effect step) or `each` (an each step)".to_owned())
        };
        entry.map_err(D::Error::custom)
    }
}

/// A look: ordered steps, how the photo is framed and the size it is scaled to, and optionally a name and a description.
///
/// A recipe read or built is only a value: `Recipe::checked` is what makes it usable, and everything that runs a
/// recipe calls it first.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    /// The steps, run in order, each on the previous one's output.
    #[serde(default)]
    pub steps: Vec<Entry>,
    /// What the look is called, free text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// What the look looks like, in a sentence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The longest side in px the recipe renders at (at least 1); none: the photo's own. Its px values are always written
    /// for a picture whose longest side is 480.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u32>,
    /// How the photo is framed before the steps and cut out after them; the default frame changes nothing.
    #[serde(default, skip_serializing_if = "Frame::is_empty")]
    pub frame: Frame,
    /// The regions the steps can name besides the built-in ones, by name, in the order they were declared.
    #[serde(default, skip_serializing_if = "Regions::is_empty")]
    pub regions: Regions,
}

impl Recipe {
    /// A recipe of `steps` (steps or each steps), unnamed, at the photo's size, unframed, with no regions.
    #[must_use]
    pub fn new<I>(steps: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<Entry>,
    {
        Self {
            steps: steps.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }

    /// This recipe with `region` declared under `name`, after the regions declared so far.
    #[must_use]
    pub fn with_region(mut self, name: impl Into<String>, region: impl Into<Region>) -> Self {
        self.regions.insert(name, region);
        self
    }

    /// The recipe the JSON object `text` writes, in the form `serde_json` writes a recipe in. Only the shape is read:
    /// see `Recipe::checked` for the rules.
    ///
    /// # Errors
    /// [`Error::Recipe`] if the text is not JSON or not the shape of a recipe: an unknown, mistyped or twice-written
    /// key is named.
    pub fn from_json(text: &str) -> Result<Self, Error> {
        serde_json::from_str::<Unique>(text).map_err(|error| Error::Recipe(format!("not a recipe: {error}")))?;
        let mut deserializer = serde_json::Deserializer::from_str(text);
        serde_path_to_error::deserialize(&mut deserializer)
            .map_err(|error| Error::Recipe(format!("not a recipe: {}", describe(&error, None))))
    }
}

/// A JSON text in which no object writes a key twice (`serde_json::Value` would keep the last silently).
struct Unique;

impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(UniqueVisitor).map(|()| Self)
    }
}

struct UniqueVisitor;

impl<'de> Visitor<'de> for UniqueVisitor {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JSON")
    }

    fn visit_bool<E>(self, _: bool) -> Result<(), E> {
        Ok(())
    }

    fn visit_i64<E>(self, _: i64) -> Result<(), E> {
        Ok(())
    }

    fn visit_u64<E>(self, _: u64) -> Result<(), E> {
        Ok(())
    }

    fn visit_f64<E>(self, _: f64) -> Result<(), E> {
        Ok(())
    }

    fn visit_str<E>(self, _: &str) -> Result<(), E> {
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<(), E> {
        Ok(())
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut items: A) -> Result<(), A::Error> {
        while items.next_element::<Unique>()?.is_some() {}
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut entries: A) -> Result<(), A::Error> {
        let mut seen = BTreeSet::new();
        while let Some(key) = entries.next_key::<String>()? {
            if !seen.insert(key.clone()) {
                return Err(A::Error::custom(format!("`{key}` is written twice in one table")));
            }
            entries.next_value::<Unique>()?;
        }
        Ok(())
    }
}

/// The `T` that `value` writes, or what is wrong, naming the key it is about and, when that is a table or a list, what was
/// written there.
#[expect(
    clippy::needless_pass_by_value,
    reason = "the callers hand the table over; only the error reads it again"
)]
fn read<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_path_to_error::deserialize(&value).map_err(|error| describe(&error, Some(&value)))
}

/// `error` led by the key it is about (`opacity`, `steps[1]`, `look 2`), when it is about one. Given the `root` it was
/// read from, a mistyped table or list is shown as written: the type error alone says only "sequence".
fn describe<E: std::fmt::Display>(error: &serde_path_to_error::Error<E>, root: Option<&Value>) -> String {
    let path = error.path().to_string();
    let mut message = error.inner().to_string();
    if message.starts_with("invalid type")
        && let Some(written) = root.and_then(|root| written_at(root, error.path()))
        && matches!(written, Value::Array(_) | Value::Object(_))
    {
        message = format!("{message}, got {written}");
    }
    if path == "." {
        message
    } else {
        format!("{}: {message}", counted_looks(&path))
    }
}

/// What `root` holds at `path`, if anything.
fn written_at<'a>(root: &'a Value, path: &serde_path_to_error::Path) -> Option<&'a Value> {
    use serde_path_to_error::Segment;
    path.iter().try_fold(root, |value, segment| match segment {
        Segment::Map { key } => value.get(key),
        Segment::Seq { index } => value.get(*index),
        Segment::Enum { .. } | Segment::Unknown => None,
    })
}

/// `path` with each `looks[N]` as `look N+1`: the places of a message count from 1, as `look 1 is ...` does.
fn counted_looks(path: &str) -> String {
    let mut counted = String::new();
    let mut rest = path;
    while let Some(at) = rest.find("looks[") {
        let after = &rest[at + "looks[".len()..];
        let Some((index, end)) = after
            .find(']')
            .and_then(|end| after[..end].parse::<usize>().ok().map(|index| (index, end)))
        else {
            break;
        };
        counted.push_str(&rest[..at]);
        counted.push_str("look ");
        counted.push_str(&(index + 1).to_string());
        rest = &after[end + 1..];
    }
    counted.push_str(rest);
    counted
}

/// The softness of a piece's border, in px, when none is written.
const fn default_feather() -> f64 {
    0.5 // constant: the default `feather` of the Python `Each`, half a px smooths the stair-steps of the pixels
}

#[expect(clippy::trivially_copy_pass_by_ref, reason = "serde passes the field by reference")]
fn is_normal(mode: &BlendMode) -> bool {
    *mode == BlendMode::Normal
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

#[expect(clippy::trivially_copy_pass_by_ref, reason = "serde passes the field by reference")]
fn is_default_feather(feather: &f64) -> bool {
    feather.to_bits() == default_feather().to_bits()
}
