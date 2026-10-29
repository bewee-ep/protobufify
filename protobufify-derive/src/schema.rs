//! `#[derive(ProtoSchema)]`.

use heck::ToShoutySnakeCase;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Data, DeriveInput, Error, Fields, Type, ext::IdentExt, spanned::Spanned};

use crate::attrs::{Reserved, container};
use crate::names::{
    LIST_ITEMS, arm_name, enum_value_name, field_list_name, payload_name, variant_list_name,
};
use crate::numbers::{
    Number, NumberedVariant, ValueNumbers, arm_numbers, field_numbers, value_numbers,
};
use crate::shape::{FieldShape, is_fieldless, vec_inner};

/// The doc comment as one string, without the leading space of each line.
fn doc_string(attrs: &[Attribute]) -> String {
    attrs
        .iter()
        .filter(|a| a.path().is_ident("doc"))
        .filter_map(|a| match &a.meta {
            syn::Meta::NameValue(nv) => match &nv.value {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) => Some(s.value()),
                _ => None,
            },
            _ => None,
        })
        .map(|line| line.strip_prefix(' ').unwrap_or(&line).to_string())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// A `Definition` expression of `kind`.
fn definition(doc: &str, kind: TokenStream, reserved: &Reserved) -> TokenStream {
    let Reserved { numbers, names } = reserved;
    quote! {
        ::protobufify::Definition {
            doc: #doc,
            kind: #kind,
            reserved: &[#(#numbers),*],
            reserved_names: &[#(#names),*],
        }
    }
}

fn proto_type_of(ty: &Type) -> TokenStream {
    quote!(<#ty as ::protobufify::ProtoSchema>::proto_type())
}

fn define_of(ty: &Type) -> TokenStream {
    quote!(<#ty as ::protobufify::ProtoSchema>::define(defs);)
}

/// Inserts the `<name>` wrapper message around a `repeated items` field of `item`.
fn list_wrapper(name: &str, item: &Type) -> TokenStream {
    let item_type = proto_type_of(item);
    let kind = quote! {
        ::protobufify::Kind::Message(::protobufify::__private::vec![::protobufify::Field {
            name: #LIST_ITEMS,
            label: ::protobufify::Label::Repeated,
            ty: #item_type,
            doc: "",
            number: ::protobufify::__private::LIST_ITEMS_NUMBER,
        }])
    };
    let definition = definition("", kind, &Reserved::default());
    quote!(defs.insert(#name, #definition);)
}

/// `Field` expressions of named fields, plus the statements defining what they reference;
/// `owner` names the message they belong to, which reserves `reserved`.
fn proto_fields(
    owner: &str,
    fields: &Fields,
    reserved: &Reserved,
) -> syn::Result<(Vec<TokenStream>, Vec<TokenStream>)> {
    let Fields::Named(fields) = fields else {
        return Err(Error::new(
            fields.span(),
            "ProtoSchema: expected named fields",
        ));
    };
    let mut exprs = vec![];
    let mut defines = vec![];
    for (field, number) in field_numbers(owner, fields, reserved)? {
        let ident = field.ident.as_ref().expect("named field").unraw();
        let name = ident.to_string();
        let doc = doc_string(&field.attrs);
        let (label, ty) = match FieldShape::of(&field.ty) {
            FieldShape::OptionalList(item) => {
                let wrapper = field_list_name(owner, &name);
                defines.push(list_wrapper(&wrapper, item));
                defines.push(define_of(item));
                (
                    quote!(Optional),
                    quote!(::protobufify::ProtoType::Named(#wrapper)),
                )
            }
            FieldShape::Optional(inner) => {
                defines.push(define_of(inner));
                (quote!(Optional), proto_type_of(inner))
            }
            FieldShape::Repeated(item) => {
                defines.push(define_of(item));
                (quote!(Repeated), proto_type_of(item))
            }
            FieldShape::Plain(ty) => {
                defines.push(define_of(ty));
                (quote!(Plain), proto_type_of(ty))
            }
        };
        exprs.push(quote! {
            ::protobufify::Field {
                name: #name,
                label: ::protobufify::Label::#label,
                ty: #ty,
                doc: #doc,
                number: #number,
            }
        });
    }
    Ok((exprs, defines))
}

/// The `ProtoSchema` impl for `input`, defining it as `kind` reserving `reserved`, before
/// running `defines`.
fn proto_impl(
    input: &DeriveInput,
    kind: TokenStream,
    reserved: &Reserved,
    defines: &[TokenStream],
) -> TokenStream {
    let ident = &input.ident;
    let name = ident.to_string();
    let definition = definition(&doc_string(&input.attrs), kind, reserved);
    quote! {
        impl ::protobufify::ProtoSchema for #ident {
            fn proto_type() -> ::protobufify::ProtoType {
                ::protobufify::ProtoType::Named(#name)
            }

            fn define(defs: &mut ::protobufify::ProtoDefs) {
                if !defs.insert(#name, #definition) {
                    return;
                }
                #(#defines)*
            }
        }
    }
}

pub(crate) fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
    if !input.generics.params.is_empty() {
        return Err(Error::new(
            input.generics.span(),
            "ProtoSchema: generic types are not supported",
        ));
    }
    let container = container(input)?;
    let reserved = &container.reserved;
    let name = input.ident.to_string();
    match &input.data {
        Data::Struct(data) => {
            let (fields, defines) = proto_fields(&name, &data.fields, reserved)?;
            let kind = quote!(::protobufify::Kind::Message(
                ::protobufify::__private::vec![#(#fields),*]
            ));
            Ok(proto_impl(input, kind, reserved, &defines))
        }
        Data::Enum(data) if is_fieldless(data) => {
            let ident = &input.ident;
            let ValueNumbers {
                values: numbers,
                checks,
            } = value_numbers(ident, data, container.numbers_from_discriminants, reserved)?;
            let unspecified = format!("{}_UNSPECIFIED", name.to_shouty_snake_case());
            let mut values =
                vec![quote!(::protobufify::EnumValue { name: #unspecified, doc: "", number: 0 })];
            for (variant, number) in numbers {
                let v = &variant.ident;
                let value = enum_value_name(&name, v);
                let doc = doc_string(&variant.attrs);
                let number = match number {
                    Number::Explicit(n) => quote!(#n),
                    Number::Discriminant => quote!(#ident::#v as u32),
                };
                values.push(
                    quote!(::protobufify::EnumValue { name: #value, doc: #doc, number: #number }),
                );
            }
            let kind = quote!(::protobufify::Kind::Enum(
                ::protobufify::__private::vec![#(#values),*]
            ));
            let mut tokens = proto_impl(input, kind, reserved, &[]);
            tokens.extend(checks);
            Ok(tokens)
        }
        Data::Enum(data) => {
            let mut arms = vec![];
            let mut defines = vec![];
            for numbered in arm_numbers(&name, data, reserved)? {
                let NumberedVariant {
                    variant,
                    number,
                    payload_reserved,
                } = numbered;
                let arm = arm_name(&variant.ident);
                let doc = doc_string(&variant.attrs);
                let ty = match &variant.fields {
                    Fields::Unit => quote!(None),
                    Fields::Unnamed(f) if f.unnamed.len() == 1 => {
                        let inner = &f.unnamed[0].ty;
                        match vec_inner(inner) {
                            Some(item) => {
                                let wrapper = variant_list_name(&name, &variant.ident);
                                defines.push(list_wrapper(&wrapper, item));
                                defines.push(define_of(item));
                                quote!(Some(::protobufify::ProtoType::Named(#wrapper)))
                            }
                            None => {
                                defines.push(define_of(inner));
                                let ty = proto_type_of(inner);
                                quote!(Some(#ty))
                            }
                        }
                    }
                    Fields::Named(_) => {
                        let payload = payload_name(&name, &variant.ident);
                        let (fields, payload_defines) =
                            proto_fields(&payload, &variant.fields, &payload_reserved)?;
                        let kind = quote! {
                            ::protobufify::Kind::Message(::protobufify::__private::vec![#(#fields),*])
                        };
                        let definition = definition("", kind, &payload_reserved);
                        defines.push(quote!(defs.insert(#payload, #definition);));
                        defines.extend(payload_defines);
                        quote!(Some(::protobufify::ProtoType::Named(#payload)))
                    }
                    Fields::Unnamed(f) => {
                        return Err(Error::new(
                            f.span(),
                            "ProtoSchema: tuple variants need exactly one field",
                        ));
                    }
                };
                arms.push(
                    quote!(::protobufify::Arm { name: #arm, ty: #ty, doc: #doc, number: #number }),
                );
            }
            let kind = quote!(::protobufify::Kind::Oneof(
                ::protobufify::__private::vec![#(#arms),*]
            ));
            Ok(proto_impl(input, kind, reserved, &defines))
        }
        Data::Union(_) => Err(Error::new(
            input.span(),
            "ProtoSchema: unions are not supported",
        )),
    }
}
