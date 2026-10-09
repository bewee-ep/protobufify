//! `#[derive(ProtoEncode)]`.

use std::collections::HashMap;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Error, Fields, ext::IdentExt, spanned::Spanned};

use crate::lock::{lock_dependency, lock_numbers};
use crate::names::{
    LIST_ITEMS, arm_name, enum_value_name, field_list_name, payload_name, variant_list_name,
};
use crate::shape::{FieldShape, vec_inner};

/// Returns `EncodeError::Unnumbered` for the member `ty.member`.
fn unnumbered(ty: &str, member: &str) -> TokenStream {
    quote!(return Err(::protobufify::__private::EncodeError::Unnumbered { ty: #ty, member: #member }))
}

/// The pattern binding (`<field>: field_<i>`), the encoding statement and the length
/// expression of each named field; `owner` names the message in the lock file. Statements and
/// lengths use the bindings (references to the fields) and write to `buf`.
struct EncodedFields {
    bindings: Vec<TokenStream>,
    encodes: Vec<TokenStream>,
    lens: Vec<TokenStream>,
}

fn encoded_fields(
    owner: &str,
    fields: &Fields,
    numbers: &HashMap<String, u32>,
) -> syn::Result<EncodedFields> {
    let Fields::Named(fields) = fields else {
        return Err(Error::new(
            fields.span(),
            "ProtoEncode: expected named fields",
        ));
    };
    let proto = quote!(::protobufify::__private);
    let mut out = EncodedFields {
        bindings: vec![],
        encodes: vec![],
        lens: vec![],
    };
    for (i, field) in fields.named.iter().enumerate() {
        let ident = field.ident.as_ref().expect("named field");
        let name = ident.unraw().to_string();
        let local = format_ident!("field_{i}");
        out.bindings.push(quote!(#ident: #local));
        let number = numbers.get(&format!("{owner}.{name}"));
        let (encode, len) = match FieldShape::of(&field.ty) {
            FieldShape::OptionalList(item) => {
                let wrapper = field_list_name(owner, &name);
                match (number, numbers.get(&format!("{wrapper}.{LIST_ITEMS}"))) {
                    (Some(n), Some(items)) => (
                        quote! {
                            if let Some(values) = #local {
                                #proto::encode_list::<#item>(values, #items, #n, buf)?;
                            }
                        },
                        quote!(#local.as_ref().map_or(0, |values| #proto::list_len::<#item>(values, #items, #n))),
                    ),
                    (n, _) => {
                        let error = match n {
                            None => unnumbered(owner, &name),
                            Some(_) => unnumbered(&wrapper, LIST_ITEMS),
                        };
                        (quote!(if #local.is_some() { #error; }), quote!(0))
                    }
                }
            }
            FieldShape::Optional(inner) => match number {
                Some(n) => (
                    quote! {
                        if let Some(value) = #local {
                            <#inner as #proto::ProtoEncode>::encode_field(value, #n, buf)?;
                        }
                    },
                    quote!(#local.as_ref().map_or(0, |value| <#inner as #proto::ProtoEncode>::encoded_field_len(value, #n))),
                ),
                None => {
                    let error = unnumbered(owner, &name);
                    (quote!(if #local.is_some() { #error; }), quote!(0))
                }
            },
            FieldShape::Repeated(item) => match number {
                Some(n) => (
                    quote!(<#item as #proto::ProtoEncode>::encode_repeated(#local, #n, buf)?;),
                    quote!(<#item as #proto::ProtoEncode>::encoded_repeated_len(#local, #n)),
                ),
                None => {
                    let error = unnumbered(owner, &name);
                    (quote!(if !#local.is_empty() { #error; }), quote!(0))
                }
            },
            FieldShape::Plain(ty) => match number {
                Some(n) => (
                    quote! {
                        if !<#ty as #proto::ProtoEncode>::is_default(#local) {
                            <#ty as #proto::ProtoEncode>::encode_field(#local, #n, buf)?;
                        }
                    },
                    quote! {
                        if <#ty as #proto::ProtoEncode>::is_default(#local) {
                            0
                        } else {
                            <#ty as #proto::ProtoEncode>::encoded_field_len(#local, #n)
                        }
                    },
                ),
                None => {
                    let error = unnumbered(owner, &name);
                    (
                        quote!(if !<#ty as #proto::ProtoEncode>::is_default(#local) { #error; }),
                        quote!(0),
                    )
                }
            },
        };
        out.encodes.push(encode);
        out.lens.push(len);
    }
    Ok(out)
}

/// `ProtoEncode` of a message type, encoded by its `ProtoEncodeMessage` `encode` and `len`
/// bodies.
fn encode_message_impl(input: &DeriveInput, encode: TokenStream, len: TokenStream) -> TokenStream {
    let ident = &input.ident;
    let proto = quote!(::protobufify::__private);
    let lock_dependency = lock_dependency();
    quote! {
        impl #proto::ProtoEncode for #ident {
            fn encode_field(
                &self,
                number: u32,
                buf: &mut impl #proto::BufMut,
            ) -> Result<(), #proto::EncodeError> {
                #proto::encode_message_field(self, number, buf)
            }

            fn encoded_field_len(&self, number: u32) -> usize {
                #proto::message_field_len(self, number)
            }
        }

        impl #proto::ProtoEncodeMessage for #ident {
            fn encode_message(
                &self,
                buf: &mut impl #proto::BufMut,
            ) -> Result<(), #proto::EncodeError> {
                #lock_dependency
                #encode
            }

            fn message_len(&self) -> usize {
                #len
            }
        }
    }
}

pub(crate) fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
    if !input.generics.params.is_empty() {
        return Err(Error::new(
            input.generics.span(),
            "ProtoEncode: generic types are not supported",
        ));
    }
    let numbers = lock_numbers();
    let ident = &input.ident;
    let name = ident.to_string();
    let proto = quote!(::protobufify::__private);
    match &input.data {
        Data::Struct(data) => {
            let EncodedFields {
                bindings,
                encodes,
                lens,
            } = encoded_fields(&name, &data.fields, &numbers)?;
            let encode = quote! {
                let Self { #(#bindings,)* } = self;
                #(#encodes)*
                Ok(())
            };
            let len = quote! {
                let Self { #(#bindings,)* } = self;
                0 #(+ #lens)*
            };
            Ok(encode_message_impl(input, encode, len))
        }
        Data::Enum(data) if data.variants.iter().all(|v| v.fields.is_empty()) => {
            let mut arms = vec![];
            for variant in &data.variants {
                let v = &variant.ident;
                let value = enum_value_name(&name, v);
                arms.push(match numbers.get(&format!("{name}.{value}")) {
                    Some(n) => {
                        let n = *n as i32;
                        quote!(Self::#v { .. } => #n)
                    }
                    None => {
                        let error = unnumbered(&name, &value);
                        quote!(Self::#v { .. } => #error)
                    }
                });
            }
            let to_number = quote! {
                |value: &Self| -> Result<i32, #proto::EncodeError> {
                    Ok(match *value {
                        #(#arms,)*
                    })
                }
            };
            let lock_dependency = lock_dependency();
            Ok(quote! {
                impl #proto::ProtoEncode for #ident {
                    fn encode_field(
                        &self,
                        number: u32,
                        buf: &mut impl #proto::BufMut,
                    ) -> Result<(), #proto::EncodeError> {
                        #lock_dependency
                        <i32 as #proto::ProtoEncode>::encode_field(&(#to_number)(self)?, number, buf)
                    }

                    fn encoded_field_len(&self, number: u32) -> usize {
                        (#to_number)(self)
                            .map_or(0, |n| <i32 as #proto::ProtoEncode>::encoded_field_len(&n, number))
                    }

                    fn encode_repeated(
                        values: &[Self],
                        number: u32,
                        buf: &mut impl #proto::BufMut,
                    ) -> Result<(), #proto::EncodeError> {
                        let numbers = values
                            .iter()
                            .map(#to_number)
                            .collect::<Result<#proto::Vec<i32>, _>>()?;
                        <i32 as #proto::ProtoEncode>::encode_repeated(&numbers, number, buf)
                    }

                    fn encoded_repeated_len(values: &[Self], number: u32) -> usize {
                        let numbers: #proto::Vec<i32> = values
                            .iter()
                            .filter_map(|value| (#to_number)(value).ok())
                            .collect();
                        <i32 as #proto::ProtoEncode>::encoded_repeated_len(&numbers, number)
                    }
                }
            })
        }
        Data::Enum(data) => {
            let mut encode_arms = vec![];
            let mut len_arms = vec![];
            for variant in &data.variants {
                let v = &variant.ident;
                if let Fields::Unnamed(f) = &variant.fields
                    && f.unnamed.len() != 1
                {
                    return Err(Error::new(
                        f.span(),
                        "ProtoEncode: tuple variants need exactly one field",
                    ));
                }
                let arm = arm_name(v);
                let Some(n) = numbers.get(&format!("{name}.{arm}")) else {
                    let error = unnumbered(&name, &arm);
                    encode_arms.push(quote!(Self::#v { .. } => #error));
                    len_arms.push(quote!(Self::#v { .. } => 0));
                    continue;
                };
                match &variant.fields {
                    // A `bool` arm: being set selects the variant.
                    Fields::Unit => {
                        encode_arms.push(quote! {
                            Self::#v => <bool as #proto::ProtoEncode>::encode_field(&true, #n, buf)?
                        });
                        len_arms.push(quote! {
                            Self::#v => <bool as #proto::ProtoEncode>::encoded_field_len(&true, #n)
                        });
                    }
                    // The `<Enum><Variant>` payload message, encoded like a struct.
                    Fields::Named(_) => {
                        let EncodedFields {
                            bindings,
                            encodes,
                            lens,
                        } = encoded_fields(&payload_name(&name, v), &variant.fields, &numbers)?;
                        encode_arms.push(quote! {
                            Self::#v { #(#bindings,)* } => {
                                #proto::encode_length_delimiter(#n, 0 #(+ #lens)*, buf);
                                #(#encodes)*
                            }
                        });
                        len_arms.push(quote! {
                            Self::#v { #(#bindings,)* } => #proto::length_delimited_len(#n, 0 #(+ #lens)*)
                        });
                    }
                    Fields::Unnamed(f) => {
                        let inner = &f.unnamed[0].ty;
                        match vec_inner(inner) {
                            Some(item) => {
                                let wrapper = variant_list_name(&name, v);
                                match numbers.get(&format!("{wrapper}.{LIST_ITEMS}")) {
                                    Some(items) => {
                                        encode_arms.push(quote! {
                                            Self::#v(values) => #proto::encode_list::<#item>(values, #items, #n, buf)?
                                        });
                                        len_arms.push(quote! {
                                            Self::#v(values) => #proto::list_len::<#item>(values, #items, #n)
                                        });
                                    }
                                    None => {
                                        let error = unnumbered(&wrapper, LIST_ITEMS);
                                        encode_arms.push(quote!(Self::#v(_) => #error));
                                        len_arms.push(quote!(Self::#v(_) => 0));
                                    }
                                }
                            }
                            // Written even if it is a default: a set oneof arm is always present.
                            None => {
                                encode_arms.push(quote! {
                                    Self::#v(value) => <#inner as #proto::ProtoEncode>::encode_field(value, #n, buf)?
                                });
                                len_arms.push(quote! {
                                    Self::#v(value) => <#inner as #proto::ProtoEncode>::encoded_field_len(value, #n)
                                });
                            }
                        }
                    }
                }
            }
            let encode = quote! {
                match self {
                    #(#encode_arms,)*
                }
                Ok(())
            };
            let len = quote! {
                match self {
                    #(#len_arms,)*
                }
            };
            Ok(encode_message_impl(input, encode, len))
        }
        Data::Union(_) => Err(Error::new(
            input.span(),
            "ProtoEncode: unions are not supported",
        )),
    }
}
