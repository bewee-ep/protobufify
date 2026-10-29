//! Derive macros of [protobufify](https://docs.rs/protobufify): `ProtoSchema`, `ProtoDecode` and
//! `ProtoEncode`. Use them through the `protobufify` crate, which re-exports them.

mod attrs;
mod decode;
mod encode;
mod names;
mod numbers;
mod schema;
mod shape;

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

/// Implements `protobufify::ProtoSchema`: records the type's proto definition, and those of the
/// types it references, so that `protobufify::ProtoDefs::render` can write the `.proto` file.
///
/// Names are the Rust identifiers, and fields, variants and enum values keep their declaration
/// order. Field types are mapped syntactically (`Option`, `Vec`); the compiler checks that every
/// other field type implements `ProtoSchema`.
///
/// - Struct with named fields → message. `Option<T>` → `optional`, `Vec<T>` → `repeated`,
///   `Option<Vec<T>>` → `optional` `<Struct><Field>List` wrapper message (its `repeated items`
///   field is number 1), so that an absent list stays distinguishable from an empty one.
/// - Enum without fields → enum, prefixed with the enum name, plus an `…_UNSPECIFIED` zero value.
/// - Enum with fields → message with a `oneof value`, one arm per variant. Struct variants get a
///   payload message `<Enum><Variant>`, a `Vec<T>` payload a `<Enum><Variant>List` wrapper, unit
///   variants a `bool` arm.
///
/// Every member is numbered explicitly: `#[protobufify(number = N)]` on the field or variant,
/// else, for an enum without fields marked `#[protobufify(numbers_from_discriminants)]`, the
/// variant's discriminant. A member without a number is a compile error, and so are numbers out
/// of range (`1..=536870911` except `19000..=19999` for fields and arms, `1..=i32::MAX` for enum
/// values), given twice, or reserved. Discriminants are checked by emitted constants, as only the
/// compiler knows them.
///
/// `#[protobufify(reserved(N, …), reserved_names("name", …))]` on the type (on a struct variant,
/// for its payload message) reserves the numbers and names of removed members: no member may
/// take them, and the schema lists them as `reserved`.
#[proc_macro_derive(ProtoSchema, attributes(protobufify))]
pub fn derive_proto_schema(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match schema::expand(&input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Implements `protobufify::ProtoDecode` (and `ProtoDecodeMessage` for messages) on prost's wire-format
/// primitives: decodes the messages of the schema `ProtoSchema` exports.
///
/// The numbers are those of `ProtoSchema`, with the same checks.
///
/// - Struct with named fields → message. An absent `Option` field is `None`, an absent `Vec` empty,
///   an absent plain field the proto3 default (an enum or a oneof has none: an error).
/// - Enum without fields → enum; an unknown number is an error.
/// - Enum with fields → message with a `oneof value`. Tuple variants need exactly one field;
///   unit variants are a `bool` arm, struct variants a `<Enum><Variant>` payload message.
#[proc_macro_derive(ProtoDecode, attributes(protobufify))]
pub fn derive_proto_decode(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match decode::expand(&input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Implements `protobufify::ProtoEncode` (and `ProtoEncodeMessage` for messages) on prost's
/// wire-format primitives: encodes into the messages of the schema `ProtoSchema` exports, which
/// `ProtoDecode` decodes back.
///
/// The numbers are those of `ProtoSchema`, with the same checks.
///
/// - Struct with named fields → message, fields in declaration order. A plain scalar field is
///   omitted when it is the proto3 default; an `Option` field is written iff `Some`; a `Vec` is
///   packed for numbers and enums; enums and messages are always written.
/// - Enum without fields → enum, as the value's number.
/// - Enum with fields → message with a `oneof value` holding exactly the variant's arm: unit
///   variants as `bool` `true`, struct variants as the `<Enum><Variant>` payload message.
#[proc_macro_derive(ProtoEncode, attributes(protobufify))]
pub fn derive_proto_encode(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match encode::expand(&input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

#[cfg(test)]
mod tests {
    use syn::parse_quote;

    use super::*;

    /// The error messages of the derives for `input`, which all three report alike.
    fn errors(input: DeriveInput) -> Vec<String> {
        let messages = |result: syn::Result<proc_macro2::TokenStream>| -> Vec<String> {
            let error = result.expect_err("an error");
            error.into_iter().map(|e| e.to_string()).collect()
        };
        let schema = messages(schema::expand(&input));
        assert_eq!(messages(decode::expand(&input)), schema, "ProtoDecode");
        assert_eq!(messages(encode::expand(&input)), schema, "ProtoEncode");
        schema
    }

    #[test]
    fn derives_return_error_when_value_has_no_number_and_enum_does_not_number_by_discriminants() {
        // given:
        let input = parse_quote! {
            #[repr(u8)]
            enum Color { Red = 1 }
        };

        // when:
        let errors = errors(input);

        // then:
        assert_eq!(
            errors,
            ["protobufify: `Color.COLOR_RED` has no number: add `#[protobufify(number = N)]`"]
        );
    }

    #[test]
    fn derives_return_error_when_enum_value_number_is_out_of_range() {
        // given:
        let input = parse_quote! {
            enum Color { #[protobufify(number = 2147483648)] Red }
        };

        // when:
        let errors = errors(input);

        // then:
        assert_eq!(
            errors,
            [
                "protobufify: an enum value number must be in 1..=2147483647, 0 is the `…_UNSPECIFIED` value"
            ]
        );
    }

    #[test]
    fn derives_return_error_when_key_is_unknown() {
        // given:
        let input = parse_quote! {
            struct Note { #[protobufify(nr = 3)] text: String }
        };

        // when:
        let errors = errors(input);

        // then:
        assert_eq!(errors, ["protobufify: expected `number = N`"]);
    }

    #[test]
    fn derives_return_error_when_field_of_tuple_variant_is_numbered() {
        // given:
        let input = parse_quote! {
            enum Anchor { #[protobufify(number = 1)] Page(#[protobufify(number = 1)] u32) }
        };

        // when:
        let errors = errors(input);

        // then:
        assert_eq!(errors, ["protobufify: number the variant, not its field"]);
    }

    #[test]
    fn derives_return_error_when_numbers_from_discriminants_is_not_on_enum_without_fields() {
        // given:
        let inputs: [DeriveInput; 2] = [
            parse_quote! {
                #[protobufify(numbers_from_discriminants)]
                struct Note { #[protobufify(number = 1)] text: String }
            },
            parse_quote! {
                #[protobufify(numbers_from_discriminants)]
                enum Room { Hall() }
            },
        ];

        // when:
        let errors = inputs.map(errors);

        // then:
        let expected =
            "protobufify: only enums without fields can be numbered by their discriminants";
        assert_eq!(errors, [[expected], [expected]]);
    }

    #[test]
    fn derives_return_error_when_reserved_is_on_variant_without_payload_message() {
        // given:
        let input = parse_quote! {
            enum Status { #[protobufify(number = 1, reserved(2))] Open }
        };

        // when:
        let errors = errors(input);

        // then:
        assert_eq!(errors, ["protobufify: expected `number = N`"]);
    }

    #[test]
    fn derives_return_all_errors_of_a_type() {
        // given:
        let input = parse_quote! {
            #[protobufify(reserved(5), reserved_names("isbn"))]
            struct Book {
                #[protobufify(number = 0)] title: String,
                #[protobufify(number = 5)] pages: u32,
                #[protobufify(number = 6)] isbn: String,
                author: String,
            }
        };

        // when:
        let errors = errors(input);

        // then:
        assert_eq!(
            errors,
            [
                "protobufify: a field number must be in 1..=536870911, except 19000..=19999",
                "protobufify: 5 is a reserved number of `Book`",
                "protobufify: `Book.isbn` is a reserved name",
                "protobufify: `Book.author` has no number: add `#[protobufify(number = N)]`",
            ]
        );
    }
}
