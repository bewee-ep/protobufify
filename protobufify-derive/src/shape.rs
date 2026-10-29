//! How a field type maps to protobuf, recognized syntactically (`Option`, `Vec`).

use syn::{DataEnum, Error, Fields, GenericArgument, PathArguments, Type, spanned::Spanned};

fn single_argument<'a>(args: &'a PathArguments, ty: &Type) -> syn::Result<&'a Type> {
    if let PathArguments::AngleBracketed(args) = args
        && args.args.len() == 1
        && let Some(GenericArgument::Type(inner)) = args.args.first()
    {
        return Ok(inner);
    }
    Err(Error::new(ty.span(), "expected exactly one type argument"))
}

/// `T` if `ty` is `Option<T>`.
fn option_inner(ty: &Type) -> Option<&Type> {
    match ty {
        Type::Group(g) => option_inner(&g.elem),
        Type::Paren(p) => option_inner(&p.elem),
        Type::Path(p) if p.qself.is_none() => {
            let segment = p.path.segments.last()?;
            if segment.ident != "Option" {
                return None;
            }
            single_argument(&segment.arguments, ty).ok()
        }
        _ => None,
    }
}

/// `T` if `ty` is `Vec<T>`.
pub(crate) fn vec_inner(ty: &Type) -> Option<&Type> {
    match ty {
        Type::Group(g) => vec_inner(&g.elem),
        Type::Paren(p) => vec_inner(&p.elem),
        Type::Path(p) if p.qself.is_none() => {
            let segment = p.path.segments.last()?;
            if segment.ident != "Vec" {
                return None;
            }
            single_argument(&segment.arguments, ty).ok()
        }
        _ => None,
    }
}

/// How a field maps to protobuf, by the syntactic shape of its type.
pub(crate) enum FieldShape<'a> {
    /// `Option<Vec<T>>`: an `optional` `…List` wrapper message around a `repeated T items`.
    OptionalList(&'a Type),
    /// `Option<T>`: `optional T`.
    Optional(&'a Type),
    /// `Vec<T>`: `repeated T`.
    Repeated(&'a Type),
    /// Anything else: a plain field.
    Plain(&'a Type),
}

impl<'a> FieldShape<'a> {
    pub(crate) fn of(ty: &'a Type) -> Self {
        match option_inner(ty) {
            Some(inner) => match vec_inner(inner) {
                Some(item) => Self::OptionalList(item),
                None => Self::Optional(inner),
            },
            None => match vec_inner(ty) {
                Some(item) => Self::Repeated(item),
                None => Self::Plain(ty),
            },
        }
    }
}

/// Whether the enum has only unit variants, which makes it a proto enum rather than a message
/// with a `oneof`.
pub(crate) fn is_fieldless(data: &DataEnum) -> bool {
    data.variants
        .iter()
        .all(|v| matches!(v.fields, Fields::Unit))
}
