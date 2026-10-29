//! `#[derive(ProtoDecode)]`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Error, Fields, ext::IdentExt, spanned::Spanned};

use crate::attrs::{Reserved, container};
use crate::names::payload_name;
use crate::numbers::{
    Number, NumberedVariant, ValueNumbers, arm_numbers, field_numbers, value_numbers,
};
use crate::shape::{FieldShape, is_fieldless, vec_inner};

/// The local, the decoding match arm and the struct field initializer of each named field of
/// the message `owner`, which reserves `reserved`.
struct DecodedFields {
    locals: Vec<TokenStream>,
    arms: Vec<TokenStream>,
    inits: Vec<TokenStream>,
}

fn decoded_fields(owner: &str, fields: &Fields, reserved: &Reserved) -> syn::Result<DecodedFields> {
    let Fields::Named(fields) = fields else {
        return Err(Error::new(
            fields.span(),
            "ProtoDecode: expected named fields",
        ));
    };
    let proto = quote!(::protobufify::__private);
    let mut out = DecodedFields {
        locals: vec![],
        arms: vec![],
        inits: vec![],
    };
    for (i, (field, n)) in field_numbers(owner, fields, reserved)?
        .into_iter()
        .enumerate()
    {
        let ident = field.ident.as_ref().expect("named field");
        let name = ident.unraw().to_string();
        let local = format_ident!("field_{i}");
        let ty = &field.ty;
        match FieldShape::of(ty) {
            FieldShape::OptionalList(item) => {
                out.locals.push(quote!(let mut #local: #ty = None;));
                out.arms.push(quote! {
                    #n => #proto::merge_list::<#item>(&mut #local, wire_type, buf, ctx.clone())?
                });
                out.inits.push(quote!(#ident: #local));
            }
            FieldShape::Optional(inner) => {
                out.locals.push(quote!(let mut #local: #ty = None;));
                out.arms.push(quote! {
                    #n => <#inner as #proto::ProtoDecode>::merge(&mut #local, wire_type, buf, ctx.clone())?
                });
                out.inits.push(quote!(#ident: #local));
            }
            FieldShape::Repeated(item) => {
                out.locals
                    .push(quote!(let mut #local: #ty = ::protobufify::__private::Vec::new();));
                out.arms.push(quote! {
                    #n => <#item as #proto::ProtoDecode>::merge_repeated(&mut #local, wire_type, buf, ctx.clone())?
                });
                out.inits.push(quote!(#ident: #local));
            }
            FieldShape::Plain(_) => {
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
    Ok(out)
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

/// `ProtoDecode` of a message type, decoded by its `ProtoDecodeMessage::decode_message` `body`.
fn message_impl(input: &DeriveInput, body: TokenStream) -> TokenStream {
    let ident = &input.ident;
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

pub(crate) fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
    if !input.generics.params.is_empty() {
        return Err(Error::new(
            input.generics.span(),
            "ProtoDecode: generic types are not supported",
        ));
    }
    let container = container(input)?;
    let reserved = &container.reserved;
    let ident = &input.ident;
    let name = ident.to_string();
    let proto = quote!(::protobufify::__private);
    match &input.data {
        Data::Struct(data) => {
            let DecodedFields {
                locals,
                arms,
                inits,
            } = decoded_fields(&name, &data.fields, reserved)?;
            let decode = decode_loop(&arms);
            let body = quote! {
                #(#locals)*
                #decode
                Ok(Self { #(#inits,)* })
            };
            Ok(message_impl(input, body))
        }
        Data::Enum(data) if is_fieldless(data) => {
            let ValueNumbers { values, checks } =
                value_numbers(ident, data, container.numbers_from_discriminants, reserved)?;
            let arms = values.into_iter().map(|(variant, number)| {
                let v = &variant.ident;
                match number {
                    Number::Discriminant => quote!(n if n == Self::#v as i32 => Self::#v),
                    // In `1..=i32::MAX`, checked when numbering.
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
            Ok(quote! {
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
            })
        }
        Data::Enum(data) => {
            let mut arms = vec![];
            for numbered in arm_numbers(&name, data, reserved)? {
                let NumberedVariant {
                    variant,
                    number: n,
                    payload_reserved,
                } = numbered;
                let v = &variant.ident;
                let f = match &variant.fields {
                    // A `bool` arm: being set selects the variant, whatever its value.
                    Fields::Unit => {
                        arms.push(quote! {
                            #n => {
                                let mut flag = None;
                                <bool as #proto::ProtoDecode>::merge(&mut flag, wire_type, buf, ctx.clone())?;
                                value = Some(Self::#v);
                            }
                        });
                        continue;
                    }
                    // The `<Enum><Variant>` payload message, decoded like a struct.
                    Fields::Named(_) => {
                        let DecodedFields {
                            locals,
                            arms: payload_arms,
                            inits,
                        } = decoded_fields(
                            &payload_name(&name, v),
                            &variant.fields,
                            &payload_reserved,
                        )?;
                        let decode = decode_loop(&payload_arms);
                        arms.push(quote! {
                            #n => {
                                let mut payload = #proto::length_delimited(wire_type, buf)?;
                                let buf = &mut payload;
                                #(#locals)*
                                #decode
                                value = Some(Self::#v { #(#inits,)* });
                            }
                        });
                        continue;
                    }
                    Fields::Unnamed(f) if f.unnamed.len() == 1 => f,
                    Fields::Unnamed(f) => {
                        return Err(Error::new(
                            f.span(),
                            "ProtoDecode: tuple variants need exactly one field",
                        ));
                    }
                };
                let inner = &f.unnamed[0].ty;
                let merge = match vec_inner(inner) {
                    Some(item) => {
                        quote!(#proto::merge_list::<#item>(&mut payload, wire_type, buf, ctx.clone())?)
                    }
                    None => {
                        quote!(<#inner as #proto::ProtoDecode>::merge(&mut payload, wire_type, buf, ctx.clone())?)
                    }
                };
                arms.push(quote! {
                    #n => {
                        let mut payload = None;
                        #merge;
                        value = payload.map(Self::#v);
                    }
                });
            }
            let decode = decode_loop(&arms);
            let body = quote! {
                let mut value: Option<Self> = None;
                #decode
                value.ok_or(#proto::ProtoError::Missing { ty: #name, field: "value" })
            };
            Ok(message_impl(input, body))
        }
        Data::Union(_) => Err(Error::new(
            input.span(),
            "ProtoDecode: unions are not supported",
        )),
    }
}
