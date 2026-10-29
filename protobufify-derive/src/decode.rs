//! `#[derive(ProtoDecode)]`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::model::{
    Body, Model, Number, Payload, ResolvedArm, ResolvedField, ResolvedValue, Shape,
};

/// The local, the decoding match arm and the struct field initializer of each named field of
/// the message `owner`.
struct DecodedFields {
    locals: Vec<TokenStream>,
    arms: Vec<TokenStream>,
    inits: Vec<TokenStream>,
}

fn decoded_fields(owner: &str, fields: &[ResolvedField]) -> DecodedFields {
    let proto = quote!(::protobufify::__private);
    let mut out = DecodedFields {
        locals: vec![],
        arms: vec![],
        inits: vec![],
    };
    for (i, resolved) in fields.iter().enumerate() {
        let ResolvedField {
            field,
            name,
            number: n,
            shape,
        } = resolved;
        let ident = field.ident.as_ref().expect("named field");
        let local = format_ident!("field_{i}");
        let ty = &field.ty;
        match shape {
            Shape::OptionalList { item, .. } => {
                out.locals.push(quote!(let mut #local: #ty = None;));
                out.arms.push(quote! {
                    #n => #proto::merge_list::<#item>(&mut #local, wire_type, buf, ctx.clone())?
                });
                out.inits.push(quote!(#ident: #local));
            }
            Shape::Optional(inner) => {
                out.locals.push(quote!(let mut #local: #ty = None;));
                out.arms.push(quote! {
                    #n => <#inner as #proto::ProtoDecode>::merge(&mut #local, wire_type, buf, ctx.clone())?
                });
                out.inits.push(quote!(#ident: #local));
            }
            Shape::Repeated(item) => {
                out.locals
                    .push(quote!(let mut #local: #ty = ::protobufify::__private::Vec::new();));
                out.arms.push(quote! {
                    #n => <#item as #proto::ProtoDecode>::merge_repeated(&mut #local, wire_type, buf, ctx.clone())?
                });
                out.inits.push(quote!(#ident: #local));
            }
            Shape::Plain(_) => {
                out.locals.push(quote!(let mut #local: Option<#ty> = None;));
                out.arms.push(quote! {
                    #n => <#ty as #proto::ProtoDecode>::merge(&mut #local, wire_type, buf, ctx.clone())?
                });
                out.inits.push(quote! {
                    #ident: match #local {
                        Some(value) => value,
                        None => <#ty as #proto::ProtoDecode>::absent()
                            .ok_or(#proto::ProtoError::Missing { ty: #owner, field: #name })?,
                    }
                });
            }
        }
    }
    out
}

/// The loop decoding the fields of a message from `buf` with `arms`, skipping unknown numbers.
fn decode_loop(arms: &[TokenStream]) -> TokenStream {
    quote! {
        while ::protobufify::__private::Buf::has_remaining(buf) {
            let (number, wire_type) = ::protobufify::__private::decode_key(buf)?;
            match number {
                #(#arms,)*
                _ => ::protobufify::__private::skip_field(wire_type, number, buf, ctx.clone())?,
            }
        }
    }
}

/// `ProtoDecode` of the message type `model`, decoded by its
/// `ProtoDecodeMessage::decode_message` `body`.
fn message_impl(model: &Model, body: TokenStream) -> TokenStream {
    let ident = &model.input.ident;
    let proto = quote!(::protobufify::__private);
    quote! {
        impl #proto::ProtoDecode for #ident {
            fn merge(
                value: &mut Option<Self>,
                wire_type: #proto::WireType,
                buf: &mut #proto::Bytes,
                ctx: #proto::DecodeContext,
            ) -> Result<(), #proto::ProtoError> {
                #proto::merge_message(value, wire_type, buf, ctx)
            }

            fn absent() -> Option<Self> {
                #proto::absent_message()
            }
        }

        impl #proto::ProtoDecodeMessage for #ident {
            #[allow(unused_mut, clippy::match_single_binding)]
            fn decode_message(
                buf: &mut #proto::Bytes,
                ctx: #proto::DecodeContext,
            ) -> Result<Self, #proto::ProtoError> {
                #body
            }
        }
    }
}

/// `ProtoDecode` of the enum `model` of `values`.
fn enum_impl(model: &Model, values: &[ResolvedValue], checks: &TokenStream) -> TokenStream {
    let ident = &model.input.ident;
    let name = &model.name;
    let proto = quote!(::protobufify::__private);
    let arms = values.iter().map(|value| {
        let v = &value.variant.ident;
        match value.number {
            Number::Discriminant => quote!(n if n == Self::#v as i32 => Self::#v),
            // In `1..=i32::MAX`, checked when analyzed.
            Number::Explicit(n) => {
                let n = n as i32;
                quote!(#n => Self::#v)
            }
        }
    });
    let from_number = quote! {
        |number: i32| -> Result<Self, #proto::ProtoError> {
            Ok(match number {
                #(#arms,)*
                number => return Err(#proto::ProtoError::UnknownEnumValue { ty: #name, number }),
            })
        }
    };
    quote! {
        impl #proto::ProtoDecode for #ident {
            fn merge(
                value: &mut Option<Self>,
                wire_type: #proto::WireType,
                buf: &mut #proto::Bytes,
                ctx: #proto::DecodeContext,
            ) -> Result<(), #proto::ProtoError> {
                let mut number = None;
                <i32 as #proto::ProtoDecode>::merge(&mut number, wire_type, buf, ctx)?;
                *value = number.map(#from_number).transpose()?;
                Ok(())
            }

            fn merge_repeated(
                values: &mut ::protobufify::__private::Vec<Self>,
                wire_type: #proto::WireType,
                buf: &mut #proto::Bytes,
                ctx: #proto::DecodeContext,
            ) -> Result<(), #proto::ProtoError> {
                let mut numbers = ::protobufify::__private::Vec::new();
                <i32 as #proto::ProtoDecode>::merge_repeated(&mut numbers, wire_type, buf, ctx)?;
                for number in numbers {
                    values.push((#from_number)(number)?);
                }
                Ok(())
            }

            fn absent() -> Option<Self> {
                None
            }
        }

        #checks
    }
}

/// The body of `decode_message` of the enum with fields `model` of `arms`.
fn oneof_body(model: &Model, arms: &[ResolvedArm]) -> TokenStream {
    let name = &model.name;
    let proto = quote!(::protobufify::__private);
    let arms = arms.iter().map(|arm| {
        let ResolvedArm {
            variant,
            number: n,
            payload,
            ..
        } = arm;
        let v = &variant.ident;
        match payload {
            // A `bool` arm: being set selects the variant, whatever its value.
            Payload::Unit => quote! {
                #n => {
                    let mut flag = None;
                    <bool as #proto::ProtoDecode>::merge(&mut flag, wire_type, buf, ctx.clone())?;
                    value = Some(Self::#v);
                }
            },
            Payload::Single(inner) => quote! {
                #n => {
                    let mut payload = None;
                    <#inner as #proto::ProtoDecode>::merge(&mut payload, wire_type, buf, ctx.clone())?;
                    value = payload.map(Self::#v);
                }
            },
            Payload::List { item, .. } => quote! {
                #n => {
                    let mut payload = None;
                    #proto::merge_list::<#item>(&mut payload, wire_type, buf, ctx.clone())?;
                    value = payload.map(Self::#v);
                }
            },
            // The payload message, decoded like a struct.
            Payload::Message {
                name: payload,
                fields,
                ..
            } => {
                let DecodedFields {
                    locals,
                    arms,
                    inits,
                } = decoded_fields(payload, fields);
                let decode = decode_loop(&arms);
                quote! {
                    #n => {
                        let mut payload = #proto::length_delimited(wire_type, buf)?;
                        let buf = &mut payload;
                        #(#locals)*
                        #decode
                        value = Some(Self::#v { #(#inits,)* });
                    }
                }
            }
        }
    });
    let decode = decode_loop(&arms.collect::<Vec<_>>());
    quote! {
        let mut value: Option<Self> = None;
        #decode
        value.ok_or(#proto::ProtoError::Missing { ty: #name, field: "value" })
    }
}

pub(crate) fn expand(model: &Model) -> TokenStream {
    match &model.body {
        Body::Message(fields) => {
            let DecodedFields {
                locals,
                arms,
                inits,
            } = decoded_fields(&model.name, fields);
            let decode = decode_loop(&arms);
            let body = quote! {
                #(#locals)*
                #decode
                Ok(Self { #(#inits,)* })
            };
            message_impl(model, body)
        }
        Body::Enum { values, checks } => enum_impl(model, values, checks),
        Body::Oneof(arms) => message_impl(model, oneof_body(model, arms)),
    }
}
