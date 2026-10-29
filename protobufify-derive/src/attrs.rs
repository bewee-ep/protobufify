//! The `#[protobufify(...)]` helper attribute. On a type: `reserved(N, …)` and
//! `reserved_names("name", …)`, the numbers and names of removed members, and, on an enum
//! without fields, `numbers_from_discriminants`. On a field or variant: `number = N`; on a struct
//! variant also `reserved` and `reserved_names`, for its payload message.

use proc_macro2::Span;
use syn::{
    Attribute, Data, DeriveInput, Error, LitInt, LitStr, Token, meta::ParseNestedMeta,
    punctuated::Punctuated,
};

use crate::shape::is_fieldless;

const ATTRIBUTE: &str = "protobufify";

/// The valid numbers of a member, by what it is.
#[derive(Clone, Copy)]
pub(crate) enum Range {
    /// A message field or oneof arm: `1..=536_870_911`, except the reserved `19000..=19999`.
    Field,
    /// An enum value: `1..=i32::MAX`, `0` being the `…_UNSPECIFIED` value.
    EnumValue,
}

impl Range {
    fn allows(self, number: u32) -> bool {
        match self {
            Self::Field => {
                (1..=536_870_911).contains(&number) && !(19000..=19999).contains(&number)
            }
            Self::EnumValue => (1..=i32::MAX as u32).contains(&number),
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::Field => "a field number must be in 1..=536870911, except 19000..=19999",
            Self::EnumValue => {
                "an enum value number must be in 1..=2147483647, 0 is the `…_UNSPECIFIED` value"
            }
        }
    }

    /// The number `lit`, if in range.
    fn parse(self, lit: &LitInt) -> syn::Result<u32> {
        lit.base10_parse::<u32>()
            .ok()
            .filter(|n| self.allows(*n))
            .ok_or_else(|| Error::new(lit.span(), format!("protobufify: {}", self.message())))
    }
}

/// The numbers and names of a type's removed members, which no member may take.
#[derive(Default)]
pub(crate) struct Reserved {
    pub(crate) numbers: Vec<u32>,
    pub(crate) names: Vec<String>,
}

/// What the attribute gives.
#[derive(Default)]
struct Attrs {
    number: Option<(u32, Span)>,
    numbers_from_discriminants: bool,
    reserved: Reserved,
}

/// The keys an attribute takes where it is.
#[derive(Clone, Copy, Default)]
struct Keys {
    /// `number = N`, in this range.
    number: Option<Range>,
    numbers_from_discriminants: bool,
    /// `reserved(…)` and `reserved_names(…)`, of numbers in this range.
    reserved: Option<Range>,
}

impl Keys {
    fn expected(self) -> String {
        let keys: Vec<_> = [
            (self.number.is_some(), "`number = N`"),
            (
                self.numbers_from_discriminants,
                "`numbers_from_discriminants`",
            ),
            (self.reserved.is_some(), "`reserved(N, …)`"),
            (self.reserved.is_some(), "`reserved_names(\"name\", …)`"),
        ]
        .into_iter()
        .filter_map(|(allowed, key)| allowed.then_some(key))
        .collect();
        format!("protobufify: expected {}", keys.join(", "))
    }
}

/// The comma-separated literals in the parentheses of `meta`.
fn list<T: syn::parse::Parse>(meta: &ParseNestedMeta) -> syn::Result<Punctuated<T, Token![,]>> {
    let content;
    syn::parenthesized!(content in meta.input);
    Punctuated::parse_terminated(&content)
}

/// The attributes `attrs`, which take `keys`.
fn parse(attrs: &[Attribute], keys: Keys) -> syn::Result<Attrs> {
    let mut out = Attrs::default();
    for attr in attrs.iter().filter(|a| a.path().is_ident(ATTRIBUTE)) {
        attr.parse_nested_meta(|meta| {
            let twice = |key: &str| meta.error(format!("protobufify: `{key}` given twice"));
            let path = &meta.path;
            if let Some(range) = keys.number.filter(|_| path.is_ident("number")) {
                let lit: LitInt = meta.value()?.parse()?;
                if out.number.is_some() {
                    return Err(twice("number"));
                }
                out.number = Some((range.parse(&lit)?, lit.span()));
            } else if keys.numbers_from_discriminants && path.is_ident("numbers_from_discriminants")
            {
                if out.numbers_from_discriminants {
                    return Err(twice("numbers_from_discriminants"));
                }
                out.numbers_from_discriminants = true;
            } else if let Some(range) = keys.reserved.filter(|_| path.is_ident("reserved")) {
                for lit in list::<LitInt>(&meta)? {
                    out.reserved.numbers.push(range.parse(&lit)?);
                }
            } else if keys.reserved.is_some() && path.is_ident("reserved_names") {
                let names = list::<LitStr>(&meta)?;
                out.reserved.names.extend(names.iter().map(LitStr::value));
            } else if keys.reserved.is_some() && path.is_ident("numbers_from_discriminants") {
                return Err(meta.error(
                    "protobufify: only enums without fields can be numbered by their discriminants",
                ));
            } else {
                return Err(meta.error(keys.expected()));
            }
            Ok(())
        })?;
    }
    Ok(out)
}

/// The attribute on the type itself.
pub(crate) struct Container {
    /// `numbers_from_discriminants`: the values of an enum without fields are numbered by their
    /// discriminants (unless numbered by `#[protobufify(number = N)]`).
    pub(crate) numbers_from_discriminants: bool,
    pub(crate) reserved: Reserved,
}

/// The attribute on the type `input`.
pub(crate) fn container(input: &DeriveInput) -> syn::Result<Container> {
    let fieldless_enum = matches!(&input.data, Data::Enum(data) if is_fieldless(data));
    let range = if fieldless_enum {
        Range::EnumValue
    } else {
        Range::Field
    };
    let keys = Keys {
        numbers_from_discriminants: fieldless_enum,
        reserved: Some(range),
        ..Keys::default()
    };
    let attrs = parse(&input.attrs, keys)?;
    Ok(Container {
        numbers_from_discriminants: attrs.numbers_from_discriminants,
        reserved: attrs.reserved,
    })
}

/// The attribute on a field, or on a variant (`payload` if it is a struct variant, whose payload
/// message takes `reserved` and `reserved_names`): the number of the member in `range`, with the
/// span of `N`, and the reserved ones of the payload message.
pub(crate) fn member(
    attrs: &[Attribute],
    range: Range,
    payload: bool,
) -> syn::Result<(Option<(u32, Span)>, Reserved)> {
    let keys = Keys {
        number: Some(range),
        reserved: payload.then_some(Range::Field),
        ..Keys::default()
    };
    let attrs = parse(attrs, keys)?;
    Ok((attrs.number, attrs.reserved))
}

/// An error if `attrs` has the attribute: the field of a tuple variant takes none, the variant
/// does.
pub(crate) fn reject(attrs: &[Attribute]) -> syn::Result<()> {
    match attrs.iter().find(|a| a.path().is_ident(ATTRIBUTE)) {
        Some(attr) => Err(Error::new_spanned(
            attr,
            "protobufify: number the variant, not its field",
        )),
        None => Ok(()),
    }
}
