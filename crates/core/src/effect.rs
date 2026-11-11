//! What an effect is to the core: a name, where it sits in the catalogue, how much of it a step shows by default, the
//! inputs it declares and a function from a picture and parameters to a new picture.
//!
//! The core's own effects are files in `src/effects/`; an effect that lives elsewhere (a Python function, say) is a
//! value of a type that implements [`Effect`] and joins them in the registry (see [`crate::register`]).

use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::params::{Declared, Kind, Table};
use crate::{Error, Footprint, Image, Inputs, Picture};

/// The sections of the effect catalogue. Every effect is in exactly one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Category {
    /// Grades, gradient maps, tints and hue changes.
    Colour,
    /// Bloom and local contrast.
    Light,
    /// Grain and other surface texture.
    Texture,
    /// Looks that redraw the picture in a printing or drawing style.
    Stylised,
    /// Pictures redrawn as characters.
    Text,
    /// Warps that bend, ripple or displace the geometry.
    Distortion,
    /// Digital-error looks.
    Glitch,
    /// Weather and air.
    Atmosphere,
    /// Effects that work on the cut-out subject.
    Subject,
    /// Ways of combining the picture with other layers.
    Compositing,
    /// Generated or replaced backgrounds.
    Backgrounds,
    /// Plain helpers such as resizing, cropping and borders.
    Utilities,
}

impl Category {
    /// Every category, in catalogue order.
    pub const ALL: &'static [Self] = &[
        Self::Colour,
        Self::Light,
        Self::Texture,
        Self::Stylised,
        Self::Text,
        Self::Distortion,
        Self::Glitch,
        Self::Atmosphere,
        Self::Subject,
        Self::Compositing,
        Self::Backgrounds,
        Self::Utilities,
    ];

    /// What the category holds, in a sentence.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Colour => {
                "Grades, gradient maps, tints and hue changes: they recolour the picture and leave its shapes alone."
            }
            Self::Light => "Bloom and local contrast: they add or shape the light in the picture.",
            Self::Texture => "Grain and other surface texture laid over the picture.",
            Self::Stylised => "Looks that redraw the picture in a printing or drawing style, such as halftone.",
            Self::Text => "Pictures redrawn as characters, such as ASCII art.",
            Self::Distortion => "Warps that bend, ripple or displace the picture's geometry.",
            Self::Glitch => "Digital-error looks: displaced slices, colour splits and corrupted blocks.",
            Self::Atmosphere => "Weather and air: fog, haze, rain and the like.",
            Self::Subject => "Effects that work on the cut-out subject, apart from its background.",
            Self::Compositing => "Ways of combining the picture with other layers.",
            Self::Backgrounds => "Generated or replaced backgrounds behind the subject.",
            Self::Utilities => "Plain helpers such as resizing, cropping and borders.",
        }
    }

    /// The category's name in recipes and in the catalogue: lowercase, as `"colour"`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Colour => "colour",
            Self::Light => "light",
            Self::Texture => "texture",
            Self::Stylised => "stylised",
            Self::Text => "text",
            Self::Distortion => "distortion",
            Self::Glitch => "glitch",
            Self::Atmosphere => "atmosphere",
            Self::Subject => "subject",
            Self::Compositing => "compositing",
            Self::Backgrounds => "backgrounds",
            Self::Utilities => "utilities",
        }
    }
}

/// What a recipe hands an effect besides the picture and its parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Input {
    /// The pieces of the region a step names: a label for every pixel.
    Partition,
    /// Working pixels per design pixel: for an effect whose light or density is per working pixel.
    SizeRatio,
    /// Where the picture lies on the canvas a render works on (see [`Place`](crate::Place)): for an effect whose answer
    /// at a pixel depends on the pixel's place on the canvas, which the picture it is handed may be only a band of.
    Place,
}

impl Input {
    /// Every input, in the order recipes list them.
    pub const ALL: &'static [Self] = &[Self::Partition, Self::SizeRatio, Self::Place];

    /// The input's name in `snake_case`: `"size_ratio"`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Partition => "partition",
            Self::SizeRatio => "size_ratio",
            Self::Place => "place",
        }
    }
}

/// A parameter of an effect that names a field: the map of the picture the effect works from (`depth`, `light`, a
/// region), which a step hands it as a [`Layer`](crate::Layer) under the parameter's name.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FieldParam {
    /// The parameter's name, which is also the name the effect finds its map under.
    pub parameter: String,
    /// The field a step that does not set the parameter uses.
    pub default: String,
}

impl FieldParam {
    /// The field parameter called `parameter`, whose field is `default` unless a step names another.
    #[must_use]
    pub fn new(parameter: impl Into<String>, default: impl Into<String>) -> Self {
        Self {
            parameter: parameter.into(),
            default: default.into(),
        }
    }
}

impl FieldParam {
    /// The field this parameter names in `given`, a step's values: the string it holds, else the default.
    ///
    /// # Errors
    /// What is wrong, as text for the caller to place, if it holds something that is no name.
    pub(crate) fn field_in<'a>(&'a self, given: &'a Map<String, Value>) -> Result<&'a str, String> {
        match given.get(&self.parameter) {
            None => Ok(&self.default),
            Some(Value::String(name)) => Ok(name),
            Some(other) => Err(format!("`{}` names a field, got {other}", self.parameter)),
        }
    }
}

/// An effect the core can run by name.
pub trait Effect: Send + Sync {
    /// The name recipes and the registry know it by.
    fn name(&self) -> &str;

    /// Its section of the catalogue.
    fn category(&self) -> Category;

    /// The opacity (0..1) of a step of this effect that sets none: below 1 for an effect meant to be mixed in partway.
    fn opacity(&self) -> f64 {
        1.0
    }

    /// The inputs it declares, if any.
    fn inputs(&self) -> &[Input] {
        &[]
    }

    /// The parameters that name a field, if any: the effect receives a map under each.
    fn fields(&self) -> &[FieldParam] {
        &[]
    }

    /// Its parameters, described; none when the effect's owner describes and validates them itself. A render checks,
    /// completes and scales the values of a described effect (`px` and `density` fields, to the picture's size); an
    /// effect with none receives them as the step wrote them, in design pixels, and scales them itself by the
    /// [`Input::SizeRatio`] it declares.
    fn params(&self) -> Option<Table> {
        None
    }

    /// What it reads around a pixel when run with `params`: `Footprint::Whole` unless the effect says it is
    /// less. `params` are exactly the parameters [`apply`](Self::apply) is given, an object completed, checked and
    /// scaled to the picture (see `Table::scaled`), so a distance in them is already in working pixels, floors
    /// included; the footprint scales nothing itself. Whatever an effect says holds for the picture it is run on (see
    /// [`Footprint`]).
    ///
    /// # Errors
    /// [`Error::Params`] if `params` are not the effect's parameters, as for [`apply`](Self::apply).
    fn footprint(&self, _params: &Value) -> Result<Footprint, Error> {
        Ok(Footprint::Whole)
    }

    /// Run it on `picture` with `params` (an object with the effect's parameters): a new image of the picture's size.
    /// `inputs` holds the inputs the effect declares and nothing else, each of the picture's size: the core makes it
    /// so (see [`Inputs`]).
    ///
    /// # Errors
    /// [`Error::Params`] if `params` miss one of the effect's parameters, have one it does not know, or one of the
    /// wrong type; [`Error::Foreign`] if an effect run elsewhere fails.
    fn apply(&self, picture: &Picture<'_>, params: serde_json::Value, inputs: &Inputs<'_>) -> Result<Image, Error>;
}

/// Run `effect` on `picture` with `params`, handing it the part of `offered` that it declares.
///
/// # Errors
/// [`Error::Inputs`] if the effect declares an input that `offered` lacks or that is not of the picture's size,
/// [`Error::Foreign`] if the image it makes is not of the picture's size, else what [`Effect::apply`] returns.
pub(crate) fn run(
    effect: &dyn Effect,
    picture: &Picture<'_>,
    params: serde_json::Value,
    offered: &Inputs<'_>,
) -> Result<Image, Error> {
    let image = effect.apply(picture, params, &offered.declared_by(effect, picture)?)?;
    if (image.height(), image.width()) != (picture.height(), picture.width()) {
        return Err(Error::foreign(
            effect.name(),
            format!(
                "changed the picture's shape from {} x {} to {} x {}",
                picture.height(),
                picture.width(),
                image.height(),
                image.width()
            ),
        ));
    }
    Ok(image)
}

/// What a file in `src/effects/` declares about its effect besides its parameters: `pub(crate) const META: Meta`.
pub(crate) struct Meta {
    /// Its section of the catalogue.
    pub(crate) category: Category,
    /// The opacity (0..1) of a step of it that sets none.
    pub(crate) opacity: f64,
    /// The inputs it declares.
    pub(crate) inputs: &'static [Input],
}

/// An effect written in Rust, in a file of `src/effects/`: its `META`, its `Params` (read from the parameters), its
/// `footprint` and its `render`. The build script makes one for each file.
pub(crate) struct Native<P> {
    name: &'static str,
    meta: &'static Meta,
    /// The `FieldName` parameters of `P`, read from its declaration, so a field is declared once.
    fields: Vec<FieldParam>,
    footprint: fn(&P) -> Footprint,
    render: fn(&Picture<'_>, &P, &Inputs<'_>) -> Image,
}

impl<P: Declared> Native<P> {
    /// The effect called `name` (its file's name) with its `meta`, `footprint` and `render`.
    pub(crate) fn new(
        name: &'static str,
        meta: &'static Meta,
        footprint: fn(&P) -> Footprint,
        render: fn(&Picture<'_>, &P, &Inputs<'_>) -> Image,
    ) -> Self {
        let fields = P::table()
            .fields
            .iter()
            .filter(|field| field.kind == Kind::Field)
            .map(|field| FieldParam::new(field.name, field.default.to_string()))
            .collect();
        Self {
            name,
            meta,
            fields,
            footprint,
            render,
        }
    }
}

impl<P: DeserializeOwned> Native<P> {
    /// `params` as the effect's parameters.
    fn read(&self, params: serde_json::Value) -> Result<P, Error> {
        serde_json::from_value(params).map_err(|source| Error::Params {
            effect: self.name,
            source,
        })
    }
}

impl<P: DeserializeOwned + Declared> Effect for Native<P> {
    fn name(&self) -> &str {
        self.name
    }

    fn category(&self) -> Category {
        self.meta.category
    }

    fn opacity(&self) -> f64 {
        self.meta.opacity
    }

    fn inputs(&self) -> &[Input] {
        self.meta.inputs
    }

    fn fields(&self) -> &[FieldParam] {
        &self.fields
    }

    fn params(&self) -> Option<Table> {
        Some(P::table())
    }

    fn footprint(&self, params: &Value) -> Result<Footprint, Error> {
        Ok((self.footprint)(&self.read(params.clone())?))
    }

    fn apply(&self, picture: &Picture<'_>, params: serde_json::Value, inputs: &Inputs<'_>) -> Result<Image, Error> {
        Ok((self.render)(picture, &self.read(params)?, inputs))
    }
}

#[cfg(test)]
mod tests {
    //! A `Native` effect lists the field parameters its `Params` declares. No effect of the core has one yet, and a
    //! `Native` is the core's own, so this is the one place it can be made.

    use serde::Deserialize;

    use super::{Category, FieldParam, Meta, Native};
    use crate::Footprint;
    use crate::params::{FieldName, Params};
    use crate::{Effect, Image, Inputs, Picture};

    /// The parameters of a probe.
    #[derive(Debug, Clone, PartialEq, Deserialize, Params)]
    #[serde(deny_unknown_fields)]
    struct Probe {
        /// A size in px.
        #[param(default = 4.0, unit = px, min = 0, max = 32)]
        size: f32,
        /// The selection the blur follows.
        #[param(default = "depth")]
        follow: FieldName,
        /// The selection that limits it.
        #[param(default = "light")]
        within: FieldName,
    }

    const META: Meta = Meta {
        category: Category::Utilities,
        opacity: 1.0,
        inputs: &[],
    };

    fn footprint(_params: &Probe) -> Footprint {
        Footprint::Pixel
    }

    fn render(picture: &Picture<'_>, _params: &Probe, _inputs: &Inputs<'_>) -> Image {
        picture.map(|pixel| pixel)
    }

    #[test]
    fn the_fields_are_the_field_parameters_of_the_declaration_with_their_default_names() {
        let probe = Native::new("probe", &META, footprint, render);
        assert_eq!(
            probe.fields(),
            [FieldParam::new("follow", "depth"), FieldParam::new("within", "light")]
        );
    }
}
