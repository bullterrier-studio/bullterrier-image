//! `#[derive(Params)]`: an effect's parameters, declared by field type, doc comment and `#[param(..)]`.

use proc_macro2::{Literal, TokenStream};
use quote::{ToTokens, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{
    Attribute, Data, DeriveInput, Error, Expr, ExprLit, Field, Fields, GenericArgument, Ident, Lit, LitStr, Meta,
    PathArguments, Token, Type, UnOp, bracketed, token,
};

use crate::snake_case;

mod colors;

/// The turn a cyclic field spans, in degrees.
const TURN: f64 = 360.0;

/// The code `#[derive(Params)]` writes for `input`: `Default` and `Declared` (the table and the values). A compile error naming the
/// field if its declaration is broken, or the struct if it is not a struct of named fields with a doc comment.
pub(crate) fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let Data::Struct(data) = &input.data else {
        return Err(Error::new_spanned(
            name,
            format!("`{name}` must be a struct: #[derive(Params)] reads its fields"),
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(Error::new_spanned(
            name,
            format!("`{name}` must be a struct with named fields"),
        ));
    };
    let doc = doc_of(&input.attrs).ok_or_else(|| {
        Error::new_spanned(
            name,
            format!("`{name}` needs a doc comment: it documents the parameters"),
        )
    })?;
    let declared = fields.named.iter().map(declare).collect::<syn::Result<Vec<_>>>()?;
    let names = declared.iter().map(|declared| &declared.name);
    let values = declared.iter().map(|declared| &declared.value);
    let rows = declared.iter().map(|declared| &declared.row);
    let labels = declared.iter().map(|declared| declared.name.to_string());
    let writes = declared.iter().map(|declared| &declared.write);
    Ok(quote! {
        impl ::core::default::Default for #name {
            fn default() -> Self {
                Self { #(#names: #values),* }
            }
        }

        impl crate::params::Declared for #name {
            fn table() -> crate::params::Table {
                crate::params::Table { doc: #doc, fields: ::std::vec![#(#rows),*] }
            }

            fn values(&self) -> ::serde_json::Map<::std::string::String, ::serde_json::Value> {
                let mut values = ::serde_json::Map::new();
                #(values.insert(::std::string::String::from(#labels), #writes);)*
                values
            }
        }
    })
}

/// One field, declared: its name, the expression of its default value, its row of the table and the expression of its
/// value as JSON.
struct Declared {
    name: Ident,
    value: TokenStream,
    row: TokenStream,
    write: TokenStream,
}

/// What a field's `#[param(..)]` attribute says.
#[derive(Default)]
struct Settings {
    default: Option<Given>,
    unit: Option<Ident>,
    min: Option<Expr>,
    max: Option<Expr>,
    min_length: Option<Expr>,
    max_length: Option<Expr>,
    drivable: bool,
    advanced: bool,
    cyclic: bool,
}

/// A number as the attribute writes it: a whole number stays whole.
#[derive(Clone, Copy)]
enum Number {
    Int(i64),
    Float(f64),
}

impl Number {
    #[expect(
        clippy::cast_precision_loss,
        reason = "compared as the decimal the attribute shows, in a f64"
    )]
    fn as_f64(self) -> f64 {
        match self {
            Self::Int(whole) => whole as f64,
            Self::Float(real) => real,
        }
    }

    /// The table's value: the literal, of its own kind.
    fn scalar(self) -> TokenStream {
        match self {
            Self::Int(whole) => {
                let whole = Literal::i64_suffixed(whole);
                quote!(crate::params::Scalar::Int(#whole))
            }
            Self::Float(real) => {
                let real = Literal::f64_suffixed(real);
                quote!(crate::params::Scalar::Float(#real))
            }
        }
    }
}

/// A default as the attribute writes it.
enum Written {
    Number(Number),
    Switch(bool),
    /// A colour: `"#rrggbb"`.
    Text(String),
    /// A list of colours: `["#rrggbb", ..]`.
    List(Vec<String>),
    /// A variant of a choice.
    Name(Ident),
}

/// How the derive reads a field's type: it recognises `f32`, `i32`, `bool`, `Color`, `Option<Color>`, `Vec<Color>`,
/// `FieldName` and `LutName` by their written names (a type alias is not recognised) and takes any other type for a
/// choice enum, whose names come from its `Choice` impl.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Float,
    Int,
    Switch,
    Colour,
    OptionalColour,
    Colours,
    Field,
    Lut,
    Choice,
}

fn shape(ty: &Type) -> Shape {
    let Type::Path(path) = ty else { return Shape::Choice };
    let Some(last) = path.path.segments.last() else {
        return Shape::Choice;
    };
    match last.ident.to_string().as_str() {
        "f32" => Shape::Float,
        "i32" => Shape::Int,
        "bool" => Shape::Switch,
        "Color" => Shape::Colour,
        "LutName" => Shape::Lut,
        "FieldName" => Shape::Field,
        "Option" if colour_argument(&last.arguments) => Shape::OptionalColour,
        "Vec" if colour_argument(&last.arguments) => Shape::Colours,
        _ => Shape::Choice,
    }
}

/// Whether `arguments` are `<Color>`: the one type a list or an option may hold.
fn colour_argument(arguments: &PathArguments) -> bool {
    let PathArguments::AngleBracketed(written) = arguments else {
        return false;
    };
    matches!(
        written.args.iter().collect::<Vec<_>>().as_slice(),
        [GenericArgument::Type(element)] if shape(element) == Shape::Colour
    )
}

/// What a checked field contributes: the expression of its default value, and the table's kind, choices, default, unit
/// and bounds.
struct Parts {
    kind: TokenStream,
    choices: TokenStream,
    value: TokenStream,
    default: TokenStream,
    unit: TokenStream,
    min: TokenStream,
    max: TokenStream,
    min_length: TokenStream,
    max_length: TokenStream,
}

/// A way to say what is wrong with the field being declared.
type Broken<'a> = &'a dyn Fn(&str) -> Error;

/// `field`, checked, as its default value and its row of the table.
fn declare(field: &Field) -> syn::Result<Declared> {
    let Some(name) = &field.ident else {
        return Err(Error::new_spanned(field, "a parameter needs a name"));
    };
    let broken = |problem: &str| Error::new_spanned(field, format!("parameter `{name}`: {problem}"));
    let doc = doc_of(&field.attrs).ok_or_else(|| {
        broken("needs a doc comment: what it does, its unit and range in words, what raising it does")
    })?;
    let settings = settings(field, name)?;
    let ty = &field.ty;
    if shape(ty) != Shape::Colours && (settings.min_length.is_some() || settings.max_length.is_some()) {
        return Err(broken(
            "only a list of colours (Vec<Color>) takes min_length and max_length",
        ));
    }
    let parts = if shape(ty) == Shape::Lut {
        let value = quote!(crate::params::LutName::default());
        unset_parts(
            &settings,
            "a lookup table",
            "none (the identity table)",
            (quote!(Lut), value),
            &broken,
        )?
    } else if shape(ty) == Shape::OptionalColour {
        let value = quote!(::core::option::Option::None);
        unset_parts(&settings, "an optional colour", "none", (quote!(Color), value), &broken)?
    } else {
        let default = settings.default.as_ref().ok_or_else(|| broken("needs a default"))?;
        let written = written(default).ok_or_else(|| {
            broken(
                "the default is a number, true or false, a '#rrggbb' colour or a field name in quotes, a list of colours or a variant name",
            )
        })?;
        match shape(ty) {
            Shape::Float | Shape::Int => number_parts(&settings, &written, shape(ty), &broken)?,
            other => other_parts(&settings, &written, other, ty, &broken)?,
        }
    };
    let Parts {
        kind,
        choices,
        value,
        default,
        unit,
        min,
        max,
        min_length,
        max_length,
    } = parts;
    let (label, cyclic, drivable, advanced) = (name.to_string(), settings.cyclic, settings.drivable, settings.advanced);
    let nullable = shape(ty) == Shape::OptionalColour;
    let row = quote! {
        crate::params::Field {
            name: #label,
            kind: #kind,
            meaning: #doc,
            default: #default,
            unit: #unit,
            min: #min,
            max: #max,
            cyclic: #cyclic,
            choices: #choices,
            min_length: #min_length,
            max_length: #max_length,
            nullable: #nullable,
            drivable: #drivable,
            advanced: #advanced,
        }
    };
    let write = match shape(ty) {
        Shape::Float => quote!(crate::params::decimal(self.#name)),
        Shape::Colour => quote!(::serde_json::Value::Array(
            self.#name.0.iter().copied().map(crate::params::decimal).collect()
        )),
        Shape::Colours => quote!(::serde_json::Value::Array(
            self.#name
                .iter()
                .map(|colour| ::serde_json::Value::Array(colour.0.iter().copied().map(crate::params::decimal).collect()))
                .collect()
        )),
        Shape::OptionalColour => quote!(self.#name.map_or(::serde_json::Value::Null, |colour| {
            ::serde_json::Value::Array(colour.0.iter().copied().map(crate::params::decimal).collect())
        })),
        Shape::Field => quote!(::serde_json::Value::from(self.#name.0.clone())),
        Shape::Lut => quote!(self.#name.json()),
        Shape::Choice => quote!(::serde_json::Value::from(crate::params::Choice::name(&self.#name))),
        Shape::Int | Shape::Switch => quote!(::serde_json::Value::from(self.#name)),
    };
    Ok(Declared {
        name: name.clone(),
        value,
        row,
        write,
    })
}

/// A switch, a colour, a list of colours or a choice: no unit, no bounds, not cyclic, not drivable, and a default of its
/// own kind.
fn other_parts(
    settings: &Settings,
    written: &Written,
    shape: Shape,
    ty: &Type,
    broken: Broken<'_>,
) -> syn::Result<Parts> {
    // A type the derive does not know is taken for a choice enum; when the declaration is not one's, the likelier
    // mistake is a number type (`f64`, `u32`, `Option<f32>`, an alias of `f32`), so the message names the type.
    let wrong = |problem: &str| {
        if shape == Shape::Choice {
            let ty = ty.to_token_stream().to_string().replace(' ', "");
            broken(&format!(
                "`{ty}` is not f32, i32, bool, Color, Option<Color>, Vec<Color>, FieldName or LutName, so it is taken for a `#[derive(Choice)]` enum, and {problem} \
                 (a number is f32 or i32)"
            ))
        } else {
            broken(problem)
        }
    };
    if settings.unit.is_some() || settings.min.is_some() || settings.max.is_some() {
        return Err(wrong("only a number takes a unit, min and max"));
    }
    if settings.cyclic {
        return Err(wrong("only a number in degrees can be cyclic"));
    }
    if settings.drivable {
        return Err(wrong("only a decimal number (f32) can be drivable"));
    }
    if shape == Shape::Colours {
        return colors::parts(settings, written, broken);
    }
    let (value, default) = match (shape, written) {
        (Shape::Switch, Written::Switch(on)) => (quote!(#on), quote!(crate::params::Scalar::Bool(#on))),
        (Shape::Colour, Written::Text(hex)) => {
            let channels = colour(hex).ok_or_else(|| broken("a colour default is '#rrggbb'"))?;
            (
                quote!(crate::params::Color([#(#channels),*])),
                quote!(crate::params::Scalar::Text(#hex)),
            )
        }
        (Shape::Choice, Written::Name(variant)) => {
            let choice = snake_case(&variant.to_string());
            (quote!(<#ty>::#variant), quote!(crate::params::Scalar::Text(#choice)))
        }
        (Shape::Field, Written::Text(selection)) if !selection.is_empty() => (
            quote!(crate::params::FieldName::new(#selection)),
            quote!(crate::params::Scalar::Text(#selection)),
        ),
        (Shape::Field, _) => return Err(broken("the default of a field is the name of a selection, in quotes")),
        (Shape::Switch, _) => return Err(broken("the default of a switch is true or false")),
        (Shape::Colour, _) => return Err(broken("the default of a colour is '#rrggbb', in quotes")),
        _ => return Err(wrong("the default of a choice is the name of one of its variants")),
    };
    let none = quote!(::core::option::Option::None);
    let (kind, choices) = match shape {
        Shape::Switch => (quote!(Bool), quote!(&[])),
        Shape::Colour => (quote!(Color), quote!(&[])),
        Shape::Field => (quote!(Field), quote!(&[])),
        _ => (quote!(Choice), quote!(<#ty as crate::params::Choice>::CHOICES)),
    };
    Ok(Parts {
        kind: quote!(crate::params::Kind::#kind),
        choices,
        value,
        default,
        unit: none.clone(),
        min: none.clone(),
        max: none.clone(),
        min_length: none.clone(),
        max_length: none,
    })
}

/// A lookup table (`LutName`) or an optional colour (`Option<Color>`): it has no value until a step gives one, so its
/// default is not written; no unit, bounds, lengths, cyclic or drivable either. `noun` names it for an error, `absent` what
/// its default is; `kind` and `value` are the table's kind and the expression of the field's value when none is given.
fn unset_parts(
    settings: &Settings,
    noun: &str,
    absent: &str,
    (kind, value): (TokenStream, TokenStream),
    broken: Broken<'_>,
) -> syn::Result<Parts> {
    if settings.default.is_some() {
        return Err(broken(&format!(
            "the default of {noun} is {absent}: leave `default` out"
        )));
    }
    if settings.unit.is_some() || settings.min.is_some() || settings.max.is_some() {
        return Err(broken("only a number takes a unit, min and max"));
    }
    if settings.cyclic || settings.drivable {
        return Err(broken("only a decimal number can be cyclic or drivable"));
    }
    let none = quote!(::core::option::Option::None);
    Ok(Parts {
        kind: quote!(crate::params::Kind::#kind),
        choices: quote!(&[]),
        value,
        default: quote!(crate::params::Scalar::Null),
        unit: none.clone(),
        min: none.clone(),
        max: none.clone(),
        min_length: none.clone(),
        max_length: none,
    })
}

/// A decimal (`f32`) or whole (`i32`) number: a unit, both bounds, a default inside them.
fn number_parts(settings: &Settings, written: &Written, shape: Shape, broken: Broken<'_>) -> syn::Result<Parts> {
    let Written::Number(number) = *written else {
        return Err(broken("the default of a number is a number"));
    };
    let (Some(unit), Some(low), Some(high)) = (&settings.unit, &settings.min, &settings.max) else {
        return Err(broken(
            "a number needs a unit and both min and max (its documented range)",
        ));
    };
    let whole = shape == Shape::Int;
    let check = |number: Number, what: &str| match number {
        Number::Float(_) if whole => Err(broken(&format!(
            "a whole number takes whole numbers, and its {what} is a decimal"
        ))),
        Number::Int(value) if whole && i32::try_from(value).is_err() => {
            Err(broken(&format!("its {what} does not fit i32")))
        }
        _ => Ok(number),
    };
    let (number, low, high) = (
        check(number, "default")?,
        check(num(low, broken)?, "min")?,
        check(num(high, broken)?, "max")?,
    );
    if low.as_f64() > high.as_f64() {
        return Err(broken("min is above max"));
    }
    if !(low.as_f64()..=high.as_f64()).contains(&number.as_f64()) {
        return Err(broken("the default is outside min .. max"));
    }
    if settings.cyclic && (unit != "degrees" || (high.as_f64() - low.as_f64() - TURN).abs() > f64::EPSILON) {
        return Err(broken("a cyclic field is in degrees, min to max one turn (360)"));
    }
    if settings.drivable && whole {
        return Err(broken(
            "only a decimal number (f32) can be drivable, and this is a whole number",
        ));
    }
    let value = number_value(number, whole).ok_or_else(|| broken("the default does not fit i32"))?;
    let unit = Ident::new(&capitalised(&unit.to_string()), unit.span());
    let (low, high) = (low.scalar(), high.scalar());
    let kind = if whole { quote!(Int) } else { quote!(Float) };
    Ok(Parts {
        kind: quote!(crate::params::Kind::#kind),
        choices: quote!(&[]),
        value,
        default: number.scalar(),
        unit: quote!(::core::option::Option::Some(crate::params::Unit::#unit)),
        min: quote!(::core::option::Option::Some(#low)),
        max: quote!(::core::option::Option::Some(#high)),
        min_length: quote!(::core::option::Option::None),
        max_length: quote!(::core::option::Option::None),
    })
}

/// The expression of `number` as a field value: an `i32` literal for a whole number, else an `f32` one.
#[expect(
    clippy::cast_possible_truncation,
    reason = "an f32 field holds the nearest f32 to the decimal written"
)]
fn number_value(number: Number, whole: bool) -> Option<TokenStream> {
    let literal = if whole {
        let Number::Int(value) = number else { return None };
        Literal::i32_suffixed(i32::try_from(value).ok()?)
    } else {
        Literal::f32_suffixed(number.as_f64() as f32)
    };
    Some(quote!(#literal))
}

/// The attribute `#[param(..)]` of `field`, read.
fn settings(field: &Field, name: &Ident) -> syn::Result<Settings> {
    let mut attributes = field
        .attrs
        .iter()
        .filter(|attribute| attribute.path().is_ident("param"));
    let Some(attribute) = attributes.next() else {
        return Err(Error::new_spanned(
            field,
            format!("parameter `{name}`: declare it with #[param(default = .., ..)]"),
        ));
    };
    if attributes.next().is_some() {
        return Err(Error::new_spanned(
            field,
            format!("parameter `{name}`: declare it in one #[param(..)] attribute, not several"),
        ));
    }
    let mut settings = Settings::default();
    let mut given = Vec::new();
    attribute.parse_nested_meta(|meta| {
        let key = meta.path.get_ident().map(ToString::to_string).unwrap_or_default();
        if given.contains(&key) {
            return Err(meta.error(format!("parameter `{name}`: `{key}` is given twice")));
        }
        match key.as_str() {
            "default" => settings.default = Some(meta.value()?.parse()?),
            "unit" => settings.unit = Some(meta.value()?.parse()?),
            "min" => settings.min = Some(meta.value()?.parse()?),
            "max" => settings.max = Some(meta.value()?.parse()?),
            "min_length" => settings.min_length = Some(meta.value()?.parse()?),
            "max_length" => settings.max_length = Some(meta.value()?.parse()?),
            "drivable" => settings.drivable = true,
            "advanced" => settings.advanced = true,
            "cyclic" => settings.cyclic = true,
            _ => {
                return Err(meta.error(format!(
                    "parameter `{name}`: `{key}` is not a setting; they are default, unit, min, max, min_length, max_length, drivable, advanced, cyclic"
                )));
            }
        }
        given.push(key);
        Ok(())
    })?;
    Ok(settings)
}

/// What `default = ..` says: a list of texts in brackets (syn's `full` feature, which would parse an array expression, is
/// not enabled), else one expression.
enum Given {
    List(Vec<String>),
    One(Expr),
}

impl Parse for Given {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if !input.peek(token::Bracket) {
            return input.parse().map(Self::One);
        }
        let items;
        bracketed!(items in input);
        let texts = Punctuated::<LitStr, Token![,]>::parse_terminated(&items)?;
        Ok(Self::List(texts.iter().map(LitStr::value).collect()))
    }
}

/// `given` as a default: a number (a minus sign allowed), true or false, a colour text, a list of them or a variant name.
fn written(given: &Given) -> Option<Written> {
    let expr = match given {
        Given::List(texts) => return Some(Written::List(texts.clone())),
        Given::One(expr) => expr,
    };
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Bool(switch), ..
        }) => Some(Written::Switch(switch.value)),
        Expr::Lit(ExprLit {
            lit: Lit::Str(text), ..
        }) => Some(Written::Text(text.value())),
        Expr::Path(path) => path.path.get_ident().cloned().map(Written::Name),
        _ => number(expr).map(Written::Number),
    }
}

/// `expr` as a bound.
fn num(expr: &Expr, broken: Broken<'_>) -> syn::Result<Number> {
    number(expr).ok_or_else(|| broken("min and max are numbers"))
}

/// `expr` as a number: an integer or decimal literal, a minus sign allowed.
fn number(expr: &Expr) -> Option<Number> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Int(whole), ..
        }) => whole.base10_parse().ok().map(Number::Int),
        Expr::Lit(ExprLit {
            lit: Lit::Float(real), ..
        }) => real.base10_parse().ok().map(Number::Float),
        Expr::Unary(unary) if matches!(unary.op, UnOp::Neg(_)) => number(&unary.expr).map(|number| match number {
            Number::Int(whole) => Number::Int(-whole),
            Number::Float(real) => Number::Float(-real),
        }),
        _ => None,
    }
}

/// The three channels, 0..1 as `f32` literals, of the colour `hex` (`#rrggbb`, lowercase as the parameter format writes it).
fn colour(hex: &str) -> Option<[Literal; 3]> {
    let digits = hex
        .strip_prefix('#')
        .filter(|digits| digits.len() == 6 && digits.chars().all(|digit| matches!(digit, '0'..='9' | 'a'..='f')))?;
    let channel = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();
    let levels = [channel(0)?, channel(2)?, channel(4)?];
    Some(levels.map(|level| Literal::f32_suffixed(f32::from(level) / 255.0))) // constant: 8-bit levels
}

/// `word` with a capital first letter (`px` is `Px`).
fn capitalised(word: &str) -> String {
    let mut letters = word.chars();
    letters
        .next()
        .map_or_else(String::new, |first| first.to_uppercase().chain(letters).collect())
}

/// The doc comment of an item: each `///` line without its one leading space, joined with a space; none if it is
/// missing or empty.
fn doc_of(attributes: &[Attribute]) -> Option<String> {
    let lines: Vec<String> = attributes
        .iter()
        .filter_map(|attribute| match &attribute.meta {
            Meta::NameValue(pair) if pair.path.is_ident("doc") => match &pair.value {
                Expr::Lit(ExprLit {
                    lit: Lit::Str(line), ..
                }) => Some(line.value()),
                _ => None,
            },
            _ => None,
        })
        .map(|line| line.strip_prefix(' ').map_or(line.clone(), str::to_owned))
        .collect();
    let doc = lines.join(" ");
    (!doc.trim().is_empty()).then_some(doc)
}

#[cfg(test)]
mod tests;
