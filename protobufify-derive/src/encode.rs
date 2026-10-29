//! `#[derive(ProtoEncode)]`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Error, Fields, spanned::Spanned};

use crate::attrs::{Reserved, container};
use crate::names::payload_name;
use crate::numbers::{
    Number, NumberedVariant, ValueNumbers, arm_numbers, field_numbers, value_numbers,
};
use crate::shape::{FieldShape, is_fieldless, vec_inner};

/// The pattern binding (`<field>: field_<i>`), the encoding statement and the length
/// expression of each named field of the message `owner`, which reserves `reserved`. Statements
/// and lengths use the bindings (references to the fields) and write to `buf`.
struct EncodedFields {
    bindings: Vec<TokenStream>,
    encodes: Vec<TokenStream>,
    lens: Vec<TokenStream>,
}

fn encoded_fields(owner: &str, fields: &Fields, reserved: &Reserved) -> syn::Result<EncodedFields> {
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
    for (i, (field, n)) in field_numbers(owner, fields, reserved)?
        .into_iter()
        .enumerate()
    {
        let ident = field.ident.as_ref().expect("named field");
        let local = format_ident!("field_{i}");
        out.bindings.push(quote!(#ident: #local));
        let (encode, len) = match FieldShape::of(&field.ty) {
            FieldShape::OptionalList(item) => (
                quote! {
                    if let Some(values) = #local {
                        #proto::encode_list::<#item>(values, #n, buf);
                    }
                },
                quote!(#local.as_ref().map_or(0, |values| #proto::list_len::<#item>(values, #n))),
            ),
            FieldShape::Optional(inner) => (
                quote! {
                    if let Some(value) = #local {
                        <#inner as #proto::ProtoEncode>::encode_field(value, #n, buf);
                    }
                },
                quote!(#local.as_ref().map_or(0, |value| <#inner as #proto::ProtoEncode>::encoded_field_len(value, #n))),
            ),
            FieldShape::Repeated(item) => (
                quote!(<#item as #proto::ProtoEncode>::encode_repeated(#local, #n, buf);),
                quote!(<#item as #proto::ProtoEncode>::encoded_repeated_len(#local, #n)),
            ),
            FieldShape::Plain(ty) => (
                quote! {
                    if !<#ty as #proto::ProtoEncode>::is_default(#local) {
                        <#ty as #proto::ProtoEncode>::encode_field(#local, #n, buf);
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
    quote! {
        impl #proto::ProtoEncode for #ident {
            fn encode_field(&self, number: u32, buf: &mut impl #proto::BufMut) {
                #proto::encode_message_field(self, number, buf)
            }

            fn encoded_field_len(&self, number: u32) -> usize {
                #proto::message_field_len(self, number)
            }
        }

        impl #proto::ProtoEncodeMessage for #ident {
            fn encode_message(&self, buf: &mut impl #proto::BufMut) {
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
    let container = container(input)?;
    let reserved = &container.reserved;
    let ident = &input.ident;
    let name = ident.to_string();
    let proto = quote!(::protobufify::__private);
    match &input.data {
        Data::Struct(data) => {
            let EncodedFields {
                bindings,
                encodes,
                lens,
            } = encoded_fields(&name, &data.fields, reserved)?;
            let encode = quote! {
                let Self { #(#bindings,)* } = self;
                #(#encodes)*
            };
            let len = quote! {
                let Self { #(#bindings,)* } = self;
                0 #(+ #lens)*
            };
            Ok(encode_message_impl(input, encode, len))
        }
        Data::Enum(data) if is_fieldless(data) => {
            let ValueNumbers { values, checks } =
                value_numbers(ident, data, container.numbers_from_discriminants, reserved)?;
            let arms = values.into_iter().map(|(variant, number)| {
                let v = &variant.ident;
                match number {
                    Number::Discriminant => quote!(Self::#v => Self::#v as i32),
                    // In `1..=i32::MAX`, checked when numbering.
                    Number::Explicit(n) => {
                        let n = n as i32;
                        quote!(Self::#v => #n)
                    }
                }
            });
            let to_number = quote! {
                |value: &Self| -> i32 {
                    match *value {
                        #(#arms,)*
                    }
                }
            };
            Ok(quote! {
                impl #proto::ProtoEncode for #ident {
                    fn encode_field(&self, number: u32, buf: &mut impl #proto::BufMut) {
                        <i32 as #proto::ProtoEncode>::encode_field(&(#to_number)(self), number, buf)
                    }

                    fn encoded_field_len(&self, number: u32) -> usize {
                        <i32 as #proto::ProtoEncode>::encoded_field_len(&(#to_number)(self), number)
                    }

                    fn encode_repeated(values: &[Self], number: u32, buf: &mut impl #proto::BufMut) {
                        let numbers: #proto::Vec<i32> = values.iter().map(#to_number).collect();
                        <i32 as #proto::ProtoEncode>::encode_repeated(&numbers, number, buf)
                    }

                    fn encoded_repeated_len(values: &[Self], number: u32) -> usize {
                        let numbers: #proto::Vec<i32> = values.iter().map(#to_number).collect();
                        <i32 as #proto::ProtoEncode>::encoded_repeated_len(&numbers, number)
                    }
                }

                #checks
            })
        }
        Data::Enum(data) => {
            let mut encode_arms = vec![];
            let mut len_arms = vec![];
            for numbered in arm_numbers(&name, data, reserved)? {
                let NumberedVariant {
                    variant,
                    number: n,
                    payload_reserved,
                } = numbered;
                let v = &variant.ident;
                match &variant.fields {
                    // A `bool` arm: being set selects the variant.
                    Fields::Unit => {
                        encode_arms.push(quote! {
                            Self::#v => <bool as #proto::ProtoEncode>::encode_field(&true, #n, buf)
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
                        } = encoded_fields(
                            &payload_name(&name, v),
                            &variant.fields,
                            &payload_reserved,
                        )?;
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
                    Fields::Unnamed(f) if f.unnamed.len() == 1 => {
                        let inner = &f.unnamed[0].ty;
                        match vec_inner(inner) {
                            Some(item) => {
                                encode_arms.push(quote! {
                                    Self::#v(values) => #proto::encode_list::<#item>(values, #n, buf)
                                });
                                len_arms.push(quote! {
                                    Self::#v(values) => #proto::list_len::<#item>(values, #n)
                                });
                            }
                            // Written even if it is a default: a set oneof arm is always present.
                            None => {
                                encode_arms.push(quote! {
                                    Self::#v(value) => <#inner as #proto::ProtoEncode>::encode_field(value, #n, buf)
                                });
                                len_arms.push(quote! {
                                    Self::#v(value) => <#inner as #proto::ProtoEncode>::encoded_field_len(value, #n)
                                });
                            }
                        }
                    }
                    Fields::Unnamed(f) => {
                        return Err(Error::new(
                            f.span(),
                            "ProtoEncode: tuple variants need exactly one field",
                        ));
                    }
                }
            }
            let encode = quote! {
                match self {
                    #(#encode_arms,)*
                }
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
