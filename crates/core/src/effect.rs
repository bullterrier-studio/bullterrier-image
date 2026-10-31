//! What an effect is to the core: a name, where it sits in the catalogue, how much of it a step shows by default, the
//! inputs it declares and a function from a picture and parameters to a new picture.
//!
//! The core's own effects are files in `src/effects/`; an effect that lives elsewhere (a Python function, say) is a
//! value of a type that implements [`Effect`] and joins them in the registry (see [`crate::register`]).

use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::params::{Declared, Table};
use crate::{Error, Image, Inputs, Picture};

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
}

impl Input {
    /// Every input, in the order recipes list them.
    pub const ALL: &'static [Self] = &[Self::Partition, Self::SizeRatio];

    /// The input's name in `snake_case`: `"size_ratio"`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Partition => "partition",
            Self::SizeRatio => "size_ratio",
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

/// An effect written in Rust, in a file of `src/effects/`: its `META`, its `Params` (read from the parameters) and its
/// `render`. The build script makes one for each file.
pub(crate) struct Native<P> {
    name: &'static str,
    meta: &'static Meta,
    render: fn(&Picture<'_>, &P, &Inputs<'_>) -> Image,
}

impl<P> Native<P> {
    /// The effect called `name` (its file's name) with its `meta` and `render`.
    pub(crate) const fn new(
        name: &'static str,
        meta: &'static Meta,
        render: fn(&Picture<'_>, &P, &Inputs<'_>) -> Image,
    ) -> Self {
        Self { name, meta, render }
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

    fn params(&self) -> Option<Table> {
        Some(P::table())
    }

    fn apply(&self, picture: &Picture<'_>, params: serde_json::Value, inputs: &Inputs<'_>) -> Result<Image, Error> {
        let params = serde_json::from_value(params).map_err(|source| Error::Params {
            effect: self.name,
            source,
        })?;
        Ok((self.render)(picture, &params, inputs))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::*;
    use crate::testing::{Counting, Recorder};
    use crate::{Labels, Layer};

    /// The variant after `category` in catalogue order. The match has no wildcard: a new variant stops this compiling
    /// until it is placed here, and then `ALL` must list it too for the walk below to match.
    const fn next_category(category: Category) -> Option<Category> {
        match category {
            Category::Colour => Some(Category::Light),
            Category::Light => Some(Category::Texture),
            Category::Texture => Some(Category::Stylised),
            Category::Stylised => Some(Category::Text),
            Category::Text => Some(Category::Distortion),
            Category::Distortion => Some(Category::Glitch),
            Category::Glitch => Some(Category::Atmosphere),
            Category::Atmosphere => Some(Category::Subject),
            Category::Subject => Some(Category::Compositing),
            Category::Compositing => Some(Category::Backgrounds),
            Category::Backgrounds => Some(Category::Utilities),
            Category::Utilities => None,
        }
    }

    /// As [`next_category`], for inputs.
    const fn next_input(input: Input) -> Option<Input> {
        match input {
            Input::Partition => Some(Input::SizeRatio),
            Input::SizeRatio => None,
        }
    }

    /// A 2 x 3 picture and the partition and map to offer with it.
    fn picture_values() -> Vec<f32> {
        vec![0.5; 2 * 3 * 3]
    }

    fn labels() -> Result<Labels, Error> {
        Labels::new(2, 3, vec![0, 0, 1, 1, 2, 2], 3)
    }

    fn layer(level: f32) -> Result<Layer, Error> {
        Layer::new(2, 3, vec![level; 6])
    }

    const PARTITION: &[Input] = &[Input::Partition];
    const RATIO: &[Input] = &[Input::SizeRatio];

    #[test]
    fn a_declared_partition_arrives_and_an_undeclared_one_is_absent() -> Result<(), Error> {
        let (values, labels) = (picture_values(), labels()?);
        let picture = Picture::new(2, 3, &values)?;
        let offered = Inputs::default().with_partition(&labels);
        let takes = Recorder::new("takes", PARTITION, &[]);
        let ignores = Recorder::new("ignores", &[], &[]);
        run(&takes, &picture, json!({}), &offered)?;
        run(&ignores, &picture, json!({}), &offered)?;
        assert_eq!(takes.received()[0].partition, Some(vec![0, 0, 1, 1, 2, 2]));
        assert_eq!(ignores.received()[0].partition, None);
        Ok(())
    }

    #[test]
    fn a_declared_field_arrives_under_its_parameter_name_and_an_undeclared_one_is_absent() -> Result<(), Error> {
        let (values, depth, light) = (picture_values(), layer(0.25)?, layer(0.75)?);
        let picture = Picture::new(2, 3, &values)?;
        let offered = Inputs::default()
            .with_field("depth", &depth)
            .with_field("light", &light);
        let takes_depth = Recorder::new("takes_depth", &[], &["depth"]);
        let takes_none = Recorder::new("takes_none", &[], &[]);
        run(&takes_depth, &picture, json!({}), &offered)?;
        run(&takes_none, &picture, json!({}), &offered)?;
        // exactly the declared one, under the parameter's name, with that map's values
        assert_eq!(
            takes_depth.received()[0].fields,
            BTreeMap::from([("depth".to_owned(), vec![0.25; 6])])
        );
        assert!(takes_none.received()[0].fields.is_empty());
        Ok(())
    }

    #[test]
    fn the_size_ratio_arrives_only_if_declared_else_the_effect_sees_one() -> Result<(), Error> {
        let values = picture_values();
        let picture = Picture::new(2, 3, &values)?;
        let offered = Inputs::default().with_size_ratio(2.5);
        let takes = Recorder::new("takes", RATIO, &[]);
        let ignores = Recorder::new("ignores", &[], &[]);
        run(&takes, &picture, json!({}), &offered)?;
        run(&ignores, &picture, json!({}), &offered)?;
        assert!((takes.received()[0].size_ratio - 2.5).abs() < f64::EPSILON);
        assert!((ignores.received()[0].size_ratio - 1.0).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    fn a_declared_input_that_is_not_on_offer_refuses_the_run_and_the_effect_never_runs() -> Result<(), Error> {
        let values = picture_values();
        let picture = Picture::new(2, 3, &values)?;
        let counting = Counting::new();
        let result = run(&counting, &picture, json!({}), &Inputs::default());
        assert!(
            matches!(&result, Err(error @ Error::Inputs(_)) if error.to_string().contains("partition")),
            "{result:?}"
        );
        let field = Recorder::new("takes_depth", &[], &["depth"]);
        let result = run(&field, &picture, json!({}), &Inputs::default());
        assert!(
            matches!(&result, Err(error @ Error::Inputs(_)) if error.to_string().contains("depth")),
            "{result:?}"
        );
        assert_eq!((counting.calls(), field.received().len()), (0, 0));
        Ok(())
    }

    #[test]
    fn a_partition_or_map_of_another_size_than_the_picture_refuses_the_run() -> Result<(), Error> {
        let values = picture_values();
        let picture = Picture::new(2, 3, &values)?;
        let (small_labels, small_layer) = (Labels::new(1, 3, vec![0; 3], 1)?, Layer::new(2, 2, vec![0.5; 4])?);
        let partition = Recorder::new("takes", PARTITION, &[]);
        let field = Recorder::new("takes_depth", &[], &["depth"]);
        let result = run(
            &partition,
            &picture,
            json!({}),
            &Inputs::default().with_partition(&small_labels),
        );
        assert!(
            matches!(&result, Err(error @ Error::Inputs(_)) if error.to_string().contains("1 x 3")),
            "{result:?}"
        );
        let result = run(
            &field,
            &picture,
            json!({}),
            &Inputs::default().with_field("depth", &small_layer),
        );
        assert!(
            matches!(&result, Err(error @ Error::Inputs(_)) if error.to_string().contains("2 x 2")),
            "{result:?}"
        );
        assert_eq!((partition.received().len(), field.received().len()), (0, 0));
        Ok(())
    }

    #[test]
    fn all_categories_lists_every_variant_once_in_catalogue_order() {
        let all: &'static [Category] = Category::ALL;
        let walk: Vec<Category> =
            std::iter::successors(Some(Category::Colour), |&variant| next_category(variant)).collect();
        assert_eq!(all, walk.as_slice());
    }

    #[test]
    fn all_inputs_lists_every_variant_once_in_recipe_order() {
        let all: &'static [Input] = Input::ALL;
        let walk: Vec<Input> = std::iter::successors(Some(Input::Partition), |&variant| next_input(variant)).collect();
        assert_eq!(all, walk.as_slice());
    }
}
