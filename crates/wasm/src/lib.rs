//! The WebAssembly package of bullterrier-image, for the browser and Node: a recipe rendered on a photo, with the core's
//! own checks and errors. The core runs unchanged (one thread, no `onnx` feature), so a render is the native render of the
//! same size, byte for byte, except for the effects that filter through the Fourier transform (`bloom`, `engraving`,
//! `focus_blur`, `lens_blur`, `starburst`): rustfft rounds differently in each architecture's SIMD routine, so a channel
//! may differ by one level of 255 (measured on 5 of the 84 example recipes, 1 to 4 bytes of 360000); this file only converts between JavaScript's values and the core's, and gives each core
//! error its JavaScript `Error`. A subject cut-out is not found here: the page hands it in with each render (`maps`).

use bullterrier_image as image;
use js_sys::{Error, Float32Array, JSON, Object, Reflect, TypeError, Uint8Array, Uint8ClampedArray};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TYPES: &str = r#"
/**
 * What a render is asked for: `size` is the longest side in pixels, else the recipe's `size`, else the photo's own;
 * `maps` are the maps the recipe reads, by name (`subject`: 1 where the subject is, `depth`: 1 nearest, or a region
 * the recipe declares `given`): a value from 0 to 1 for each pixel of the photo, row after row.
 */
export interface RenderOptions {
    size?: number;
    maps?: Record<string, Float32Array>;
}

/** A render: `data` is the 8-bit RGBA bytes of `new ImageData(data, width, height)`, straight alpha. */
export interface Rendered {
    width: number;
    height: number;
    data: Uint8ClampedArray;
}

/**
 * A parameter of a recipe that can be set, by address (`recipe.parameters()`, `recipe.withValues`): a number (`kind` is
 * "number") or a colour ("color": `value` is a text like #rrggbb, or `[r, g, b]` from 0 to 1 when a channel is no whole 255th;
 * `unit` is "" and `min`, `max` are `null`). `value` is `null` for an optional parameter the recipe leaves unset
 * (`nullable` marks an optional colour; `withValues` sets a colour and never unsets it).
 */
export interface RecipeParameter {
    kind: "number" | "color";
    value: number | string | number[] | null;
    unit: string;
    min: number | null;
    max: number | null;
    meaning: string;
    whole: boolean;
    cyclic: boolean;
    nullable: boolean;
}

/** A parameter of an effect, as the core declares it (`kind` is "float", "int", "bool", "choice", "color", ...). */
export interface Parameter {
    name: string;
    kind: string;
    meaning: string;
    default: number | string | boolean | string[] | null;
    unit: string;
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

    /// A number of pixels.
    #[wasm_bindgen(typescript_type = "number")]
    pub type Pixels;

    /// The numbers and colours to set, by address (a colour as `"#rrggbb"`, `"#rgb"` or `[r, g, b]`).
    #[wasm_bindgen(typescript_type = "Record<string, number | string | number[]>")]
    pub type Values;

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

/// The `TypeError` for a value that is not what a call reads.
fn refuse(what: &str, expected: &str, value: &JsValue) -> JsValue {
    let found = value.js_typeof().as_string().unwrap_or_default();
    TypeError::new(&format!("{what}: expected {expected}, got a {found}")).into()
}

/// The number of pixels `value` writes (`what` is its name): a whole number from 1 up that a `u32` holds.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "checked to be a whole number within 1..=u32::MAX"
)]
fn whole_pixels(value: &JsValue, what: &str) -> Result<u32, JsValue> {
    value
        .as_f64()
        .filter(|size| size.fract() == 0.0 && (1.0..=f64::from(u32::MAX)).contains(size))
        .map(|size| size as u32)
        .ok_or_else(|| TypeError::new(&format!("{what}: expected a whole number of pixels, got {value:?}")).into())
}

/// A photo to render recipes on: 8-bit pixels (the alpha is not read), as the core's levels.
#[wasm_bindgen(js_name = Photo)]
pub struct Photo {
    height: usize,
    width: usize,
    levels: Vec<f32>,
}

#[wasm_bindgen(js_class = Photo)]
impl Photo {
    /// The photo of `width` x `height` pixels that `rgba` holds, four bytes a pixel, row after row (an `ImageData`'s
    /// `data` is such an array).
    ///
    /// # Errors
    /// A `TypeError` naming `width`, `height` or `rgba` unless they are whole numbers of at least 1 and a `Uint8Array` or
    /// `Uint8ClampedArray`; `InvalidImageError` unless `rgba` has `width * height * 4` bytes.
    #[wasm_bindgen(constructor)]
    pub fn new(rgba: &Rgba, width: &Pixels, height: &Pixels) -> Result<Photo, JsValue> {
        let (width, height) = (whole_pixels(width, "width")?, whole_pixels(height, "height")?);
        let given: &JsValue = rgba.as_ref();
        let bytes = if let Some(array) = given.dyn_ref::<Uint8Array>() {
            array.to_vec()
        } else if let Some(array) = given.dyn_ref::<Uint8ClampedArray>() {
            array.to_vec()
        } else {
            return Err(refuse("rgba", "a Uint8Array or Uint8ClampedArray", given));
        };
        let (width, height) = (width as usize, height as usize);
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
        Ok(Photo { height, width, levels })
    }
}

/// A recipe: the look to render, as the core reads and checks it.
#[wasm_bindgen(js_name = Recipe)]
pub struct Recipe(image::Recipe);

/// A recipe the core has read and checked, as `load_recipe` does in Python.
fn loaded(recipe: Result<image::Recipe, image::Error>) -> Result<Recipe, JsValue> {
    let presets = raised(image::Presets::shipped())?;
    raised(recipe.and_then(|recipe| recipe.checked(&presets))).map(Recipe)
}

#[wasm_bindgen(js_class = Recipe)]
impl Recipe {
    /// The recipe a recipe file writes (TOML text).
    ///
    /// # Errors
    /// `RecipeError` for text that is not a recipe or breaks a rule of one (an unknown parameter, a value out of its
    /// range), with the step and the rule; `UnknownEffectError` for an effect that does not exist.
    #[wasm_bindgen(js_name = fromToml)]
    pub fn from_toml(text: &str) -> Result<Recipe, JsValue> {
        loaded(image::Recipe::from_toml(text))
    }

    /// The recipe the JSON document `text` writes, as `toJson` writes it.
    ///
    /// # Errors
    /// As `fromToml`.
    #[wasm_bindgen(js_name = fromJson)]
    pub fn from_json(text: &str) -> Result<Recipe, JsValue> {
        loaded(image::Recipe::from_json(text))
    }

    /// The look that ships with the package, by name (`looks()` lists them).
    ///
    /// # Errors
    /// `RecipeError` naming the closest looks for any other name.
    pub fn look(name: &str) -> Result<Recipe, JsValue> {
        raised(image::Recipe::look(name)).map(Recipe)
    }

    /// The recipe as a JSON document: `fromJson` reads it back.
    ///
    /// # Errors
    /// `RecipeError` if the recipe cannot be written (it can always be).
    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&self.0).map_err(|error| thrown_for(&image::Error::Recipe(error.to_string())))
    }

    /// Every number and colour of the recipe that can be set, by address, in the order of the recipe (`halftone.cell`,
    /// `point_light.color`; as Python's `Recipe.parameters`): an object of the address to `{ kind, value, unit, min, max,
    /// meaning, whole, cyclic, nullable }`, what a page needs for a slider or a colour picker each. `value` is `null` for
    /// an optional parameter the recipe leaves unset.
    ///
    /// # Errors
    /// `UnknownEffectError` for a step of an effect that does not exist (a loaded recipe has none).
    #[wasm_bindgen(unchecked_return_type = "Record<string, RecipeParameter>")]
    pub fn parameters(&self) -> Result<JsValue, JsValue> {
        let all = Object::new();
        for parameter in raised(self.0.parameters())? {
            let found = Object::new();
            let value = match parameter.value {
                Some(setting) => JSON::parse(&setting.to_json().to_string())?,
                None => JsValue::NULL,
            };
            let kind = match parameter.kind {
                image::ParameterKind::Color => "color",
                _ => "number",
            };
            for (key, member) in [
                ("kind", kind.into()),
                ("value", value),
                ("unit", parameter.unit.into()),
                ("min", parameter.min.map_or(JsValue::NULL, JsValue::from_f64)),
                ("max", parameter.max.map_or(JsValue::NULL, JsValue::from_f64)),
                ("meaning", parameter.doc.into()),
                ("whole", parameter.whole.into()),
                ("cyclic", parameter.cyclic.into()),
                ("nullable", parameter.nullable.into()),
            ] {
                Reflect::set(&found, &key.into(), &member)?;
            }
            Reflect::set(&all, &parameter.address.into(), &found)?;
        }
        Ok(all.into())
    }

    /// A copy of the recipe with each parameter at an address of `values` set (as Python's `Recipe.with_values`): a number,
    /// or a colour as `"#rrggbb"`, `"#rgb"` or `[r, g, b]`. The recipe itself is left as it is.
    ///
    /// # Errors
    /// A `TypeError` naming `values` unless it is an object; `RecipeError` naming the address for an address that points
    /// at nothing, a value that is neither a number nor a colour, or a value the recipe refuses (never clamped).
    #[wasm_bindgen(js_name = withValues)]
    pub fn with_values(&self, values: &Values) -> Result<Recipe, JsValue> {
        let given: &JsValue = values.as_ref();
        let object = given
            .dyn_ref::<Object>()
            .ok_or_else(|| refuse("values", "an object of addresses to numbers or colours", given))?;
        let mut set = Vec::new();
        for address in Object::keys(object) {
            let address = address.as_string().unwrap_or_default();
            let value = Reflect::get(object, &JsValue::from_str(&address))?;
            let json = if let Some(number) = value.as_f64() {
                serde_json::Value::from(number)
            } else if value.is_string() || js_sys::Array::is_array(&value) {
                JSON::stringify(&value)
                    .ok()
                    .and_then(|text| text.as_string())
                    .and_then(|text| serde_json::from_str(&text).ok())
                    .unwrap_or(serde_json::Value::Null)
            } else {
                return Err(refuse(&address, "a number or a colour", &value));
            };
            set.push((address.clone(), raised(image::Setting::read(&address, &json))?));
        }
        let asked: Vec<(&str, image::Setting)> = set
            .iter()
            .map(|(address, setting)| (address.as_str(), *setting))
            .collect();
        let presets = raised(image::Presets::shipped())?;
        raised(self.0.with_values(&asked, &presets)).map(Recipe)
    }

    /// Run the recipe on `photo`: the photo framed and scaled to `options.size` (else the recipe's `size`, else its own),
    /// the steps in order, with the maps of `options.maps` (`subject`, `depth`, or a region the recipe declares `given`: a
    /// `Float32Array` of 0..1, a value per pixel of the photo), as the native render of the same size does.
    ///
    /// # Errors
    /// A `TypeError` naming `options`, `size` or a map for a value of the wrong type; what the core refuses the recipe for
    /// (`RecipeError`, `UnknownEffectError`); `MapNotFoundError` for a map of the wrong length or with a value outside
    /// 0..1; `SubjectNotFoundError` or another `MapNotFoundError` for a map the recipe needs and `maps` lacks.
    #[wasm_bindgen(unchecked_return_type = "Rendered")]
    #[expect(
        clippy::cast_precision_loss,
        reason = "a side is at most 2^26 pixels, far inside f64's 53 bits"
    )]
    pub fn render(&self, photo: &Photo, options: Option<RenderOptions>) -> Result<JsValue, JsValue> {
        let mut chosen = image::Options::default();
        let mut maps = Vec::new();
        if let Some(options) = options {
            let given: &JsValue = options.as_ref();
            if !given.is_object() {
                return Err(refuse("options", "an object", given));
            }
            let size = Reflect::get(given, &JsValue::from_str("size"))?;
            if !size.is_undefined() {
                chosen = chosen.with_size(whole_pixels(&size, "size")?);
            }
            let given_maps = Reflect::get(given, &JsValue::from_str("maps"))?;
            if !given_maps.is_undefined() {
                let object = given_maps
                    .dyn_ref::<Object>()
                    .ok_or_else(|| refuse("options.maps", "an object of names to Float32Arrays", &given_maps))?;
                for name in Object::keys(object) {
                    let name = name.as_string().unwrap_or_default();
                    let values = Reflect::get(object, &JsValue::from_str(&name))?;
                    let values = values
                        .dyn_ref::<Float32Array>()
                        .ok_or_else(|| refuse(&format!("maps.{name}"), "a Float32Array", &values))?;
                    maps.push((name, values.to_vec()));
                }
            }
        }
        let picture = raised(image::Picture::new(photo.height, photo.width, &photo.levels))?;
        let mut core = image::Photo::new(picture);
        for (name, values) in maps {
            let layer = image::Layer::new(photo.height, photo.width, values).map_err(|why| {
                thrown_for(&image::Error::map_not_found(
                    &name,
                    format!(
                        "the map '{name}' is no map of the {} x {} photo: {why}",
                        photo.width, photo.height
                    ),
                ))
            })?;
            core = raised(core.with_map(name, layer))?;
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
        // a plain object holding a copy in JavaScript's memory: nothing to free, and the array is the same on every read
        let rendered = Object::new();
        Reflect::set(&rendered, &"width".into(), &JsValue::from_f64(width as f64))?;
        Reflect::set(&rendered, &"height".into(), &JsValue::from_f64(height as f64))?;
        Reflect::set(&rendered, &"data".into(), &Uint8ClampedArray::from(bytes.as_slice()))?;
        Ok(rendered.into())
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
    let fields = JSON::parse(&text)?;
    // no unit is "", as the other bindings write it, not `null`
    for field in js_sys::Array::from(&fields) {
        if Reflect::get(&field, &"unit".into())?.is_null() {
            Reflect::set(&field, &"unit".into(), &"".into())?;
        }
    }
    Ok(fields)
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
