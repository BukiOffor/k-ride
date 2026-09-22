use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Data, Fields};

#[proc_macro_derive(Validate)]
pub fn derive_validate(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let validation_checks = match &input.data {
        Data::Struct(data) => {
            match &data.fields {
                Fields::Named(fields) => {
                    let checks = fields.named.iter().filter_map(|f| {
                        let field_name = &f.ident;
                        let field_type = &f.ty;
                        
                        // Check if the type is String
                        if is_string_type(field_type) {
                            Some(quote! {
                                if self.#field_name.is_empty() {
                                    return Err(crate::errors::ModuleError::Error(format!("field '{}' cannot be empty", stringify!(#field_name)).into()));
                                }
                            })
                        } else {
                            None
                        }
                    });
                    quote! { #(#checks)* }
                }
                _ => quote! {},
            }
        }
        _ => panic!("Validate can only be derived for structs"),
    };

    let expanded = quote! {
        impl crate::traits::Validate for #name {
            fn validate(&self) -> Result<(), crate::errors::ModuleError> {
                #validation_checks
                Ok(())
            }
        }
    };

    TokenStream::from(expanded)
}

fn is_string_type(ty: &syn::Type) -> bool {
    if let syn::Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "String";
        }
    }
    false
}