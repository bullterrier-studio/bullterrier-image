//! A `Vec<Color>` field: a list of colours with the fewest it takes, optionally the most, and a default inside both.

use proc_macro2::Literal;
use quote::quote;
use syn::{Expr, ExprLit, Lit};

use super::{Broken, Parts, Settings, Written, colour};

/// The parts of a list of colours: its default as a `Vec` of colours and as the texts the table shows, and its lengths.
/// Refused, naming the field, when it needs no `min_length` of at least 1, has a `max_length` below it, or has a default
/// that is not a list of `'#rrggbb'` colours of a length inside both.
pub(super) fn parts(settings: &Settings, written: &Written, broken: Broken<'_>) -> syn::Result<Parts> {
    let Written::List(texts) = written else {
        return Err(broken(
            "the default of a list of colours is a list of '#rrggbb' texts, in quotes",
        ));
    };
    let Some(least) = settings.min_length.as_ref() else {
        return Err(broken("a list of colours needs min_length (at least 1)"));
    };
    let least = length(least).ok_or_else(|| broken("min_length is a whole number"))?;
    let most = settings
        .max_length
        .as_ref()
        .map(|most| length(most).ok_or_else(|| broken("max_length is a whole number")))
        .transpose()?;
    if least == 0 {
        return Err(broken("min_length is at least 1: a list of colours is never empty"));
    }
    if most.is_some_and(|most| most < least) {
        return Err(broken("min_length is above max_length"));
    }
    if texts.len() < least || most.is_some_and(|most| texts.len() > most) {
        return Err(broken(
            "the default has fewer or more colours than min_length .. max_length allow",
        ));
    }
    let colours = texts
        .iter()
        .map(|text| colour(text).ok_or_else(|| broken("every colour of the default is '#rrggbb'")))
        .collect::<syn::Result<Vec<_>>>()?;
    let colours = colours
        .iter()
        .map(|channels| quote!(crate::params::Color([#(#channels),*])));
    let most = most.map_or_else(
        || quote!(::core::option::Option::None),
        |most| {
            let most = Literal::usize_suffixed(most);
            quote!(::core::option::Option::Some(#most))
        },
    );
    let least = Literal::usize_suffixed(least);
    let none = quote!(::core::option::Option::None);
    Ok(Parts {
        kind: quote!(crate::params::Kind::Colors),
        choices: quote!(&[]),
        value: quote!(::std::vec![#(#colours),*]),
        default: quote!(crate::params::Scalar::List(&[#(#texts),*])),
        unit: none.clone(),
        min: none.clone(),
        max: none,
        min_length: quote!(::core::option::Option::Some(#least)),
        max_length: most,
    })
}

/// `expr` as a length: a whole number literal, zero or more.
fn length(expr: &Expr) -> Option<usize> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Int(whole), ..
        }) => whole.base10_parse().ok(),
        _ => None,
    }
}
