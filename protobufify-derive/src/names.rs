//! The proto names of the export, shared by `ProtoSchema` (which defines them) and
//! `ProtoDecode`/`ProtoEncode` (which name them in errors).

use heck::{ToShoutySnakeCase, ToSnakeCase, ToUpperCamelCase};
use syn::Ident;

/// The wrapper message of an `Option<Vec<T>>` field: `<Owner><Field>List`.
pub(crate) fn field_list_name(owner: &str, field: &str) -> String {
    format!("{owner}{}List", field.to_upper_camel_case())
}

/// The wrapper message of a `Vec<T>` variant payload: `<Enum><Variant>List`.
pub(crate) fn variant_list_name(enum_name: &str, variant: &Ident) -> String {
    format!("{enum_name}{variant}List")
}

/// The payload message of a struct variant: `<Enum><Variant>`.
pub(crate) fn payload_name(enum_name: &str, variant: &Ident) -> String {
    format!("{enum_name}{variant}")
}

/// An enum value: `<ENUM>_<VARIANT>`.
pub(crate) fn enum_value_name(enum_name: &str, variant: &Ident) -> String {
    format!(
        "{}_{}",
        enum_name.to_shouty_snake_case(),
        variant.to_string().to_shouty_snake_case()
    )
}

/// A oneof arm: the variant in snake case.
pub(crate) fn arm_name(variant: &Ident) -> String {
    variant.to_string().to_snake_case()
}

/// The single field of a list wrapper message.
pub(crate) const LIST_ITEMS: &str = "items";
