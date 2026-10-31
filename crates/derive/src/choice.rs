//! `#[derive(Choice)]`: the choices of a parameter, from the variants of a fieldless enum.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Error, Fields};

use crate::snake_case;

/// The code `#[derive(Choice)]` writes for `input`: its names as the parameter's choices, the name of each variant, and a `Deserialize` that
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
    for (index, variant) in variants.iter().enumerate() {
        if let Some(first) = names[..index].iter().position(|earlier| *earlier == names[index]) {
            let (first, name_of_both) = (variants[first], &names[index]);
            return Err(Error::new_spanned(
                variant,
                format!(
                    "variants `{first}` and `{variant}` of `{name}` are both called `{name_of_both}`: a choice has one name each"
                ),
            ));
        }
    }
    Ok(quote! {
        impl crate::params::Choice for #name {
            const CHOICES: &'static [&'static str] = &[#(#names),*];

            fn name(&self) -> &'static str {
                match self {
                    #(Self::#variants => #names,)*
                }
            }
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
                                <#name as crate::params::Choice>::CHOICES,
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
    fn what_is_not_a_fieldless_enum_is_refused_naming_it() {
        let cases: [(DeriveInput, &[&str]); 5] = [
            (parse_quote! { struct Kind { a: u8 } }, &["enum"]),
            (parse_quote! { enum Kind {} }, &["at least one"]),
            (parse_quote! { enum Kind { Stripes(u8) } }, &["Stripes"]),
            (parse_quote! { enum Kind { Stripes { width: u8 } } }, &["Stripes"]),
            (
                parse_quote! { enum Kind { FourthWay, Fourth_way } },
                &["FourthWay", "Fourth_way", "fourth_way"],
            ),
        ];
        for (input, named) in cases {
            let error = expanded(&input).err();
            assert!(
                error
                    .as_deref()
                    .is_some_and(|text| text.contains("Kind") && named.iter().all(|name| text.contains(name))),
                "{named:?}: {error:?}"
            );
        }
    }
}
