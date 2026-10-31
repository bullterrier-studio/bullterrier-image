//! What an effect is to the core: a name, where it sits in the catalogue, how much of it a step shows by default, the
//! inputs it declares and a function from a picture and parameters to a new picture.
//!
//! The core's own effects are files in `src/effects/`; an effect that lives elsewhere (a Python function, say) is a
//! value of a type that implements [`Effect`] and joins them in the registry (see [`crate::register`]).

use serde::de::DeserializeOwned;

use crate::params::{Declared, Table};
use crate::{Error, Image, Picture};

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

    /// Its parameters, described; none when the effect's owner describes and validates them itself.
    fn params(&self) -> Option<Table> {
        None
    }

    /// Run it on `picture` with `params` (an object with the effect's parameters): a new image of the picture's size.
    ///
    /// # Errors
    /// [`Error::Params`] if `params` miss one of the effect's parameters, have one it does not know, or one of the
    /// wrong type; [`Error::Foreign`] if an effect run elsewhere fails.
    fn apply(&self, picture: &Picture<'_>, params: serde_json::Value) -> Result<Image, Error>;
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
    render: fn(&Picture<'_>, &P) -> Image,
}

impl<P> Native<P> {
    /// The effect called `name` (its file's name) with its `meta` and `render`.
    pub(crate) const fn new(name: &'static str, meta: &'static Meta, render: fn(&Picture<'_>, &P) -> Image) -> Self {
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

    fn apply(&self, picture: &Picture<'_>, params: serde_json::Value) -> Result<Image, Error> {
        let params = serde_json::from_value(params).map_err(|source| Error::Params {
            effect: self.name,
            source,
        })?;
        Ok((self.render)(picture, &params))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
