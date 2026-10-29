//! The derive macros an effect of bullterrier-image declares its parameters with. The core re-exports both
//! (`bullterrier_image::params`); an effect imports them from there.
//!
//! - `#[derive(Params)]` on an effect's `Params` struct: each field is declared once, by its type, its `///` doc
//!   comment and a `#[param(default = .., unit = .., min = .., max = .., drivable, advanced, cyclic)]` attribute. The
//!   struct's own doc comment documents the effect's parameters as a whole. The derive writes `Default` and the table
//!   the Python package builds the effect's model from, and refuses a broken declaration at compile time, naming the
//!   field.
//! - `#[derive(Choice)]` on a fieldless enum: the choices of a parameter, named in `snake_case` in declaration order.
//!   It reads them from text, so the names are written once, in the enum.
//!
//! The code they write refers to the core's `crate::params` paths: every effect lives in the core.

mod choice;
mod params;

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

/// Derive `Default` and `Declared` (the parameters' table) for an effect's `Params` struct; see the crate documentation.
#[proc_macro_derive(Params, attributes(param))]
pub fn derive_params(input: TokenStream) -> TokenStream {
    params::expand(&parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Derive `Parameter` and `Deserialize` for a fieldless enum whose variants are the choices of a parameter.
#[proc_macro_derive(Choice)]
pub fn derive_choice(input: TokenStream) -> TokenStream {
    choice::expand(&parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// `name` in `snake_case`: an uppercase letter starts a new word (`FourthWay` is `fourth_way`).
fn snake_case(name: &str) -> String {
    let mut snake = String::new();
    for (index, letter) in name.chars().enumerate() {
        if letter.is_uppercase() && index > 0 {
            snake.push('_');
        }
        snake.extend(letter.to_lowercase());
    }
    snake
}
