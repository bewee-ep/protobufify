//! `#[derive(ProtoSchema)]`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Type};

use crate::model::{
    Body, Model, Number, Payload, Reserved, ResolvedArm, ResolvedField, ResolvedValue, Shape,
};
use crate::names::{LIST_ITEMS, unspecified_name};

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

/// The `Kind::Message` expression of `fields`, plus the statements defining what they reference.
fn message(fields: &[ResolvedField]) -> (TokenStream, Vec<TokenStream>) {
    let mut exprs = vec![];
    let mut defines = vec![];
    for ResolvedField {
        field,
        name,
        number,
        shape,
    } in fields
    {
        let doc = doc_string(&field.attrs);
        let (label, ty) = match shape {
            Shape::OptionalList { item, wrapper } => {
                defines.push(list_wrapper(wrapper, item));
                defines.push(define_of(item));
                (
                    quote!(Optional),
                    quote!(::protobufify::ProtoType::Named(#wrapper)),
                )
            }
            Shape::Optional(inner) => {
                defines.push(define_of(inner));
                (quote!(Optional), proto_type_of(inner))
            }
            Shape::Repeated(item) => {
                defines.push(define_of(item));
                (quote!(Repeated), proto_type_of(item))
            }
            Shape::Plain(ty) => {
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
    let kind = quote!(::protobufify::Kind::Message(
        ::protobufify::__private::vec![#(#exprs),*]
    ));
    (kind, defines)
}

/// The `Kind::Enum` expression of the `values` of the enum `model`.
fn enumeration(model: &Model, values: &[ResolvedValue]) -> TokenStream {
    let ident = &model.input.ident;
    let unspecified = unspecified_name(&model.name);
    let mut exprs =
        vec![quote!(::protobufify::EnumValue { name: #unspecified, doc: "", number: 0 })];
    for ResolvedValue {
        variant,
        name,
        number,
    } in values
    {
        let v = &variant.ident;
        let doc = doc_string(&variant.attrs);
        let number = match number {
            Number::Explicit(n) => quote!(#n),
            Number::Discriminant => quote!(#ident::#v as u32),
        };
        exprs.push(quote!(::protobufify::EnumValue { name: #name, doc: #doc, number: #number }));
    }
    quote!(::protobufify::Kind::Enum(
        ::protobufify::__private::vec![#(#exprs),*]
    ))
}

/// The `Kind::Oneof` expression of `arms`, plus the statements defining what they reference.
fn oneof(arms: &[ResolvedArm]) -> (TokenStream, Vec<TokenStream>) {
    let mut exprs = vec![];
    let mut defines = vec![];
    for ResolvedArm {
        variant,
        name,
        number,
        payload,
    } in arms
    {
        let doc = doc_string(&variant.attrs);
        let ty = match payload {
            Payload::Unit => quote!(None),
            Payload::Single(ty) => {
                defines.push(define_of(ty));
                let ty = proto_type_of(ty);
                quote!(Some(#ty))
            }
            Payload::List { item, wrapper } => {
                defines.push(list_wrapper(wrapper, item));
                defines.push(define_of(item));
                quote!(Some(::protobufify::ProtoType::Named(#wrapper)))
            }
            Payload::Message {
                name: payload,
                fields,
                reserved,
            } => {
                let (kind, payload_defines) = message(fields);
                let definition = definition("", kind, reserved);
                defines.push(quote!(defs.insert(#payload, #definition);));
                defines.extend(payload_defines);
                quote!(Some(::protobufify::ProtoType::Named(#payload)))
            }
        };
        exprs.push(quote!(::protobufify::Arm { name: #name, ty: #ty, doc: #doc, number: #number }));
    }
    let kind = quote!(::protobufify::Kind::Oneof(
        ::protobufify::__private::vec![#(#exprs),*]
    ));
    (kind, defines)
}

pub(crate) fn expand(model: &Model) -> TokenStream {
    let (kind, defines, checks) = match &model.body {
        Body::Message(fields) => {
            let (kind, defines) = message(fields);
            (kind, defines, quote!())
        }
        Body::Enum { values, checks } => (enumeration(model, values), vec![], checks.clone()),
        Body::Oneof(arms) => {
            let (kind, defines) = oneof(arms);
            (kind, defines, quote!())
        }
    };
    let ident = &model.input.ident;
    let name = &model.name;
    let definition = definition(&doc_string(&model.input.attrs), kind, &model.reserved);
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

        #checks
    }
}
