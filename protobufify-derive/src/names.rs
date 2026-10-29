//! The proto names of the export: those the types and members derive from their Rust
//! identifiers, converted to the protobuf style guide, and those of the messages generated from
//! them, which follow the proto names (also when given by `#[protobufify(name = "…")]`).

use heck::{ToShoutySnakeCase, ToSnakeCase, ToUpperCamelCase};
use syn::{Ident, ext::IdentExt};

/// What a proto name is, for error messages.
pub(crate) const PROTO_NAME: &str = "an ASCII letter, then ASCII letters, digits or `_`";

/// Whether `name` is a proto identifier: [`PROTO_NAME`].
pub(crate) fn is_proto_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A message or enum: the type in upper camel case.
pub(crate) fn type_name(ident: &Ident) -> String {
    ident.unraw().to_string().to_upper_camel_case()
}

/// A message field: the field in snake case.
pub(crate) fn field_name(ident: &Ident) -> String {
    ident.unraw().to_string().to_snake_case()
}

/// The JSON name of a field in protobuf's JSON format, as protoc derives it: each `_` dropped
/// and the letter after it upper-cased (`page_count` → `pageCount`). proto3 rejects two fields of
/// a message with the same JSON name.
pub(crate) fn json_name(name: &str) -> String {
    let mut json = String::with_capacity(name.len());
    let mut upper = false;
    for c in name.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            json.push(c.to_ascii_uppercase());
            upper = false;
        } else {
            json.push(c);
        }
    }
    json
}

/// The wrapper message of an `Option<Vec<T>>` field: `<Owner><Field>List`.
pub(crate) fn field_list_name(owner: &str, field: &str) -> String {
    format!("{owner}{}List", field.to_upper_camel_case())
}

/// The wrapper message of a `Vec<T>` variant payload: `<Enum><Arm>List`.
pub(crate) fn variant_list_name(enum_name: &str, arm: &str) -> String {
    format!("{enum_name}{}List", arm.to_upper_camel_case())
}

/// The payload message of a struct variant: `<Enum><Arm>`.
pub(crate) fn payload_name(enum_name: &str, arm: &str) -> String {
    format!("{enum_name}{}", arm.to_upper_camel_case())
}

/// An enum value: `<ENUM>_<VARIANT>`, in upper snake case.
pub(crate) fn enum_value_name(enum_name: &str, variant: &Ident) -> String {
    format!(
        "{}_{}",
        enum_name.to_shouty_snake_case(),
        variant.unraw().to_string().to_shouty_snake_case()
    )
}

/// The zero value of an enum: `<ENUM>_UNSPECIFIED`.
pub(crate) fn unspecified_name(enum_name: &str) -> String {
    format!("{}_UNSPECIFIED", enum_name.to_shouty_snake_case())
}

/// A oneof arm, which is a field: the variant in snake case.
pub(crate) fn arm_name(variant: &Ident) -> String {
    variant.unraw().to_string().to_snake_case()
}

/// The single field of a list wrapper message.
pub(crate) const LIST_ITEMS: &str = "items";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_proto_name_returns_true_for_ascii_identifiers_only() {
        // given:
        let names = [
            "isbn",
            "GENRE_NOVEL",
            "a1_b",
            "type",
            "4",
            "4th",
            "_a",
            "is bn",
            "",
            "größe",
        ];

        // when:
        let valid: Vec<_> = names.iter().map(|n| is_proto_name(n)).collect();

        // then:
        assert_eq!(
            valid,
            [
                true, true, true, true, false, false, false, false, false, false
            ]
        );
    }

    #[test]
    fn json_name_returns_name_without_underscores_and_with_the_letters_after_them_upper_cased() {
        // given:
        let names = ["page_count", "pageCount", "foo_1", "foo1", "a__b", "title"];

        // when:
        let json: Vec<_> = names.iter().map(|n| json_name(n)).collect();

        // then:
        assert_eq!(
            json,
            ["pageCount", "pageCount", "foo1", "foo1", "aB", "title"]
        );
    }
}
