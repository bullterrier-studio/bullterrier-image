//! The WebAssembly package of bullterrier-image, for the browser and Node: a recipe rendered on a photo, with the core's
//! own checks and errors. The core runs unchanged (one thread, no `onnx` feature), so a render is the native render of the
//! same size, byte for byte; this file only converts between JavaScript's values and the core's, and gives each core
//! error its JavaScript `Error`. A subject cut-out is not found here: the page hands it in with `Photo.setMap`.

use bullterrier_image as image;
use js_sys::{Error, JSON, Uint8Array, Uint8ClampedArray};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TYPES: &str = r#"
/** What a render is asked for: `size` is the longest side in pixels, else the recipe's `size`, else the photo's own. */
export interface RenderOptions {
    size?: number;
}

/** A parameter of an effect, as the core declares it (`kind` is "float", "int", "bool", "choice", "color", ...). */
export interface Parameter {
    name: string;
    kind: string;
    meaning: string;
    default: number | string | boolean | string[] | null;
    unit: string | null;
    min: number | null;
    max: number | null;
    choices: string[];
    drivable: boolean;
    advanced: boolean;
}
"#;

#[wasm_bindgen]
extern "C" {
    /// The 8-bit RGBA pixels of a photo: a `Uint8Array`, or the `data` of an `ImageData` (a `Uint8ClampedArray`).
    #[wasm_bindgen(typescript_type = "Uint8Array | Uint8ClampedArray")]
    pub type Rgba;

    /// What a render is asked for.
    #[wasm_bindgen(typescript_type = "RenderOptions")]
    pub type RenderOptions;

    /// The parameters of an effect.
    #[wasm_bindgen(typescript_type = "Parameter[]")]
    pub type Parameters;
}

/// The `Error` a core error becomes: its `name` is the kind as the Python package names its exception (`RecipeError`,
/// `UnknownEffectError`, `SubjectNotFoundError`), its message the core's.
fn thrown_for(error: &image::Error) -> JsValue {
    let kind = error.kind().name();
    let name = if kind == "other" {
        "BullterrierImageError".to_owned()
    } else {
        let mut name: String = kind
            .split('_')
            .map(|word| word[..1].to_uppercase() + &word[1..])
            .collect();
        name.push_str("Error");
        name
    };
    let message = match error {
        // the core's own sentence names no alternative; the Python package adds them, and so does this
        image::Error::UnknownEffect(name) => image::Presets::shipped().map_or_else(
            |_| error.to_string(),
            |presets| image::documentation::unknown_effect(name, &presets),
        ),
        _ => error.to_string(),
    };
    let thrown = Error::new(&message);
    thrown.set_name(&name);
    thrown.into()
}

/// Run a core call and throw its error as the matching `Error`.
fn raised<T>(result: Result<T, image::Error>) -> Result<T, JsValue> {
    result.map_err(|error| thrown_for(&error))
}

/// The 8-bit level of `level` (0..1), rounded as the files the core writes are (`files::write`): `level * 255 + 0.5`, cut to
/// a whole number.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a level is 0..1, so the sum is 0.5..255.5 and cutting it is the rounding"
)]
fn eight_bit(level: f32) -> u8 {
    (level * 255.0 + 0.5) as u8
}

/// The number of pixels `value` writes: a whole number from 1 up that a `u32` holds.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "checked to be a whole number within 1..=u32::MAX"
)]
fn whole_pixels(value: &JsValue) -> Result<u32, JsValue> {
    value
        .as_f64()
        .filter(|size| size.fract() == 0.0 && (1.0..=f64::from(u32::MAX)).contains(size))
        .map(|size| size as u32)
        .ok_or_else(|| js_sys::TypeError::new("size: expected a whole number of pixels").into())
}

/// A photo to render recipes on: 8-bit pixels (the alpha is not read) and the maps handed in beside them.
#[wasm_bindgen(js_name = Photo)]
pub struct Photo {
    height: usize,
    width: usize,
    levels: Vec<f32>,
    maps: Vec<(String, image::Layer)>,
}

#[wasm_bindgen(js_class = Photo)]
impl Photo {
    /// The photo of `width` x `height` pixels that `rgba` holds, four bytes a pixel, row after row (an `ImageData`'s
    /// `data` is such an array). Throws `InvalidImageError` for any other length.
    ///
    /// # Errors
    /// `InvalidImageError` unless `width` and `height` are at least 1 and `rgba` has `width * height * 4` bytes.
    #[wasm_bindgen(constructor)]
    pub fn new(rgba: &Rgba, width: usize, height: usize) -> Result<Photo, JsValue> {
        let bytes = Uint8Array::new(rgba.as_ref()).to_vec();
        if width.checked_mul(height).and_then(|pixels| pixels.checked_mul(4)) != Some(bytes.len()) {
            let message = format!(
                "{} bytes do not make {width} x {height} pixels of 4 channels",
                bytes.len()
            );
            return Err(thrown_for(&image::Error::InvalidImage(message)));
        }
        // the level of an 8-bit file: the byte over 255, as the core reads one
        let levels = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|pixel| &pixel[..3])
            .map(|&byte| f32::from(byte) / 255.0)
            .collect();
        let photo = Photo {
            height,
            width,
            levels,
            maps: Vec::new(),
        };
        raised(image::Picture::new(height, width, &photo.levels))?; // a size of zero
        Ok(photo)
    }

    /// Hand in the map `name` (`"subject"`: 1 where the subject is, `"depth"`: 1 nearest, or a region the recipe declares
    /// as given): one `Float32Array` value from 0 to 1 for each pixel of the photo, row after row. A map of the same name
    /// replaces the one before.
    ///
    /// # Errors
    /// `MapNotFoundError` unless there is a value for each pixel and each is within 0..1.
    #[wasm_bindgen(js_name = setMap)]
    pub fn set_map(&mut self, name: &str, values: &[f32]) -> Result<(), JsValue> {
        let layer = image::Layer::new(self.height, self.width, values.to_vec()).map_err(|why| {
            thrown_for(&image::Error::map_not_found(
                name,
                format!(
                    "the map '{name}' is no map of the {} x {} photo: {why}",
                    self.width, self.height
                ),
            ))
        })?;
        self.maps.retain(|(known, _)| known != name);
        self.maps.push((name.to_owned(), layer));
        Ok(())
    }
}

/// A recipe: the look to render, as the core reads and checks it.
#[wasm_bindgen(js_name = Recipe)]
pub struct Recipe(image::Recipe);

#[wasm_bindgen(js_class = Recipe)]
impl Recipe {
    /// The recipe a recipe file writes (TOML text).
    ///
    /// # Errors
    /// `RecipeError` for text that is not a recipe, with the line and the rule.
    #[wasm_bindgen(js_name = fromToml)]
    pub fn from_toml(text: &str) -> Result<Recipe, JsValue> {
        raised(image::Recipe::from_toml(text)).map(Recipe)
    }

    /// The recipe the JSON document `text` writes, as `toJson` writes it.
    ///
    /// # Errors
    /// `RecipeError` for text that is not a recipe.
    #[wasm_bindgen(js_name = fromJson)]
    pub fn from_json(text: &str) -> Result<Recipe, JsValue> {
        raised(image::Recipe::from_json(text)).map(Recipe)
    }

    /// The look that ships with the package, by name (`looks()` lists them).
    ///
    /// # Errors
    /// `RecipeError` naming the closest looks for any other name.
    pub fn look(name: &str) -> Result<Recipe, JsValue> {
        raised(image::Recipe::look(name)).map(Recipe)
    }

    /// The recipe as a JSON document: edit a value and read it back with `fromJson`.
    ///
    /// # Errors
    /// `BullterrierImageError` if the recipe cannot be written (it can always be).
    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&self.0).map_err(|error| thrown_for(&image::Error::Recipe(error.to_string())))
    }

    /// Run the recipe on `photo`: the photo framed and scaled to `options.size` (else the recipe's `size`, else its own),
    /// the steps in order, as the native render of the same size does.
    ///
    /// # Errors
    /// What the core refuses the recipe for (`RecipeError`, `UnknownEffectError`), `SubjectNotFoundError` or another
    /// `MapNotFoundError` for a map the recipe needs and the photo was not given.
    pub fn render(&self, photo: &Photo, options: Option<RenderOptions>) -> Result<Rendered, JsValue> {
        let mut chosen = image::Options::default();
        if let Some(options) = options {
            let size = js_sys::Reflect::get(options.as_ref(), &JsValue::from_str("size"))?;
            if !size.is_undefined() {
                chosen = chosen.with_size(whole_pixels(&size)?);
            }
        }
        let picture = raised(image::Picture::new(photo.height, photo.width, &photo.levels))?;
        let mut core = image::Photo::new(picture);
        for (name, layer) in &photo.maps {
            core = raised(core.with_map(name.clone(), layer.clone()))?;
        }
        let presets = raised(image::Presets::shipped())?;
        let done = raised(core.render(&self.0, &presets, chosen))?;
        let (height, width, channels) = (done.height(), done.width(), done.channels());
        let values = done.into_values();
        let mut bytes = Vec::with_capacity(height * width * 4);
        for pixel in values.chunks_exact(channels) {
            bytes.extend(pixel.iter().map(|&value| eight_bit(value)));
            if channels == 3 {
                bytes.push(255);
            }
        }
        Ok(Rendered { width, height, bytes })
    }
}

/// A render's pixels: `width`, `height` and `data`, the 8-bit RGBA bytes of `new ImageData(data, width, height)`.
#[wasm_bindgen(js_name = Rendered)]
pub struct Rendered {
    width: usize,
    height: usize,
    bytes: Vec<u8>,
}

#[wasm_bindgen(js_class = Rendered)]
impl Rendered {
    /// The number of columns.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn width(&self) -> usize {
        self.width
    }

    /// The number of rows.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn height(&self) -> usize {
        self.height
    }

    /// The pixels, four bytes each, straight alpha (255 unless the recipe cuts the picture out): a new array on every
    /// read, so read it once.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn data(&self) -> Uint8ClampedArray {
        Uint8ClampedArray::from(self.bytes.as_slice())
    }
}

/// The names of the effects, sorted.
#[wasm_bindgen]
#[must_use]
pub fn effects() -> Vec<String> {
    image::effects()
}

/// The parameters of the effect called `effect`, in declaration order: what a page needs for a slider each (`min`, `max`,
/// `default`, `unit`, `meaning`).
///
/// # Errors
/// `UnknownEffectError` for any other name, with the closest ones.
#[wasm_bindgen(unchecked_return_type = "Parameter[]")]
pub fn parameters(effect: &str) -> Result<JsValue, JsValue> {
    let table = raised(image::effect(effect))?.params();
    let text =
        serde_json::to_string(&table.fields).map_err(|error| thrown_for(&image::Error::Recipe(error.to_string())))?;
    JSON::parse(&text)
}

/// The names of the looks that ship with the package, for `Recipe.look`.
#[wasm_bindgen]
#[must_use]
pub fn looks() -> Vec<String> {
    image::shipped::looks()
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect()
}
