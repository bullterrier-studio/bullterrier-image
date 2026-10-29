//! `#[derive(Choice)]`: the choices of a parameter, from the variants of a fieldless enum.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Error, Fields};

use crate::snake_case;

/// The code `#[derive(Choice)]` writes for `input`: its names as the parameter's choices, and a `Deserialize` that
/// reads one of them; an error naming the type or variant if `input` is not a fieldless enum.
pub(crate) fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let Data::Enum(data) = &input.data else {
        return Err(Error::new_spanned(
            name,
            format!("`{name}` must be an enum: #[derive(Choice)] lists its variants"),
        ));
    };
    if data.variants.is_empty() {
        return Err(Error::new_spanned(
            name,
            format!("`{name}` needs at least one variant: a choice is one of them"),
        ));
    }
    if let Some(variant) = data
        .variants
        .iter()
        .find(|variant| !matches!(variant.fields, Fields::Unit))
    {
        let variant_name = &variant.ident;
        return Err(Error::new_spanned(
            variant,
            format!("variant `{variant_name}` of `{name}` holds data: a choice is a name alone"),
        ));
    }
    let variants: Vec<_> = data.variants.iter().map(|variant| &variant.ident).collect();
    let names: Vec<_> = variants
        .iter()
        .map(|variant| snake_case(&variant.to_string()))
        .collect();
    Ok(quote! {
        impl crate::params::Parameter for #name {
            const KIND: crate::params::Kind = crate::params::Kind::Choice;
            const CHOICES: &'static [&'static str] = &[#(#names),*];
        }

        impl<'de> ::serde::Deserialize<'de> for #name {
            fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> ::core::result::Result<Self, D::Error> {
                struct Choices;

                impl ::serde::de::Visitor<'_> for Choices {
                    type Value = #name;

                    fn expecting(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                        formatter.write_str("the name of a choice")
                    }

                    fn visit_str<E: ::serde::de::Error>(self, text: &str) -> ::core::result::Result<#name, E> {
                        match text {
                            #(#names => ::core::result::Result::Ok(#name::#variants),)*
                            _ => ::core::result::Result::Err(E::unknown_variant(
                                text,
                                <#name as crate::params::Parameter>::CHOICES,
                            )),
                        }
                    }
                }

                deserializer.deserialize_str(Choices)
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::expand;
    use syn::{DeriveInput, parse_quote};

    fn expanded(input: &DeriveInput) -> Result<String, String> {
        expand(input)
            .map(|code| code.to_string())
            .map_err(|error| error.to_string())
    }

    #[test]
    fn a_fieldless_enum_gives_its_choices_in_snake_case_in_declaration_order() {
        let code = expanded(&parse_quote! {
            enum Kind { Stripes, Checks, FourthWay }
        });
        let places = code.as_deref().ok().and_then(|code| {
            Some((
                code.find("\"stripes\"")?,
                code.find("\"checks\"")?,
                code.find("\"fourth_way\"")?,
            ))
        });
        assert!(
            matches!(places, Some((first, second, third)) if first < second && second < third),
            "{code:?}"
        );
    }

    #[test]
    fn what_is_not_a_fieldless_enum_is_refused_naming_it() {
        let cases: [(DeriveInput, &str); 4] = [
            (parse_quote! { struct Kind { a: u8 } }, "enum"),
            (parse_quote! { enum Kind {} }, "at least one"),
            (parse_quote! { enum Kind { Stripes(u8) } }, "Stripes"),
            (parse_quote! { enum Kind { Stripes { width: u8 } } }, "Stripes"),
        ];
        for (input, named) in cases {
            let error = expanded(&input).err();
            assert!(
                error
                    .as_deref()
                    .is_some_and(|text| text.contains(named) && text.contains("Kind")),
                "{named}: {error:?}"
            );
        }
    }
}
