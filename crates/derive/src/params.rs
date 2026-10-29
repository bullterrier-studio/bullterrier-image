//! `#[derive(Params)]`: an effect's parameters, declared by field type, doc comment and `#[param(..)]`.

use proc_macro2::{Literal, TokenStream};
use quote::quote;
use syn::{
    Attribute, Data, DeriveInput, Error, Expr, ExprLit, Field, Fields, Ident, Lit, Meta, Type, UnOp, Visibility,
};

use crate::snake_case;

/// The turn a cyclic field spans, in degrees.
const TURN: f64 = 360.0;

/// The code `#[derive(Params)]` writes for `input`: `Default` and `Declared` (the table). A compile error naming the
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
        }
    })
}

/// One field, declared: its name, the expression of its default value, and its row of the table.
struct Declared {
    name: Ident,
    value: TokenStream,
    row: TokenStream,
}

/// What a field's `#[param(..)]` attribute says.
#[derive(Default)]
struct Settings {
    default: Option<Expr>,
    unit: Option<Ident>,
    min: Option<Expr>,
    max: Option<Expr>,
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
    /// A variant of a choice.
    Name(Ident),
}

/// How the derive reads a field's type: it knows the two numeric primitives, to check their declaration; every other
/// type reports its kind itself (`Parameter`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Float,
    Int,
    Other,
}

fn shape(ty: &Type) -> Shape {
    match ty {
        Type::Path(path) if path.path.is_ident("f32") => Shape::Float,
        Type::Path(path) if path.path.is_ident("i32") => Shape::Int,
        _ => Shape::Other,
    }
}

/// What a checked field contributes: the expression of its default value, and the table's default, unit and bounds.
struct Parts {
    value: TokenStream,
    default: TokenStream,
    unit: TokenStream,
    min: TokenStream,
    max: TokenStream,
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
    if matches!(field.vis, Visibility::Public(_)) {
        // a `pub` field is never reported when `render` does not read it; fields become `pub` with the typed entry
        // point of the core, which will need a test that every parameter is read
        return Err(broken(
            "declare it `pub(crate)`: only fields no code outside the crate can reach are reported when unread",
        ));
    }
    let settings = settings(field, name)?;
    let default = settings.default.as_ref().ok_or_else(|| broken("needs a default"))?;
    let written = written(default).ok_or_else(|| {
        broken("the default is a number, true or false, a '#rrggbb' colour in quotes or a variant name")
    })?;
    let ty = &field.ty;
    let parts = match shape(ty) {
        Shape::Other => other_parts(&settings, &written, ty, &broken)?,
        numeric => number_parts(&settings, &written, numeric, &broken)?,
    };
    let Parts {
        value,
        default,
        unit,
        min,
        max,
    } = parts;
    let (label, cyclic, drivable, advanced) = (name.to_string(), settings.cyclic, settings.drivable, settings.advanced);
    let row = quote! {
        crate::params::Field {
            name: #label,
            kind: <#ty as crate::params::Parameter>::KIND,
            doc: #doc,
            default: #default,
            unit: #unit,
            min: #min,
            max: #max,
            cyclic: #cyclic,
            choices: <#ty as crate::params::Parameter>::CHOICES,
            drivable: #drivable,
            advanced: #advanced,
        }
    };
    Ok(Declared {
        name: name.clone(),
        value,
        row,
    })
}

/// A switch, a colour or a choice: no unit, no bounds, not cyclic, not drivable.
fn other_parts(settings: &Settings, written: &Written, ty: &Type, broken: Broken<'_>) -> syn::Result<Parts> {
    if settings.unit.is_some() || settings.min.is_some() || settings.max.is_some() {
        return Err(broken("only a number takes a unit, min and max"));
    }
    if settings.cyclic {
        return Err(broken("only a number in degrees can be cyclic"));
    }
    if settings.drivable {
        return Err(broken("only a decimal number (f32) can be drivable"));
    }
    let (value, default) = match written {
        Written::Switch(on) => (quote!(#on), quote!(crate::params::Scalar::Bool(#on))),
        Written::Text(hex) => {
            let channels = colour(hex).ok_or_else(|| broken("a colour default is '#rrggbb'"))?;
            (
                quote!(crate::params::Color([#(#channels),*])),
                quote!(crate::params::Scalar::Text(#hex)),
            )
        }
        Written::Name(variant) => {
            let choice = snake_case(&variant.to_string());
            (quote!(<#ty>::#variant), quote!(crate::params::Scalar::Text(#choice)))
        }
        Written::Number(_) => return Err(broken("a number default needs a number field (f32 or i32)")),
    };
    let none = quote!(::core::option::Option::None);
    Ok(Parts {
        value,
        default,
        unit: none.clone(),
        min: none.clone(),
        max: none,
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
    let check = |number: Number, what: &str| {
        if whole && matches!(number, Number::Float(_)) {
            Err(broken(&format!(
                "a whole number takes whole numbers, and its {what} is a decimal"
            )))
        } else {
            Ok(number)
        }
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
    Ok(Parts {
        value,
        default: number.scalar(),
        unit: quote!(::core::option::Option::Some(crate::params::Unit::#unit)),
        min: quote!(::core::option::Option::Some(#low)),
        max: quote!(::core::option::Option::Some(#high)),
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
    let attribute = field
        .attrs
        .iter()
        .find(|attribute| attribute.path().is_ident("param"))
        .ok_or_else(|| {
            Error::new_spanned(
                field,
                format!("parameter `{name}`: declare it with #[param(default = .., ..)]"),
            )
        })?;
    let mut settings = Settings::default();
    attribute.parse_nested_meta(|meta| {
        let key = meta.path.get_ident().map(ToString::to_string).unwrap_or_default();
        match key.as_str() {
            "default" => settings.default = Some(meta.value()?.parse()?),
            "unit" => settings.unit = Some(meta.value()?.parse()?),
            "min" => settings.min = Some(meta.value()?.parse()?),
            "max" => settings.max = Some(meta.value()?.parse()?),
            "drivable" => settings.drivable = true,
            "advanced" => settings.advanced = true,
            "cyclic" => settings.cyclic = true,
            _ => {
                return Err(meta.error(format!(
                    "parameter `{name}`: `{key}` is not a setting; they are default, unit, min, max, drivable, advanced, cyclic"
                )));
            }
        }
        Ok(())
    })?;
    Ok(settings)
}

/// `expr` as a default: a number (a minus sign allowed), true or false, a colour text or a variant name.
fn written(expr: &Expr) -> Option<Written> {
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

/// The three channels, 0..1 as `f32` literals, of the colour `hex` (`#rrggbb`).
fn colour(hex: &str) -> Option<[Literal; 3]> {
    let digits = hex
        .strip_prefix('#')
        .filter(|digits| digits.len() == 6 && digits.is_ascii())?;
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
mod tests {
    use super::expand;
    use syn::{DeriveInput, parse_quote};

    /// A documented `Params` struct whose one field, documented, is written in the call.
    macro_rules! documented {
        ($($field:tt)*) => {{
            let input: DeriveInput = parse_quote! {
                /// The parameters of a probe.
                struct Params {
                    /// A field.
                    $($field)*
                }
            };
            input
        }};
    }

    fn expanded(input: &DeriveInput) -> Result<String, String> {
        expand(input)
            .map(|code| code.to_string())
            .map_err(|error| error.to_string())
    }

    /// Each row must be refused with an error that names every word of `named`.
    fn assert_refused(rows: &[(&str, DeriveInput, &[&str])]) {
        for (rule, input, named) in rows {
            let error = expanded(input).err();
            assert!(
                error
                    .as_deref()
                    .is_some_and(|text| named.iter().all(|name| text.contains(name))),
                "{rule}: {error:?}"
            );
        }
    }

    #[test]
    fn a_complete_declaration_is_accepted_for_every_kind_of_field() {
        let good = [
            documented! { #[param(default = 4.0, unit = px, min = 0, max = 32, drivable, advanced)] cell: f32, },
            documented! { #[param(default = 4, unit = count, min = 3, max = 32)] cells: i32, },
            documented! { #[param(default = 4, unit = count, min = 3, max = 32)] pub(crate) crate_wide: i32, },
            // the bounds are inclusive: a default may sit on either
            documented! { #[param(default = 0.0, unit = px, min = 0, max = 32)] low: f32, },
            documented! { #[param(default = 32.0, unit = px, min = 0, max = 32)] high: f32, },
            documented! { #[param(default = false)] keep: bool, },
            documented! { #[param(default = "#101018")] tint: Color, },
            documented! { #[param(default = Grid)] kind: Kind, },
            documented! { #[param(default = 0.0, unit = degrees, min = 0, max = 360, cyclic)] angle: f32, },
            // as rustfmt wraps a long attribute
            documented! {
                #[param(
                    default = -90.0,
                    unit = degrees,
                    min = -180,
                    max = 180,
                    cyclic,
                )]
                lean: f32,
            },
        ];
        for input in &good {
            let code = expanded(input);
            assert!(code.as_ref().is_ok_and(|code| code.contains("Declared")), "{code:?}");
        }
    }

    #[test]
    fn a_declaration_missing_a_part_is_refused_naming_the_field() {
        assert_refused(&[
            (
                "no doc",
                parse_quote! { /// The parameters.
                struct Params { #[param(default = 4.0, unit = px, min = 0, max = 32)] cell: f32, } },
                &["cell", "doc"],
            ),
            ("no param attribute", documented! { cell: f32, }, &["cell", "param"]),
            (
                "a public field, which the dead-code lint would not report if render never read it",
                documented! { #[param(default = 4.0, unit = px, min = 0, max = 32)] pub cell: f32, },
                &["cell", "pub(crate)"],
            ),
            (
                "no default",
                documented! { #[param(unit = px, min = 0, max = 32)] cell: f32, },
                &["cell", "default"],
            ),
            (
                "no unit",
                documented! { #[param(default = 4.0, min = 0, max = 32)] cell: f32, },
                &["cell", "unit"],
            ),
            (
                "no min",
                documented! { #[param(default = 4.0, unit = px, max = 32)] cell: f32, },
                &["cell", "min"],
            ),
            (
                "no max",
                documented! { #[param(default = 4.0, unit = px, min = 0)] cell: f32, },
                &["cell", "max"],
            ),
            (
                "a setting nobody knows",
                documented! { #[param(default = 4.0, unit = px, min = 0, max = 32, strength = 2)] cell: f32, },
                &["cell", "strength"],
            ),
        ]);
    }

    #[test]
    fn a_number_that_breaks_its_range_is_refused_naming_the_field() {
        assert_refused(&[
            (
                "min above max",
                documented! { #[param(default = 4.0, unit = px, min = 32, max = 0)] cell: f32, },
                &["cell", "min"],
            ),
            (
                "default below the range",
                documented! { #[param(default = -1.0, unit = px, min = 0, max = 32)] cell: f32, },
                &["cell", "default"],
            ),
            (
                "default above the range",
                documented! { #[param(default = 33, unit = px, min = 0, max = 32)] cell: f32, },
                &["cell", "default"],
            ),
            (
                "cyclic, not in degrees",
                documented! { #[param(default = 0.0, unit = px, min = 0, max = 360, cyclic)] cell: f32, },
                &["cell", "cyclic"],
            ),
            (
                "cyclic, not a turn",
                documented! { #[param(default = 0.0, unit = degrees, min = 0, max = 180, cyclic)] cell: f32, },
                &["cell", "cyclic"],
            ),
            (
                "a decimal default for a whole number",
                documented! { #[param(default = 4.5, unit = count, min = 3, max = 32)] cells: i32, },
                &["cells", "whole"],
            ),
            (
                "a decimal bound for a whole number",
                documented! { #[param(default = 4, unit = count, min = 3, max = 32.0)] cells: i32, },
                &["cells", "whole"],
            ),
            (
                "a whole number that is drivable",
                documented! { #[param(default = 4, unit = count, min = 3, max = 32, drivable)] cells: i32, },
                &["cells", "drivable"],
            ),
        ]);
    }

    #[test]
    fn a_switch_colour_or_choice_that_takes_a_number_setting_is_refused_naming_the_field() {
        assert_refused(&[
            (
                "a unit on a switch",
                documented! { #[param(default = false, unit = px)] keep: bool, },
                &["keep", "unit"],
            ),
            (
                "a bound on a colour",
                documented! { #[param(default = "#101018", min = 0)] tint: Color, },
                &["tint", "min"],
            ),
            (
                "a bound on a choice",
                documented! { #[param(default = Grid, max = 3)] kind: Kind, },
                &["kind", "max"],
            ),
            (
                "cyclic on a switch",
                documented! { #[param(default = false, cyclic)] keep: bool, },
                &["keep", "cyclic"],
            ),
            (
                "drivable on a switch",
                documented! { #[param(default = false, drivable)] keep: bool, },
                &["keep", "drivable"],
            ),
            (
                "a colour that is not #rrggbb",
                documented! { #[param(default = "red")] tint: Color, },
                &["tint", "#rrggbb"],
            ),
            (
                "a number for a switch",
                documented! { #[param(default = 1.0)] keep: bool, },
                &["keep", "number"],
            ),
        ]);
    }

    #[test]
    fn what_is_not_a_documented_struct_of_named_fields_is_refused_naming_it() {
        let undocumented: DeriveInput = parse_quote! {
            struct Params {
                /// A size.
                #[param(default = 4.0, unit = px, min = 0, max = 32)]
                cell: f32,
            }
        };
        let tuple: DeriveInput = parse_quote! {
            /// The parameters.
            struct Params(f32);
        };
        let choice: DeriveInput = parse_quote! {
            /// The parameters.
            enum Params { A }
        };
        assert_refused(&[
            ("no doc on the struct", undocumented, &["Params", "doc"]),
            ("a tuple struct", tuple, &["Params", "named fields"]),
            ("an enum", choice, &["Params", "struct"]),
        ]);
    }
}
