//! `#[derive(ProtoDecode)]`.

use std::collections::HashMap;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Error, Fields, ext::IdentExt, spanned::Spanned};

use crate::lock::{lock_dependency, lock_numbers};
use crate::names::{
    LIST_ITEMS, arm_name, enum_value_name, field_list_name, payload_name, variant_list_name,
};
use crate::shape::{FieldShape, vec_inner};

/// The local, the decoding match arm (if the member has a number) and the struct field
/// initializer of each named field; `owner` names the message in the lock file.
struct DecodedFields {
    locals: Vec<TokenStream>,
    arms: Vec<TokenStream>,
    inits: Vec<TokenStream>,
}

fn decoded_fields(
    owner: &str,
    fields: &Fields,
    numbers: &HashMap<String, u32>,
) -> syn::Result<DecodedFields> {
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
    for (i, field) in fields.named.iter().enumerate() {
        let ident = field.ident.as_ref().expect("named field");
        let name = ident.unraw().to_string();
        let local = format_ident!("field_{i}");
        let number = numbers.get(&format!("{owner}.{name}"));
        let ty = &field.ty;
        match FieldShape::of(ty) {
            FieldShape::OptionalList(item) => {
                let wrapper = field_list_name(owner, &name);
                out.locals.push(quote!(let mut #local: #ty = None;));
                if let (Some(n), Some(items)) =
                    (number, numbers.get(&format!("{wrapper}.{LIST_ITEMS}")))
                {
                    out.arms.push(quote! {
                        #n => #proto::merge_list::<#item>(&mut #local, #items, wire_type, buf, ctx.clone())?
                    });
                }
                out.inits.push(quote!(#ident: #local));
            }
            FieldShape::Optional(inner) => {
                out.locals.push(quote!(let mut #local: #ty = None;));
                if let Some(n) = number {
                    out.arms.push(quote! {
                        #n => <#inner as #proto::ProtoDecode>::merge(&mut #local, wire_type, buf, ctx.clone())?
                    });
                }
                out.inits.push(quote!(#ident: #local));
            }
            FieldShape::Repeated(item) => {
                out.locals
                    .push(quote!(let mut #local: #ty = ::protobufify::__private::Vec::new();));
                if let Some(n) = number {
                    out.arms.push(quote! {
                        #n => <#item as #proto::ProtoDecode>::merge_repeated(&mut #local, wire_type, buf, ctx.clone())?
                    });
                }
                out.inits.push(quote!(#ident: #local));
            }
            FieldShape::Plain(_) => {
                out.locals.push(quote!(let mut #local: Option<#ty> = None;));
                if let Some(n) = number {
                    out.arms.push(quote! {
                        #n => <#ty as #proto::ProtoDecode>::merge(&mut #local, wire_type, buf, ctx.clone())?
                    });
                }
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
fn message_impl(input: &DeriveInput, locked: bool, body: TokenStream) -> TokenStream {
    let ident = &input.ident;
    let proto = quote!(::protobufify::__private);
    let lock_dependency = lock_dependency();
    quote! {
        impl #proto::ProtoDecode for #ident {
            const LOCKED: bool = #locked;

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
                #lock_dependency
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
    let numbers = lock_numbers();
    let ident = &input.ident;
    let name = ident.to_string();
    let locked = numbers
        .keys()
        .any(|key| key.starts_with(&format!("{name}.")));
    let proto = quote!(::protobufify::__private);
    match &input.data {
        Data::Struct(data) => {
            let DecodedFields {
                locals,
                arms,
                inits,
            } = decoded_fields(&name, &data.fields, &numbers)?;
            let decode = decode_loop(&arms);
            let body = quote! {
                #(#locals)*
                #decode
                Ok(Self { #(#inits,)* })
            };
            Ok(message_impl(input, locked, body))
        }
        Data::Enum(data) if data.variants.iter().all(|v| v.fields.is_empty()) => {
            let mut arms = vec![];
            for variant in &data.variants {
                let v = &variant.ident;
                if let Some(n) = numbers.get(&format!("{name}.{}", enum_value_name(&name, v))) {
                    let n = *n as i32;
                    arms.push(quote!(#n => Self::#v));
                }
            }
            let from_number = quote! {
                |number: i32| -> Result<Self, #proto::ProtoError> {
                    Ok(match number {
                        #(#arms,)*
                        number => return Err(#proto::ProtoError::UnknownEnumValue { ty: #name, number }),
                    })
                }
            };
            let lock_dependency = lock_dependency();
            Ok(quote! {
                impl #proto::ProtoDecode for #ident {
                    const LOCKED: bool = #locked;

                    fn merge(
                        value: &mut Option<Self>,
                        wire_type: #proto::WireType,
                        buf: &mut #proto::Bytes,
                        ctx: #proto::DecodeContext,
                    ) -> Result<(), #proto::ProtoError> {
                        #lock_dependency
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
            })
        }
        Data::Enum(data) => {
            let mut arms = vec![];
            for variant in &data.variants {
                let v = &variant.ident;
                if let Fields::Unnamed(f) = &variant.fields
                    && f.unnamed.len() != 1
                {
                    return Err(Error::new(
                        f.span(),
                        "ProtoDecode: tuple variants need exactly one field",
                    ));
                }
                let Some(n) = numbers.get(&format!("{name}.{}", arm_name(v))) else {
                    continue;
                };
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
                        } = decoded_fields(&payload_name(&name, v), &variant.fields, &numbers)?;
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
                    Fields::Unnamed(f) => f,
                };
                let inner = &f.unnamed[0].ty;
                let merge = match vec_inner(inner) {
                    Some(item) => {
                        let wrapper = variant_list_name(&name, v);
                        let Some(items) = numbers.get(&format!("{wrapper}.{LIST_ITEMS}")) else {
                            continue;
                        };
                        quote!(#proto::merge_list::<#item>(&mut payload, #items, wire_type, buf, ctx.clone())?)
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
            Ok(message_impl(input, locked, body))
        }
        Data::Union(_) => Err(Error::new(
            input.span(),
            "ProtoDecode: unions are not supported",
        )),
    }
}
