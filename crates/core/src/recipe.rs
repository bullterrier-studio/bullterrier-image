//! A look as data: a [`Recipe`] is an ordered list of [`Entry`]s, each an effect [`Step`] or an [`Each`] (a look for
//! every piece of a partition).
//!
//! The types are the whole model, built in code (`Step::new("bloom").param("radius", 12)`) or read from JSON
//! ([`Recipe::from_json`]); `serde_json` writes them back. A step keeps its parameters as written and says how its
//! result is laid over the picture below it: a `mask`, an `opacity` (none: the effect's own default) and a
//! [`BlendMode`]. Regions and the framing are tables the runner reads; here they are only tables.
//!
//! Reading and building check nothing but the shape: an unknown key is refused by name, a wrong type too. The rules of
//! a look (known effects, parameters within their ranges, names that resolve) are `Recipe::checked`.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use crate::Error;

mod check;
mod file;

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
    /// How the photo is framed before the steps and cut out after them: a table whose fields the runner reads.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub frame: Map<String, Value>,
    /// The regions the steps can name besides the built-in ones, by name: tables whose kind the runner reads.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub regions: Map<String, Value>,
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

    /// The recipe the JSON object `text` writes, in the form `serde_json` writes a recipe in. Only the shape is read:
    /// see `Recipe::checked` for the rules.
    ///
    /// # Errors
    /// [`Error::Recipe`] if the text is not JSON or not the shape of a recipe: an unknown or mistyped key is named.
    pub fn from_json(text: &str) -> Result<Self, Error> {
        let mut deserializer = serde_json::Deserializer::from_str(text);
        serde_path_to_error::deserialize(&mut deserializer)
            .map_err(|error| Error::Recipe(format!("not a recipe: {}", describe(&error))))
    }
}

/// The `T` that `value` writes, or what is wrong, naming the key it is about.
fn read<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_path_to_error::deserialize(value).map_err(|error| describe(&error))
}

/// `error` led by the key it is about (`opacity`, `steps[1]`), when it is about one.
fn describe<E: std::fmt::Display>(error: &serde_path_to_error::Error<E>) -> String {
    let path = error.path().to_string();
    if path == "." {
        error.inner().to_string()
    } else {
        format!("{path}: {}", error.inner())
    }
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn read(text: &Value) -> Result<Recipe, Error> {
        Recipe::from_json(&text.to_string())
    }

    #[test]
    fn builders_set_what_they_say_and_leave_the_rest_at_its_default() {
        let step = Step::new("bloom")
            .param("radius", 12)
            .param("threshold", 0.25)
            .mask("subject")
            .opacity(0.4)
            .blend_mode(BlendMode::Screen);
        assert_eq!(step.effect, "bloom");
        assert_eq!(
            Value::Object(step.params.clone()),
            json!({"radius": 12, "threshold": 0.25})
        );
        assert_eq!(
            (step.mask.as_deref(), step.opacity, step.blend_mode),
            (Some("subject"), Some(0.4), BlendMode::Screen)
        );
        assert_eq!((step.preset, step.partition, step.name), (None, None, None));
        let plain = Step::new("grade");
        assert_eq!(
            (plain.opacity, plain.blend_mode, plain.params.len()),
            (None, BlendMode::Normal, 0)
        );
        // a later call for the same key replaces it
        assert_eq!(Step::new("a").param("k", 1).param("k", 2).params["k"], json!(2));
    }

    #[test]
    fn a_recipe_is_written_in_the_form_a_person_would_and_reads_back_equal() {
        let mut each = Each::new("panes", [Some(Step::new("grade").param("exposure", 0.5)), None]);
        each.assign = Assign::Random;
        each.seed = 7;
        each.feather = 3.0;
        each.mask = Some("background".to_owned());
        each.opacity = Some(0.5);
        each.blend_mode = BlendMode::Multiply;
        each.name = Some("tiles".to_owned());
        let mut seams = Step::new("seams");
        seams.partition = Some("panes".to_owned());
        seams.preset = Some("thin".to_owned());
        seams.name = Some("edges".to_owned());
        let mut recipe = Recipe::new([
            Entry::from(Step::new("bloom").param("radius", 12).opacity(0.4)),
            seams.into(),
            each.into(),
        ]);
        recipe.name = Some("night".to_owned());
        recipe.description = Some("Dark and glowing.".to_owned());
        recipe.size = Some(1024);
        recipe.frame = json!({"aspect": 1}).as_object().cloned().unwrap_or_default();
        recipe.regions = json!({"panes": {"kind": "grid", "columns": 2}})
            .as_object()
            .cloned()
            .unwrap_or_default();
        let written = json!({
            "steps": [
                {"effect": "bloom", "params": {"radius": 12}, "opacity": 0.4},
                {"effect": "seams", "preset": "thin", "partition": "panes", "name": "edges"},
                {"each": "panes", "looks": [{"effect": "grade", "params": {"exposure": 0.5}}, null],
                 "assign": "random", "seed": 7, "feather": 3.0, "mask": "background", "opacity": 0.5,
                 "blend_mode": "multiply", "name": "tiles"},
            ],
            "name": "night", "description": "Dark and glowing.", "size": 1024,
            "frame": {"aspect": 1}, "regions": {"panes": {"kind": "grid", "columns": 2}},
        });
        assert_eq!(serde_json::to_value(&recipe).ok(), Some(written.clone()));
        assert_eq!(read(&written).ok(), Some(recipe));
    }

    #[test]
    fn what_is_left_at_its_default_is_not_written_and_is_read_as_the_default() {
        let recipe = Recipe::new([Step::new("grade"), Step::new("pattern")]);
        assert_eq!(
            serde_json::to_value(&recipe).ok(),
            Some(json!({"steps": [{"effect": "grade"}, {"effect": "pattern"}]}))
        );
        let each = read(&json!({"steps": [{"each": "p", "looks": []}]}));
        assert_eq!(
            each.ok().map(|recipe| recipe.steps),
            Some(vec![Entry::Each(Each::new("p", []))]),
        );
        let each = Each::new("p", []);
        assert_eq!(
            (each.assign, each.seed, each.blend_mode),
            (Assign::Cycle, 0, BlendMode::Normal)
        );
        assert!((each.feather - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn a_whole_number_stays_whole_and_a_decimal_stays_decimal() {
        let recipe = Recipe::new([Step::new("a").param("whole", 4).param("decimal", 4.0)]);
        let text = serde_json::to_string(&recipe).ok();
        assert_eq!(
            text.as_deref(),
            Some(r#"{"steps":[{"effect":"a","params":{"decimal":4.0,"whole":4}}]}"#)
        );
        assert_eq!(
            Recipe::from_json(text.as_deref().unwrap_or_default()).ok(),
            Some(recipe)
        );
    }

    #[test]
    fn an_entry_is_an_each_by_its_each_key_and_a_step_by_its_effect() {
        let steps = read(&json!({"steps": [{"effect": "grade"}, {"each": "p", "looks": [null]}]})).map(|r| r.steps);
        assert!(
            matches!(steps.as_deref(), Ok([Entry::Step(_), Entry::Each(_)])),
            "{steps:?}"
        );
        let neither = read(&json!({"steps": [{"mask": "subject"}]}));
        assert!(
            matches!(&neither, Err(Error::Recipe(message)) if message.contains("`effect`") && message.contains("`each`")),
            "{neither:?}"
        );
    }

    #[test]
    fn a_key_that_is_not_the_models_is_refused_by_name() {
        for (what, text, key) in [
            (
                "a parameter beside the step's keys",
                json!({"steps": [{"effect": "bloom", "radius": 12}]}),
                "radius",
            ),
            ("an unknown recipe key", json!({"steps": [], "format": 3}), "format"),
            (
                "an unknown key of an each step",
                json!({"steps": [{"each": "p", "looks": [], "count": 2}]}),
                "count",
            ),
            (
                "an unknown key of a look",
                json!({"steps": [{"each": "p", "looks": [{"effect": "a", "each": "x"}]}]}),
                "each",
            ),
        ] {
            let result = read(&text);
            assert!(
                matches!(&result, Err(Error::Recipe(message)) if message.contains(key)),
                "{what}: {result:?}"
            );
        }
    }

    #[test]
    fn a_value_of_the_wrong_type_is_refused_naming_the_key() {
        for (what, text, key) in [
            (
                "an opacity as text",
                json!({"steps": [{"effect": "a", "opacity": "half"}]}),
                "opacity",
            ),
            (
                "a blend mode that is none of the seven",
                json!({"steps": [{"effect": "a", "blend_mode": "burn"}]}),
                "blend_mode",
            ),
            (
                "an assign that is none of the two",
                json!({"steps": [{"each": "p", "looks": [], "assign": "shuffle"}]}),
                "assign",
            ),
            (
                "a seed beyond 32 bits",
                json!({"steps": [{"each": "p", "looks": [], "seed": 4_294_967_296_u64}]}),
                "seed",
            ),
            ("a negative size", json!({"steps": [], "size": -1}), "size"),
            ("steps that are not a list", json!({"steps": {"effect": "a"}}), "steps"),
            (
                "a look that is not a step",
                json!({"steps": [{"each": "p", "looks": [{"effect": 3}]}]}),
                "effect",
            ),
        ] {
            let result = read(&text);
            assert!(
                matches!(&result, Err(Error::Recipe(message)) if message.contains(key)),
                "{what}: {result:?}"
            );
        }
        assert!(matches!(read(&json!("steps")), Err(Error::Recipe(_))));
        assert!(matches!(Recipe::from_json("{steps"), Err(Error::Recipe(_))));
        // the largest seed is allowed
        let seed = read(&json!({"steps": [{"each": "p", "looks": [], "seed": 4_294_967_295_u32}]}));
        assert!(seed.is_ok(), "{seed:?}");
    }

    /// The place of `mode` in `ALL`. The match has no wildcard: a new mode stops this compiling until it is placed here.
    const fn place(mode: BlendMode) -> usize {
        match mode {
            BlendMode::Normal => 0,
            BlendMode::Multiply => 1,
            BlendMode::Screen => 2,
            BlendMode::Overlay => 3,
            BlendMode::SoftLight => 4,
            BlendMode::Difference => 5,
            BlendMode::Color => 6,
        }
    }

    #[test]
    fn all_lists_every_blend_mode_once_in_order() {
        let places: Vec<usize> = BlendMode::ALL.iter().map(|mode| place(*mode)).collect();
        assert_eq!(places, (0..7).collect::<Vec<_>>());
    }

    #[test]
    fn the_blend_modes_are_the_librarys_seven_in_recipe_order() {
        let names: Vec<&str> = BlendMode::ALL.iter().map(|mode| mode.name()).collect();
        assert_eq!(
            names,
            [
                "normal",
                "multiply",
                "screen",
                "overlay",
                "soft_light",
                "difference",
                "color"
            ]
        );
        for mode in BlendMode::ALL {
            let read: Result<BlendMode, _> = serde_json::from_value(json!(mode.name()));
            assert_eq!(read.ok(), Some(*mode));
        }
    }
}
