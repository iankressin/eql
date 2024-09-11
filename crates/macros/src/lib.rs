extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput};

#[proc_macro_derive(EnumVariants)]
pub fn enum_variants_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    let variants = match input.data {
        Data::Enum(data_enum) => data_enum.variants,
        _ => panic!("EnumVariants can only be applied to enums"),
    };

    let variant_names = variants.iter().map(|variant| &variant.ident);

    let expanded = quote! {
        impl #name {
            pub fn all_variants() -> &'static [#name] {
                &[
                    #( #name::#variant_names, )*
                ]
            }
        }
    };

    TokenStream::from(expanded)
}