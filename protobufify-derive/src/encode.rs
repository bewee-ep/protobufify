//! `#[derive(ProtoEncode)]`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::model::{
    Body, Model, Number, Payload, ResolvedArm, ResolvedField, ResolvedValue, Shape,
};

/// The pattern binding (`<field>: field_<i>`), the encoding statement and the length
/// expression of each named field. Statements and lengths use the bindings (references to the
/// fields) and write to `buf`.
struct EncodedFields {
    bindings: Vec<TokenStream>,
    encodes: Vec<TokenStream>,
    lens: Vec<TokenStream>,
}

fn encoded_fields(fields: &[ResolvedField]) -> EncodedFields {
    let proto = quote!(::protobufify::__private);
    let mut out = EncodedFields {
        bindings: vec![],
        encodes: vec![],
        lens: vec![],
    };
    for (i, resolved) in fields.iter().enumerate() {
        let ResolvedField {
            field,
            number: n,
            shape,
            ..
        } = resolved;
        let ident = field.ident.as_ref().expect("named field");
        let local = format_ident!("field_{i}");
        out.bindings.push(quote!(#ident: #local));
        let (encode, len) = match shape {
            Shape::OptionalList { item, .. } => (
                quote! {
                    if let Some(values) = #local {
                        #proto::encode_list::<#item>(values, #n, buf);
                    }
                },
                quote!(#local.as_ref().map_or(0, |values| #proto::list_len::<#item>(values, #n))),
            ),
            Shape::Optional(inner) => (
                quote! {
                    if let Some(value) = #local {
                        <#inner as #proto::ProtoEncode>::encode_field(value, #n, buf);
                    }
                },
                quote!(#local.as_ref().map_or(0, |value| <#inner as #proto::ProtoEncode>::encoded_field_len(value, #n))),
            ),
            Shape::Repeated(item) => (
                quote!(<#item as #proto::ProtoEncode>::encode_repeated(#local, #n, buf);),
                quote!(<#item as #proto::ProtoEncode>::encoded_repeated_len(#local, #n)),
            ),
            Shape::Plain(ty) => (
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
    out
}

/// `ProtoEncode` of the message type `model`, encoded by its `ProtoEncodeMessage` `encode` and
/// `len` bodies.
fn message_impl(model: &Model, encode: TokenStream, len: TokenStream) -> TokenStream {
    let ident = &model.input.ident;
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

/// `ProtoEncode` of the enum `model` of `values`.
fn enum_impl(model: &Model, values: &[ResolvedValue], checks: &TokenStream) -> TokenStream {
    let ident = &model.input.ident;
    let proto = quote!(::protobufify::__private);
    let arms = values.iter().map(|value| {
        let v = &value.variant.ident;
        match value.number {
            Number::Discriminant => quote!(Self::#v => Self::#v as i32),
            // In `1..=i32::MAX`, checked when analyzed.
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
    quote! {
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
    }
}

/// The `encode_message` and `message_len` bodies of the enum with fields of `arms`.
fn oneof_bodies(arms: &[ResolvedArm]) -> (TokenStream, TokenStream) {
    let proto = quote!(::protobufify::__private);
    let mut encode_arms = vec![];
    let mut len_arms = vec![];
    for ResolvedArm {
        variant,
        number: n,
        payload,
        ..
    } in arms
    {
        let v = &variant.ident;
        match payload {
            // A `bool` arm: being set selects the variant.
            Payload::Unit => {
                encode_arms.push(quote! {
                    Self::#v => <bool as #proto::ProtoEncode>::encode_field(&true, #n, buf)
                });
                len_arms.push(quote! {
                    Self::#v => <bool as #proto::ProtoEncode>::encoded_field_len(&true, #n)
                });
            }
            // Written even if it is a default: a set oneof arm is always present.
            Payload::Single(inner) => {
                encode_arms.push(quote! {
                    Self::#v(value) => <#inner as #proto::ProtoEncode>::encode_field(value, #n, buf)
                });
                len_arms.push(quote! {
                    Self::#v(value) => <#inner as #proto::ProtoEncode>::encoded_field_len(value, #n)
                });
            }
            Payload::List { item, .. } => {
                encode_arms.push(quote! {
                    Self::#v(values) => #proto::encode_list::<#item>(values, #n, buf)
                });
                len_arms.push(quote! {
                    Self::#v(values) => #proto::list_len::<#item>(values, #n)
                });
            }
            // The payload message, encoded like a struct.
            Payload::Message { fields, .. } => {
                let EncodedFields {
                    bindings,
                    encodes,
                    lens,
                } = encoded_fields(fields);
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
    (encode, len)
}

pub(crate) fn expand(model: &Model) -> TokenStream {
    match &model.body {
        Body::Message(fields) => {
            let EncodedFields {
                bindings,
                encodes,
                lens,
            } = encoded_fields(fields);
            let encode = quote! {
                let Self { #(#bindings,)* } = self;
                #(#encodes)*
            };
            let len = quote! {
                let Self { #(#bindings,)* } = self;
                0 #(+ #lens)*
            };
            message_impl(model, encode, len)
        }
        Body::Enum { values, checks } => enum_impl(model, values, checks),
        Body::Oneof(arms) => {
            let (encode, len) = oneof_bodies(arms);
            message_impl(model, encode, len)
        }
    }
}
