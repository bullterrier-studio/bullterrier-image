//! One step of a recipe: [`prepare`] resolves it (its parameters complete and scaled, the regions it names), a
//! [`Prepared`] runs its effect on a picture and lays the result over it.

use std::sync::Arc;

use serde_json::{Map, Value};

use super::assets::Assets;
use super::each::EachRun;
use super::regions::Evaluator;
use crate::effect::{self, Effect};
use crate::ops::{blend, levels};
use crate::params::Kind;
use crate::recipe::drive;
use crate::registry::Registry;
use crate::{BlendMode, Entry, Error, Image, Inputs, Labels, Layer, Lut, Picture, Place, Presets, Step};

/// The entry `entry` (at `place`, as `step 2`) resolved: for a step its parameters complete and scaled, the regions it
/// names, for an `each` step its looks the same.
pub(super) fn prepare(
    registry: &Registry,
    presets: &Presets,
    ratio: f64,
    assets: &mut Assets<'_>,
    entry: &Entry,
    place: &str,
) -> Result<Prepared, Error> {
    Ok(match entry {
        Entry::Step(step) => Prepared::Step(StepRun::new(registry, presets, ratio, assets, step, place)?),
        Entry::Each(each) => Prepared::Each(EachRun::new(registry, presets, ratio, assets, each, place)?),
    })
}

/// An entry of a recipe ready to run.
pub(super) enum Prepared {
    Step(StepRun),
    Each(EachRun),
}

impl Prepared {
    /// This entry run on `before`, with the selections it names made by `regions` for `working`, the picture the render
    /// works on, and the result laid over `before`; `ratio` is the working picture's size over the design size.
    /// `before` is `working`, or, for a step with a [`margin`](Self::margin), the rows of it from `at` on (an `each`
    /// step is always run on all of it, at 0): the maps the step names are cut to those rows and it is told its place.
    ///
    /// # Errors
    /// Why a region could not be made, or an effect failed.
    pub(super) fn apply(
        &self,
        before: &Picture<'_>,
        ratio: f64,
        regions: &mut Evaluator,
        working: &Picture<'_>,
        at: usize,
    ) -> Result<Image, Error> {
        match self {
            Self::Step(step) => step.apply(before, ratio, regions, working, at),
            Self::Each(each) => each.apply(before, ratio, regions, working),
        }
    }

    /// The rows of margin this entry needs above and below the rows it makes, or none if it reads the whole
    /// picture: an `each` step, a driven step and an effect that says [`Whole`](crate::Footprint::Whole).
    ///
    /// # Errors
    /// [`Error::Params`] if the effect refuses its own parameters.
    pub(super) fn margin(&self) -> Result<Option<usize>, Error> {
        match self {
            Self::Step(step) => step.margin(),
            Self::Each(_) => Ok(None),
        }
    }
}

/// The complete parameters a step's effect is run with.
enum Runs {
    /// A plain step: one run, with the values completed and scaled to the picture (what a footprint is asked about).
    One { scaled: Value },
    /// A step with driven parameters: one run for each level, `levels[n]` for level `n` (0 to `top`), mixed by the
    /// selection called `by`.
    Driven { by: String, top: u8, levels: Vec<Value> },
}

/// How the result of a step is laid over its input: by a blend mode, mixed in by an opacity and, where there is a `mask`,
/// only as far as the mask says.
pub(super) struct Laid {
    opacity: f64,
    blend_mode: BlendMode,
    /// The name of the region that limits it.
    mask: Option<String>,
}

impl Laid {
    pub(super) fn new(opacity: f64, blend_mode: BlendMode, mask: Option<&String>) -> Self {
        Self {
            opacity,
            blend_mode,
            mask: mask.cloned(),
        }
    }

    /// `after`, the result for `before` (the rows of `working` from `at`), laid over `before`. A plain step (blend mode normal, opacity 1, no mask) is its
    /// result as it is, uncopied.
    pub(super) fn over(
        &self,
        before: &Picture<'_>,
        after: Image,
        regions: &mut Evaluator,
        working: &Picture<'_>,
        at: usize,
    ) -> Result<Image, Error> {
        let mask = self
            .mask
            .as_ref()
            .map(|mask| {
                regions
                    .selection(mask, working)
                    .map(|mask| rows_of(mask, at, before.height()))
            })
            .transpose()?;
        Ok(
            if self.opacity >= 1.0 && mask.is_none() && self.blend_mode == BlendMode::Normal {
                after
            } else {
                blend::lay(
                    before,
                    &after.as_picture(),
                    self.blend_mode,
                    self.opacity,
                    mask.as_deref(),
                )
            },
        )
    }
}

/// A step ready to run: its effect, the complete parameters it receives, the selections it works from (by region name)
/// and how its result is laid over its input.
pub(super) struct StepRun {
    effect: Arc<dyn Effect>,
    runs: Runs,
    /// The field parameters of the effect, each with the name of its region.
    fields: Vec<(String, String)>,
    /// The region whose pieces the effect receives, if it takes a partition.
    partition: Option<String>,
    /// The lookup table parameters of the effect that name a table, each with its table.
    luts: Vec<(String, Arc<Lut>)>,
    /// The picture parameters of the effect that name a picture, each with its picture.
    pictures: Vec<(String, Arc<Image>)>,
    laid: Laid,
}

/// The parameters `effect` (called `name`) is run with for the values `given`: for an effect that describes its
/// parameters, checked and scaled to `ratio`, else as given.
fn complete(
    effect: &dyn Effect,
    name: &str,
    given: Map<String, Value>,
    ratio: f64,
    place: &str,
) -> Result<Value, Error> {
    Ok(match effect.params() {
        Some(table) => {
            let design = table.check(name, &given).map_err(|error| error.at(place))?;
            Value::Object(table.scaled(design, ratio))
        }
        None => Value::Object(given),
    })
}

/// The table each lookup table parameter of `effect` names in `runs` (the parameters are the same at every level of a
/// drive: a table is not drivable), found by `assets`; a parameter with no name is the identity table and has none.
///
/// # Errors
/// [`Error::Lut`] if a name is no table, placed at the step and the parameter.
fn tables(
    effect: &dyn Effect,
    runs: &Runs,
    assets: &mut Assets<'_>,
    place: &str,
) -> Result<Vec<(String, Arc<Lut>)>, Error> {
    let Some(table) = effect.params() else {
        return Ok(Vec::new());
    };
    let params = match runs {
        Runs::One { scaled: params } => params,
        Runs::Driven { levels, .. } => &levels[0],
    };
    let mut found = Vec::new();
    for field in table.fields.iter().filter(|field| field.kind == Kind::Lut) {
        if let Some(name) = params[field.name].as_str() {
            let lut = assets.lut(name).map_err(|error| match error {
                Error::Lut(why) => Error::Lut(format!("{place}: effect '{}': `{}`: {why}", effect.name(), field.name)),
                other => other,
            })?;
            found.push((field.name.to_owned(), lut));
        }
    }
    Ok(found)
}

/// The picture each picture parameter of `effect` names in `runs` (a picture is not drivable), found by `assets`; a
/// parameter with no name is no picture and has none.
///
/// # Errors
/// [`Error::ImageRead`] if a name is no picture and no readable image file, placed at the step and the parameter.
fn pictures(
    effect: &dyn Effect,
    runs: &Runs,
    assets: &mut Assets<'_>,
    place: &str,
) -> Result<Vec<(String, Arc<Image>)>, Error> {
    let Some(table) = effect.params() else {
        return Ok(Vec::new());
    };
    let params = match runs {
        Runs::One { scaled: params } => params,
        Runs::Driven { levels, .. } => &levels[0],
    };
    let mut found = Vec::new();
    for field in table.fields.iter().filter(|field| field.kind == Kind::Picture) {
        if let Some(name) = params[field.name].as_str() {
            let image = assets.image(name).map_err(|error| match error {
                Error::ImageRead(why) => {
                    Error::ImageRead(format!("{place}: effect '{}': `{}`: {why}", effect.name(), field.name))
                }
                other => other,
            })?;
            found.push((field.name.to_owned(), image));
        }
    }
    Ok(found)
}

impl StepRun {
    /// The step `step` (at `place`, as `step 2`) resolved.
    pub(super) fn new(
        registry: &Registry,
        presets: &Presets,
        ratio: f64,
        assets: &mut Assets<'_>,
        step: &Step,
        place: &str,
    ) -> Result<Self, Error> {
        let effect = registry.effect(&step.effect)?;
        let given = presets.under(step, place)?;
        let mut fields = Vec::new();
        for field in effect.fields() {
            let name = field
                .field_in(&given)
                .map_err(|why| Error::Recipe(format!("{place}: {why}")))?;
            fields.push((field.parameter.clone(), name.to_owned()));
        }
        let drives = drive::in_params(&step.effect, &given, place)?;
        let runs = if let Some((_, first)) = drives.first() {
            // one run for each level of the drives, each with its values checked and scaled like a plain step's
            let levels = (0..first.levels)
                .map(|level| {
                    let mut values = given.clone();
                    for (name, drive) in &drives {
                        values.insert(name.clone(), Value::from(drive.value(level)));
                    }
                    complete(&*effect, &step.effect, values, ratio, place)
                })
                .collect::<Result<_, _>>()?;
            Runs::Driven {
                by: first.by.clone(),
                top: first.levels - 1,
                levels,
            }
        } else {
            Runs::One {
                scaled: complete(&*effect, &step.effect, given, ratio, place)?,
            }
        };
        let luts = tables(&*effect, &runs, assets, place)?;
        let pictures = pictures(&*effect, &runs, assets, place)?;
        Ok(Self {
            luts,
            pictures,
            laid: Laid::new(
                step.opacity.unwrap_or_else(|| effect.opacity()),
                step.blend_mode,
                step.mask.as_ref(),
            ),
            partition: step.partition.clone(),
            fields,
            effect,
            runs,
        })
    }

    /// The rows of margin this step needs above and below the rows it makes (see
    /// [`Footprint::margin`](crate::Footprint::margin)), or none if it reads the whole picture. A driven step is whole:
    /// it mixes runs by a map, and its footprint is the widest of its levels', which is not worked out.
    ///
    /// # Errors
    /// [`Error::Params`] if the effect refuses its own parameters.
    pub(super) fn margin(&self) -> Result<Option<usize>, Error> {
        match &self.runs {
            Runs::One { scaled } => Ok(self.effect.footprint(scaled)?.margin()),
            Runs::Driven { .. } => Ok(None),
        }
    }

    /// This step's effect run on `before`, which is the rows of `working` from `at` on (all of it when `at` is 0 and
    /// `before` is as tall), with the selections it names made by `regions` for `working` and cut to those rows, and the
    /// result laid over `before`.
    pub(super) fn apply(
        &self,
        before: &Picture<'_>,
        ratio: f64,
        regions: &mut Evaluator,
        working: &Picture<'_>,
        at: usize,
    ) -> Result<Image, Error> {
        let rows = before.height();
        let mut fields = Vec::with_capacity(self.fields.len());
        for (parameter, region) in &self.fields {
            fields.push((parameter, rows_of(regions.selection(region, working)?, at, rows)));
        }
        let mut offered =
            Inputs::default()
                .with_size_ratio(ratio)
                .with_place(Place::new(working.height(), working.width(), at));
        for (parameter, layer) in &fields {
            offered = offered.with_field(parameter.as_str(), layer);
        }
        for (parameter, lut) in &self.luts {
            offered = offered.with_lut(parameter.as_str(), lut);
        }
        for (parameter, image) in &self.pictures {
            offered = offered.with_image(parameter.as_str(), image);
        }
        let partition = self
            .partition
            .as_ref()
            .map(|region| {
                regions
                    .pieces(region, working)
                    .map(|pieces| pieces_of(pieces, at, rows))
            })
            .transpose()?;
        if let Some(pieces) = &partition {
            offered = offered.with_partition(pieces);
        }
        let after = match &self.runs {
            Runs::One { scaled } => effect::run(&*self.effect, before, scaled.clone(), &offered)?,
            Runs::Driven { by, top, levels } => {
                let by = rows_of(regions.selection(by, working)?, at, rows);
                self.driven(before, &offered, &by, *top, levels)?
            }
        };
        self.laid.over(before, after, regions, working, at)
    }

    /// The effect run on `before` for each level of the drive that some pixel is on or next to, and each pixel mixes the
    /// runs of the two levels either side of its value in `by`, linearly; a level none is near is not run.
    fn driven(
        &self,
        before: &Picture<'_>,
        offered: &Inputs<'_>,
        by: &Layer,
        top: u8,
        levels: &[Value],
    ) -> Result<Image, Error> {
        let positions: Vec<f32> = by.values().iter().map(|value| value * f32::from(top)).collect();
        let mut mixed = vec![[0.0_f32; 3]; positions.len()];
        for (level, params) in (0..=top).zip(levels) {
            let weights: Vec<f32> = positions
                .iter()
                .map(|&position| levels::weight(position, level))
                .collect();
            if weights.iter().all(|&weight| weight <= 0.0) {
                continue;
            }
            let run = effect::run(&*self.effect, before, params.clone(), offered)?;
            for ((out, pixel), weight) in mixed.iter_mut().zip(run.as_picture().pixels()).zip(&weights) {
                *out = std::array::from_fn(|channel| out[channel] + weight * pixel[channel]);
            }
        }
        Ok(Image::from_pixels(before.height(), before.width(), mixed))
    }
}

/// `layer`, a map of the working picture, cut to the `rows` rows from `at`: the map itself if that is all of it.
fn rows_of(layer: Arc<Layer>, at: usize, rows: usize) -> Arc<Layer> {
    if rows == layer.height() {
        layer
    } else {
        Arc::new(layer.rows(at, at + rows))
    }
}

/// As [`rows_of`], for the pieces of a partition.
fn pieces_of(pieces: Arc<Labels>, at: usize, rows: usize) -> Arc<Labels> {
    if rows == pieces.height() {
        pieces
    } else {
        Arc::new(pieces.rows(at, at + rows))
    }
}
