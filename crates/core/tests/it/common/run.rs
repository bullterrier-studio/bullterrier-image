//! What the integration tests of the run area share: effects of the tests' own, written against the public [`Effect`]
//! trait, that join the core's registry under a name of their own and leave it when dropped; a finder that counts its
//! calls; and the parameter tables of effects that describe theirs.
//!
//! The registry is the process's, shared by every test of the binary, which run at once: each double therefore takes
//! a name no other has ([`unique`]); a test asks about the effects it registered, never about the whole registry.

use std::collections::BTreeMap;
use std::ops::Deref;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use bullterrier_image::params::{Field, Kind, Scalar, Table, Unit};
use bullterrier_image::{
    Category, Effect, Error, FieldParam, Finder, Image, Input, Inputs, Layer, Picture, Place, Regions, register,
    unregister,
};
use serde_json::Value;

/// A name no other call of this process has, beginning with `prefix`.
pub(crate) fn unique(prefix: &str) -> String {
    static NAMES: AtomicUsize = AtomicUsize::new(0);
    format!("{prefix}_{}", NAMES.fetch_add(1, Ordering::Relaxed))
}

/// The regions the JSON table `declared` writes (name to a table with its `kind`), in its order. A table that is no
/// regions is a mistake of the test, so it is an empty declaration, which the test then notices.
pub(crate) fn regions(declared: Value) -> Regions {
    serde_json::from_value(declared).unwrap_or_default()
}

/// An effect in the registry for as long as this guard lives.
pub(crate) struct Registered<E: Effect + 'static> {
    effect: Arc<E>,
}

impl<E: Effect + 'static> Registered<E> {
    /// `effect` added to the registry under its name.
    pub(crate) fn new(effect: E) -> Result<Self, Error> {
        let effect = Arc::new(effect);
        register(effect.clone())?;
        Ok(Self { effect })
    }

    /// The effect itself, to hand to something that outlives the guard's borrow.
    pub(crate) fn shared(&self) -> Arc<E> {
        Arc::clone(&self.effect)
    }

    /// The name steps know the effect by.
    pub(crate) fn name(&self) -> &str {
        self.effect.name()
    }
}

impl<E: Effect + 'static> Deref for Registered<E> {
    type Target = E;

    fn deref(&self) -> &E {
        &self.effect
    }
}

impl<E: Effect + 'static> Drop for Registered<E> {
    fn drop(&mut self) {
        unregister(self.effect.name());
    }
}

/// A foreign effect that paints the picture with the `level` of its parameters; its steps default to an opacity of 0.25
/// and it declares the size ratio.
pub(crate) struct Paint {
    name: String,
}

impl Paint {
    /// A painter with a name of its own.
    pub(crate) fn new() -> Self {
        Self::named(&unique("paint"))
    }

    /// A painter called `name`.
    pub(crate) fn named(name: &str) -> Self {
        Self { name: name.to_owned() }
    }
}

impl Effect for Paint {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn opacity(&self) -> f64 {
        0.25
    }

    fn inputs(&self) -> &[Input] {
        &[Input::SizeRatio]
    }

    fn params(&self) -> Table {
        let mut level = field("level", Kind::Float, Scalar::Float(0.5), "The grey it paints.");
        level.drivable = true;
        Table {
            doc: "A painter.",
            fields: vec![level],
        }
    }

    fn apply(&self, picture: &Picture<'_>, params: &Value, _inputs: &Inputs<'_>) -> Result<Image, Error> {
        let level = params["level"]
            .as_f64()
            .ok_or_else(|| Error::foreign(&self.name, "no level"))?;
        #[expect(clippy::cast_possible_truncation, reason = "a test level is a small number")]
        Ok(picture.map(|_| [level as f32; 3]))
    }
}

/// What a [`Recorder`] was handed by one `apply`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Received {
    pub(crate) picture: Vec<f32>,
    pub(crate) params: Value,
    pub(crate) size_ratio: f64,
    pub(crate) place: Place,
    pub(crate) partition: Option<Vec<i32>>,
    pub(crate) fields: BTreeMap<String, Vec<f32>>,
}

/// A foreign effect that declares the inputs it is made with, logs the parameters and inputs it receives and returns
/// the picture as it is: the oracle for "an undeclared input is absent" and for the parameters a step resolves to. It
/// describes no parameters unless given a table.
pub(crate) struct Recorder {
    name: String,
    inputs: &'static [Input],
    fields: Vec<FieldParam>,
    table: Option<Table>,
    log: Mutex<Vec<Received>>,
}

impl Recorder {
    /// A recorder with a name of its own that declares `inputs` and one field parameter for each of `fields`, whose
    /// default field is the parameter's name upcased.
    pub(crate) fn new(inputs: &'static [Input], fields: &[&str]) -> Self {
        Self {
            name: unique("recorder"),
            inputs,
            fields: fields
                .iter()
                .map(|parameter| FieldParam::new(*parameter, parameter.to_uppercase()))
                .collect(),
            table: None,
            log: Mutex::new(Vec::new()),
        }
    }

    /// This recorder describing its parameters by `table`, as a core effect does: the core then checks and scales a
    /// step's values before it sees them.
    pub(crate) fn with_table(mut self, table: Table) -> Self {
        self.table = Some(table);
        self
    }

    /// What each `apply` so far received, in order.
    pub(crate) fn received(&self) -> Vec<Received> {
        self.log.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// What each `apply` so far received as the field parameter `parameter`, in order.
    pub(crate) fn seen(&self, parameter: &str) -> Vec<Vec<f32>> {
        self.received()
            .into_iter()
            .filter_map(|received| received.fields.get(parameter).cloned())
            .collect()
    }

    /// Forget what was received so far.
    pub(crate) fn clear(&self) {
        self.log.lock().unwrap_or_else(PoisonError::into_inner).clear();
    }
}

impl Effect for Recorder {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn inputs(&self) -> &[Input] {
        self.inputs
    }

    fn fields(&self) -> &[FieldParam] {
        &self.fields
    }

    fn params(&self) -> Table {
        self.table.clone().unwrap_or_else(|| table_of_fields(&self.fields))
    }

    fn apply(&self, picture: &Picture<'_>, params: &Value, inputs: &Inputs<'_>) -> Result<Image, Error> {
        let fields = self
            .fields
            .iter()
            .filter_map(|field| {
                Some((
                    field.parameter.clone(),
                    inputs.field(&field.parameter)?.values().to_vec(),
                ))
            })
            .collect();
        self.log.lock().unwrap_or_else(PoisonError::into_inner).push(Received {
            picture: picture.values().to_vec(),
            params: params.clone(),
            size_ratio: inputs.size_ratio(),
            place: inputs.place(picture),
            partition: inputs.partition().map(|labels| labels.values().to_vec()),
            fields,
        });
        Ok(picture.map(|pixel| pixel))
    }
}

/// A foreign effect that declares a partition, returns the picture as it is and counts its calls: the oracle for "the
/// effect was never run".
pub(crate) struct Counting {
    name: String,
    calls: AtomicUsize,
}

impl Counting {
    /// A counter with a name of its own.
    pub(crate) fn new() -> Self {
        Self {
            name: unique("counting"),
            calls: AtomicUsize::new(0),
        }
    }

    /// How many times `apply` ran.
    pub(crate) fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Effect for Counting {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn inputs(&self) -> &[Input] {
        &[Input::Partition]
    }

    fn apply(&self, picture: &Picture<'_>, _params: &Value, _inputs: &Inputs<'_>) -> Result<Image, Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(picture.map(|pixel| pixel))
    }
}

/// A foreign effect that breaks the contract: it returns a single pixel, whatever the size of the picture.
pub(crate) struct Shrinks {
    name: String,
}

impl Shrinks {
    /// A shrinker with a name of its own.
    pub(crate) fn new() -> Self {
        Self {
            name: unique("shrinks"),
        }
    }
}

impl Effect for Shrinks {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn apply(&self, _picture: &Picture<'_>, _params: &Value, _inputs: &Inputs<'_>) -> Result<Image, Error> {
        Image::new(1, 1, &[0.0; 3])
    }
}

/// What a [`Finds`] answers: a map, or an error, for the picture it is asked about.
type Answer = Box<dyn Fn(&Picture<'_>) -> Result<Layer, Error> + Send + Sync>;

/// A finder that answers with what `answer` makes of the picture and counts its calls: the oracle for "the map was found
/// once" and "the finder was never asked".
pub(crate) struct Finds {
    answer: Answer,
    calls: AtomicUsize,
}

impl Finds {
    /// A finder whose answer to every picture is `answer` of it.
    pub(crate) fn new(answer: impl Fn(&Picture<'_>) -> Result<Layer, Error> + Send + Sync + 'static) -> Self {
        Self {
            answer: Box::new(answer),
            calls: AtomicUsize::new(0),
        }
    }

    /// A finder that answers a map of `level` everywhere, of the size of the picture it is asked about.
    pub(crate) fn flat(level: f32) -> Self {
        Self::new(move |picture| {
            Layer::new(
                picture.height(),
                picture.width(),
                vec![level; picture.height() * picture.width()],
            )
        })
    }

    /// How many times the finder was asked.
    pub(crate) fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Finder for Finds {
    fn find(&self, picture: &Picture<'_>) -> Result<Layer, Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        (self.answer)(picture)
    }
}

/// The table of an effect whose only parameters are its field parameters, each named by `fields` (the names live as long
/// as the test).
pub(crate) fn table_of_fields(fields: &[FieldParam]) -> Table {
    let leak = |text: &str| -> &'static str { Box::leak(text.to_owned().into_boxed_str()) };
    Table {
        doc: "An effect with field parameters.",
        fields: fields
            .iter()
            .map(|each| {
                field(
                    leak(&each.parameter),
                    Kind::Field,
                    Scalar::Text(leak(&each.default)),
                    "A selection.",
                )
            })
            .collect(),
    }
}

/// The table of an effect with the drivable decimals `x` and `y` and the whole number `z`.
pub(crate) fn xyz_table() -> Table {
    let mut x = field("x", Kind::Float, Scalar::Float(0.0), "A decimal.");
    x.drivable = true;
    let mut y = field("y", Kind::Float, Scalar::Float(0.0), "Another decimal.");
    y.drivable = true;
    Table {
        doc: "x, y and z.",
        fields: vec![x, y, field("z", Kind::Int, Scalar::Int(0), "A whole number.")],
    }
}

/// A parameter of the tests' tables, described.
fn field(name: &'static str, kind: Kind, default: Scalar, meaning: &'static str) -> Field {
    Field {
        name,
        kind,
        meaning,
        default,
        unit: None,
        min: None,
        max: None,
        cyclic: false,
        choices: &[],
        min_length: None,
        max_length: None,
        min_distinct: None,
        at_most: None,
        nullable: false,
        drivable: false,
        advanced: false,
    }
}

/// The parameters of an effect with one of every kind: a length in px, a count, a direction, a choice, a switch and a
/// colour.
pub(crate) fn probe_table() -> Table {
    Table {
        doc: "The parameters of a probe effect.",
        fields: vec![
            Field {
                unit: Some(Unit::Px),
                min: Some(Scalar::Int(2)),
                max: Some(Scalar::Int(512)),
                drivable: true,
                ..field("cell", Kind::Float, Scalar::Float(32.0), "The size of one cell in px.")
            },
            Field {
                unit: Some(Unit::Count),
                min: Some(Scalar::Int(-3)),
                max: Some(Scalar::Int(32)),
                ..field("cells", Kind::Int, Scalar::Int(4), "How many cells.")
            },
            Field {
                unit: Some(Unit::Degrees),
                min: Some(Scalar::Int(0)),
                max: Some(Scalar::Int(360)),
                cyclic: true,
                drivable: true,
                ..field("angle", Kind::Float, Scalar::Float(0.5), "The direction.")
            },
            Field {
                choices: &["stripes", "checks", "fourth_way"],
                advanced: true,
                ..field(
                    "shape",
                    Kind::Choice,
                    Scalar::Text("fourth_way"),
                    "The shape: stripes, checks or fourth_way.",
                )
            },
            field("keep", Kind::Bool, Scalar::Bool(true), "Whether it is kept."),
            field("tint", Kind::Color, Scalar::Text("#ff8000"), "The colour."),
        ],
    }
}

/// The parameters of an effect that measures: a length in px, at least 2 and at most 100, a drive may vary.
pub(crate) fn length_table() -> Table {
    Table {
        doc: "The parameters of an effect that measures.",
        fields: vec![Field {
            unit: Some(Unit::Px),
            min: Some(Scalar::Int(2)),
            max: Some(Scalar::Int(100)),
            drivable: true,
            ..field("length", Kind::Float, Scalar::Float(10.0), "A length in px.")
        }],
    }
}
