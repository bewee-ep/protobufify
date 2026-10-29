//! The `#[protobufify(...)]` helper attribute, parsed: which keys are where, with their literals
//! as given. What the values mean, and whether they are valid, is checked in `model`.
//!
//! On a type: `name = "…"`, its proto name; `reserved(N, "name", …)`, the numbers and names of
//! removed members; and, on an enum without fields, `numbers_from_discriminants`. On a field or
//! variant: `number = N` and `name = "…"`; on a struct variant also `reserved`, for its payload
//! message.

use syn::{
    Attribute, Data, DeriveInput, Error, Lit, LitInt, LitStr, Token, meta::ParseNestedMeta,
    punctuated::Punctuated,
};

use crate::errors::Errors;
use crate::shape::is_fieldless;

const ATTRIBUTE: &str = "protobufify";

/// An entry of `reserved(…)`.
pub(crate) enum ReservedItem {
    Number(LitInt),
    Name(LitStr),
}

/// The attribute on a type.
#[derive(Default)]
pub(crate) struct TypeAttrs {
    pub(crate) name: Option<LitStr>,
    pub(crate) numbers_from_discriminants: bool,
    pub(crate) reserved: Vec<ReservedItem>,
}

/// The attribute on a field or variant.
#[derive(Default)]
pub(crate) struct MemberAttrs {
    pub(crate) number: Option<LitInt>,
    pub(crate) name: Option<LitStr>,
    /// For a struct variant, the reserved numbers and names of its payload message.
    pub(crate) reserved: Vec<ReservedItem>,
    /// Whether an attribute failed to parse, so that keys may be missing.
    pub(crate) broken: bool,
}

/// Where an attribute is, which fixes the keys it takes.
#[derive(Clone, Copy)]
enum Place {
    /// On a type; `fieldless_enum` for an enum without fields.
    Type { fieldless_enum: bool },
    /// On a field or variant; `payload` for a struct variant.
    Member { payload: bool },
}

impl Place {
    fn expected(self) -> String {
        let keys = match self {
            Self::Type {
                fieldless_enum: true,
            } => "`name`, `reserved`, `numbers_from_discriminants`",
            Self::Type { .. } => "`name`, `reserved`",
            Self::Member { payload: true } => "`number`, `name`, `reserved`",
            Self::Member { .. } => "`number`, `name`",
        };
        format!("protobufify: expected {keys}")
    }
}

/// Everything an attribute can give; what a place does not take stays empty.
#[derive(Default)]
struct Parsed {
    number: Option<LitInt>,
    name: Option<LitStr>,
    numbers_from_discriminants: bool,
    reserved: Vec<ReservedItem>,
    broken: bool,
}

/// The entries of `reserved(…)`.
fn reserved(meta: &ParseNestedMeta) -> syn::Result<Vec<ReservedItem>> {
    let content;
    syn::parenthesized!(content in meta.input);
    Punctuated::<Lit, Token![,]>::parse_terminated(&content)?
        .into_iter()
        .map(|lit| match lit {
            Lit::Int(lit) => Ok(ReservedItem::Number(lit)),
            Lit::Str(lit) => Ok(ReservedItem::Name(lit)),
            lit => Err(Error::new(
                lit.span(),
                "protobufify: expected a number or a \"name\"",
            )),
        })
        .collect()
}

/// The attributes `attrs`, which are at `place`. An attribute that fails to parse is reported to
/// `errors`, keeping the keys parsed before the failure.
fn parse(attrs: &[Attribute], place: Place, errors: &mut Errors) -> Parsed {
    let mut out = Parsed::default();
    for attr in attrs.iter().filter(|a| a.path().is_ident(ATTRIBUTE)) {
        let result = attr.parse_nested_meta(|meta| {
            let twice = |key: &str| meta.error(format!("protobufify: `{key}` given twice"));
            let Some(key) = meta.path.get_ident().map(ToString::to_string) else {
                return Err(meta.error(place.expected()));
            };
            match (key.as_str(), place) {
                ("number", Place::Member { .. }) => {
                    let lit: LitInt = meta.value()?.parse()?;
                    if out.number.is_some() {
                        return Err(twice("number"));
                    }
                    out.number = Some(lit);
                }
                ("name", _) => {
                    let lit: LitStr = meta.value()?.parse()?;
                    if out.name.is_some() {
                        return Err(twice("name"));
                    }
                    out.name = Some(lit);
                }
                (
                    "numbers_from_discriminants",
                    Place::Type {
                        fieldless_enum: true,
                    },
                ) => {
                    if out.numbers_from_discriminants {
                        return Err(twice("numbers_from_discriminants"));
                    }
                    out.numbers_from_discriminants = true;
                }
                ("numbers_from_discriminants", Place::Type { .. }) => {
                    return Err(meta.error(
                        "protobufify: only enums without fields can be numbered by their \
                         discriminants",
                    ));
                }
                ("reserved", Place::Type { .. } | Place::Member { payload: true }) => {
                    out.reserved.extend(reserved(&meta)?);
                }
                ("reserved", Place::Member { payload: false }) => {
                    return Err(meta.error(
                        "protobufify: on a variant, `reserved` is that of the payload message of \
                         a struct variant; reserve arms on the enum",
                    ));
                }
                _ => return Err(meta.error(place.expected())),
            }
            Ok(())
        });
        if let Err(error) = result {
            errors.push(error);
            out.broken = true;
        }
    }
    out
}

/// The attribute on the type `input`.
pub(crate) fn type_attrs(input: &DeriveInput, errors: &mut Errors) -> TypeAttrs {
    let fieldless_enum = matches!(&input.data, Data::Enum(data) if is_fieldless(data));
    let parsed = parse(&input.attrs, Place::Type { fieldless_enum }, errors);
    TypeAttrs {
        name: parsed.name,
        numbers_from_discriminants: parsed.numbers_from_discriminants,
        reserved: parsed.reserved,
    }
}

/// The attribute on a field, or on a variant (`payload` if it is a struct variant, whose payload
/// message takes `reserved`).
pub(crate) fn member_attrs(attrs: &[Attribute], payload: bool, errors: &mut Errors) -> MemberAttrs {
    let parsed = parse(attrs, Place::Member { payload }, errors);
    MemberAttrs {
        number: parsed.number,
        name: parsed.name,
        reserved: parsed.reserved,
        broken: parsed.broken,
    }
}

/// An error if `attrs` has the attribute: the field of a tuple variant takes none, the variant
/// does.
pub(crate) fn reject(attrs: &[Attribute], errors: &mut Errors) {
    if let Some(attr) = attrs.iter().find(|a| a.path().is_ident(ATTRIBUTE)) {
        errors.push(Error::new_spanned(
            attr,
            "protobufify: number and name the variant, not its field",
        ));
    }
}
