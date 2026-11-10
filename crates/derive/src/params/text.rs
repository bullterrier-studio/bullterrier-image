//! A `String` field: free text with, optionally, the fewest and most characters and the fewest different ones.

use proc_macro2::{Literal, TokenStream};
use quote::quote;
use syn::{Expr, ExprLit, Lit};

use super::{Broken, Parts, Settings, Written};

/// The parts of a text: its default as a `String` and as the text the table shows, and its length rules. Refused, naming
/// the field, when a rule is not a whole number, `min_length` is above `max_length`, or the default is not text inside
/// every rule (a length counts characters).
pub(super) fn parts(settings: &Settings, written: &Written, broken: Broken<'_>) -> syn::Result<Parts> {
    if settings.unit.is_some() || settings.min.is_some() || settings.max.is_some() {
        return Err(broken("only a number takes a unit, min and max"));
    }
    if settings.cyclic || settings.drivable {
        return Err(broken("only a decimal number can be cyclic or drivable"));
    }
    let Written::Text(text) = written else {
        return Err(broken("the default of a text is text, in quotes"));
    };
    let least = rule(settings.min_length.as_ref(), "min_length", broken)?;
    let most = rule(settings.max_length.as_ref(), "max_length", broken)?;
    let different = rule(settings.min_distinct.as_ref(), "min_distinct", broken)?;
    if let (Some(least), Some(most)) = (least, most)
        && least > most
    {
        return Err(broken("min_length is above max_length"));
    }
    let length = text.chars().count();
    if least.is_some_and(|least| length < least) || most.is_some_and(|most| length > most) {
        return Err(broken(
            "the default has fewer or more characters than min_length .. max_length allow",
        ));
    }
    let distinct = text.chars().collect::<std::collections::BTreeSet<_>>().len();
    if different.is_some_and(|different| distinct < different) {
        return Err(broken(
            "the default has fewer different characters than min_distinct requires",
        ));
    }
    let none = quote!(::core::option::Option::None);
    Ok(Parts {
        kind: quote!(crate::params::Kind::Text),
        choices: quote!(&[]),
        value: quote!(::std::string::String::from(#text)),
        default: quote!(crate::params::Scalar::Text(#text)),
        unit: none.clone(),
        min: none.clone(),
        max: none,
        min_length: optional(least),
        max_length: optional(most),
    })
}

/// The table's `min_distinct`: the rule the field gives, or none.
pub(super) fn distinct(settings: &Settings, broken: Broken<'_>) -> syn::Result<TokenStream> {
    Ok(optional(rule(settings.min_distinct.as_ref(), "min_distinct", broken)?))
}

/// `Some(n)` or `None` as a table writes it.
fn optional(count: Option<usize>) -> TokenStream {
    count.map_or_else(
        || quote!(::core::option::Option::None),
        |count| {
            let count = Literal::usize_suffixed(count);
            quote!(::core::option::Option::Some(#count))
        },
    )
}

/// The rule `name` written as `expr`: a whole number literal, or none if it is not given.
fn rule(expr: Option<&Expr>, name: &str, broken: Broken<'_>) -> syn::Result<Option<usize>> {
    expr.map(|expr| match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Int(whole), ..
        }) => whole
            .base10_parse()
            .map_err(|_| broken(&format!("{name} is a whole number"))),
        _ => Err(broken(&format!("{name} is a whole number"))),
    })
    .transpose()
}
