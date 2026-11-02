//! What the integration tests of the recipe area share: doubles written against the public [`Effect`] trait and the
//! global registry, and the declared regions a JSON table writes.
//!
//! The registry is the process's: a test that needs an effect of its own registers one under a name no other test uses
//! ([`Double::register`]); the three that recipes name by a fixed name (`probe`, `seams`, `fielded`) are registered once
//! by [`setup`].

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use bullterrier_image::params::{Field, Kind, Scalar, Table, Unit};
use bullterrier_image::{Category, Effect, Error, FieldParam, Image, Input, Inputs, Picture, Regions, register};
use serde_json::{Value, json};

/// What a [`Double`] was handed by one `apply`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Received {
    pub(crate) params: Value,
    pub(crate) size_ratio: f64,
}

/// A foreign effect that declares what it is made with, logs the parameters and the size ratio it receives and returns
/// the picture as it is: the oracle for the parameters a step resolves to.
pub(crate) struct Double {
    name: String,
    category: Category,
    inputs: &'static [Input],
    fields: Vec<FieldParam>,
    table: Option<Table>,
    log: Mutex<Vec<Received>>,
}

impl Double {
    /// A double called `name` that declares nothing and describes no parameters.
    pub(crate) fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            category: Category::Utilities,
            inputs: &[],
            fields: Vec::new(),
            table: None,
            log: Mutex::new(Vec::new()),
        }
    }

    /// A double with a name no other double of this process has.
    pub(crate) fn unique() -> Self {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        Self::new(format!("double_{}", COUNT.fetch_add(1, Ordering::SeqCst)))
    }

    /// This double in `category`.
    pub(crate) fn in_category(mut self, category: Category) -> Self {
        self.category = category;
        self
    }

    /// This double declaring `inputs`.
    pub(crate) fn with_inputs(mut self, inputs: &'static [Input]) -> Self {
        self.inputs = inputs;
        self
    }

    /// This double with a field parameter `parameter` whose default field is the name upcased.
    pub(crate) fn with_field(mut self, parameter: &str) -> Self {
        self.fields.push(FieldParam::new(parameter, parameter.to_uppercase()));
        self
    }

    /// This double describing its parameters by `table`, as a core effect does: a render then checks and scales a
    /// step's values before the double sees them.
    pub(crate) fn with_table(mut self, table: Table) -> Self {
        self.table = Some(table);
        self
    }

    /// Add this double to the registry and give it back, to read what it received.
    pub(crate) fn register(self) -> Result<Arc<Self>, Error> {
        let double = Arc::new(self);
        register(double.clone())?;
        Ok(double)
    }

    /// What each `apply` so far received, in order.
    pub(crate) fn received(&self) -> Vec<Received> {
        self.log.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

impl Effect for Double {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> Category {
        self.category
    }

    fn inputs(&self) -> &[Input] {
        self.inputs
    }

    fn fields(&self) -> &[FieldParam] {
        &self.fields
    }

    fn params(&self) -> Option<Table> {
        self.table.clone()
    }

    fn apply(&self, picture: &Picture<'_>, params: Value, inputs: &Inputs<'_>) -> Result<Image, Error> {
        self.log.lock().unwrap_or_else(PoisonError::into_inner).push(Received {
            params,
            size_ratio: inputs.size_ratio(),
        });
        Ok(picture.map(|pixel| pixel))
    }
}

/// Register the effects that recipes name by a fixed name, once however many tests ask: `probe` (one parameter of
/// every kind, described by [`probe_table`]), `seams` (takes a partition and describes no parameters) and `fielded`
/// (one field parameter, `near`, whose default field is `NEAR`).
pub(crate) fn setup() -> Result<(), Error> {
    static DONE: OnceLock<Result<(), String>> = OnceLock::new();
    DONE.get_or_init(|| {
        let shared = [
            Double::new("probe").with_table(probe_table()),
            Double::new("seams")
                .in_category(Category::Stylised)
                .with_inputs(&[Input::Partition]),
            Double::new("fielded").with_field("near"),
        ];
        for double in shared {
            double.register().map_err(|error| error.to_string())?;
        }
        Ok(())
    })
    .clone()
    .map_err(|message| Error::Recipe(format!("registering the test effects failed: {message}")))
}

/// The regions the JSON table `declared` writes (name to a table with its `kind`), in its order. A table that is no
/// regions is a mistake of the test, so it is an empty declaration, which the test then notices.
pub(crate) fn regions(declared: Value) -> Regions {
    serde_json::from_value(declared).unwrap_or_default()
}

/// A number or choice field, with what every test field shares.
const fn field(name: &'static str, kind: Kind, meaning: &'static str, default: Scalar) -> Field {
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
        drivable: false,
        advanced: false,
    }
}

/// A number field in `unit` from `min` to `max`.
const fn number(
    name: &'static str,
    kind: Kind,
    default: Scalar,
    unit: Unit,
    (min, max): (i64, i64),
    drivable: bool,
) -> Field {
    Field {
        unit: Some(unit),
        min: Some(Scalar::Int(min)),
        max: Some(Scalar::Int(max)),
        drivable,
        ..field(name, kind, "A number.", default)
    }
}

/// The parameters of the `probe` effect, one of every kind, written by hand: `cell` (a drivable decimal in px, 2 to
/// 512), `cells` (a whole count, -3 to 32), `angle` (a cyclic, drivable direction), `shape` (a choice), `keep` (a
/// switch) and `tint` (a colour).
pub(crate) fn probe_table() -> Table {
    let mut angle = number("angle", Kind::Float, Scalar::Float(0.5), Unit::Degrees, (0, 360), true);
    angle.cyclic = true;
    angle.meaning = "The direction: 'stripes' or 'checks'.";
    let mut cell = number("cell", Kind::Float, Scalar::Float(32.0), Unit::Px, (2, 512), true);
    cell.meaning = "The size of one cell in px, over two lines. Larger = coarser.";
    let mut cells = number("cells", Kind::Int, Scalar::Int(4), Unit::Count, (-3, 32), false);
    cells.meaning = "How many cells.";
    let mut shape = field("shape", Kind::Choice, "The shape.", Scalar::Text("fourth_way"));
    shape.choices = &["stripes", "checks", "fourth_way"];
    shape.advanced = true;
    Table {
        doc: "The parameters of a probe effect, which reads them as the Python package gives them.",
        fields: vec![
            cell,
            cells,
            angle,
            shape,
            field("keep", Kind::Bool, "Whether it is kept.", Scalar::Bool(true)),
            field("tint", Kind::Color, "The colour.", Scalar::Text("#ff8000")),
        ],
    }
}

/// The JSON a recipe file writes for a drive: `by`, `start` and `end`.
pub(crate) fn drive(by: &str, start: f64, end: f64) -> Value {
    json!({"by": by, "start": start, "end": end})
}
