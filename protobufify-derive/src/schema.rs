//! `#[derive(ProtoSchema)]`.

use heck::ToShoutySnakeCase;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Data, DeriveInput, Error, Fields, Type, ext::IdentExt, spanned::Spanned};

use crate::names::{
    LIST_ITEMS, arm_name, enum_value_name, field_list_name, payload_name, variant_list_name,
};
use crate::shape::{FieldShape, vec_inner};

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

fn proto_type_of(ty: &Type) -> TokenStream {
    quote!(<#ty as ::protobufify::ProtoSchema>::proto_type())
}

fn define_of(ty: &Type) -> TokenStream {
    quote!(<#ty as ::protobufify::ProtoSchema>::define(defs);)
}

/// Inserts the `<name>` wrapper message around a `repeated items` field of `item`.
fn list_wrapper(name: &str, item: &Type) -> TokenStream {
    let item_type = proto_type_of(item);
    quote! {
        defs.insert(#name, ::protobufify::Definition {
            doc: "",
            kind: ::protobufify::Kind::Message(::protobufify::__private::vec![::protobufify::Field {
                name: #LIST_ITEMS,
                label: ::protobufify::Label::Repeated,
                ty: #item_type,
                doc: "",
            }]),
        });
    }
}

/// `Field` expressions of named fields, plus the statements defining what they reference;
/// `owner` names the message they belong to.
fn proto_fields(owner: &str, fields: &Fields) -> syn::Result<(Vec<TokenStream>, Vec<TokenStream>)> {
    let Fields::Named(fields) = fields else {
        return Err(Error::new(
            fields.span(),
            "ProtoSchema: expected named fields",
        ));
    };
    let mut exprs = vec![];
    let mut defines = vec![];
    for field in &fields.named {
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
            }
        });
    }
    Ok((exprs, defines))
}

/// The `ProtoSchema` impl for `input`, defining it as `kind` before running `defines`.
fn proto_impl(input: &DeriveInput, kind: TokenStream, defines: &[TokenStream]) -> TokenStream {
    let ident = &input.ident;
    let name = ident.to_string();
    let doc = doc_string(&input.attrs);
    quote! {
        impl ::protobufify::ProtoSchema for #ident {
            fn proto_type() -> ::protobufify::ProtoType {
                ::protobufify::ProtoType::Named(#name)
            }

            fn define(defs: &mut ::protobufify::ProtoDefs) {
                let definition = ::protobufify::Definition { doc: #doc, kind: #kind };
                if !defs.insert(#name, definition) {
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
    let name = input.ident.to_string();
    match &input.data {
        Data::Struct(data) => {
            let (fields, defines) = proto_fields(&name, &data.fields)?;
            let kind = quote!(::protobufify::Kind::Message(
                ::protobufify::__private::vec![#(#fields),*]
            ));
            Ok(proto_impl(input, kind, &defines))
        }
        Data::Enum(data) if data.variants.iter().all(|v| v.fields.is_empty()) => {
            let unspecified = format!("{}_UNSPECIFIED", name.to_shouty_snake_case());
            let mut values = vec![quote!(::protobufify::EnumValue { name: #unspecified, doc: "" })];
            for variant in &data.variants {
                let value = enum_value_name(&name, &variant.ident);
                let doc = doc_string(&variant.attrs);
                values.push(quote!(::protobufify::EnumValue { name: #value, doc: #doc }));
            }
            let kind = quote!(::protobufify::Kind::Enum(
                ::protobufify::__private::vec![#(#values),*]
            ));
            Ok(proto_impl(input, kind, &[]))
        }
        Data::Enum(data) => {
            let mut arms = vec![];
            let mut defines = vec![];
            for variant in &data.variants {
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
                        let (fields, payload_defines) = proto_fields(&payload, &variant.fields)?;
                        defines.push(quote! {
                            defs.insert(#payload, ::protobufify::Definition {
                                doc: "",
                                kind: ::protobufify::Kind::Message(::protobufify::__private::vec![#(#fields),*]),
                            });
                        });
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
                arms.push(quote!(::protobufify::Arm { name: #arm, ty: #ty, doc: #doc }));
            }
            let kind = quote!(::protobufify::Kind::Oneof(
                ::protobufify::__private::vec![#(#arms),*]
            ));
            Ok(proto_impl(input, kind, &defines))
        }
        Data::Union(_) => Err(Error::new(
            input.span(),
            "ProtoSchema: unions are not supported",
        )),
    }
}
