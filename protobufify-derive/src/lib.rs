//! Derive macros of [protobufify](https://docs.rs/protobufify): `ProtoSchema`, `ProtoDecode` and
//! `ProtoEncode`. Use them through the `protobufify` crate, which re-exports them.

mod attrs;
mod decode;
mod encode;
mod errors;
mod model;
mod names;
mod schema;
mod shape;

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

/// Analyzes `input` and generates its code with `generate`; the errors of the analysis, which
/// all three derives report alike, so that rustc shows each once.
fn derive(
    input: TokenStream,
    generate: fn(&model::Model) -> proc_macro2::TokenStream,
) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match model::analyze(&input) {
        Ok(model) => generate(&model).into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Implements `protobufify::ProtoSchema`: records the type's proto definition, and those of the
/// types it references, so that `protobufify::ProtoDefs::render` can write the `.proto` file.
///
/// Fields, variants and enum values keep their declaration order. Field types are mapped
/// syntactically (`Option`, `Vec`); the compiler checks that every other field type implements
/// `ProtoSchema`. In the generated message names below, `<…>` are proto names in upper camel case.
///
/// - Struct with named fields → message. `Option<T>` → `optional`, `Vec<T>` → `repeated`,
///   `Option<Vec<T>>` → `optional` `<Message><Field>List` wrapper message (its `repeated items`
///   field is number 1), so that an absent list stays distinguishable from an empty one.
/// - Enum without fields → enum, prefixed with the enum name, plus an `…_UNSPECIFIED` zero value.
/// - Enum with fields → message with a `oneof value`, one arm per variant. Struct variants get a
///   payload message `<Enum><Arm>`, a `Vec<T>` payload a `<Enum><Arm>List` wrapper, unit
///   variants a `bool` arm.
///
/// Every member is numbered explicitly: `#[protobufify(number = N)]` on the field or variant,
/// else, for an enum without fields marked `#[protobufify(numbers_from_discriminants)]`, the
/// variant's discriminant. A member without a number is a compile error, and so are numbers out
/// of range (`1..=536870911` except `19000..=19999` for fields and arms, `1..=i32::MAX` for enum
/// values), given twice, or reserved. Discriminants are checked by emitted constants, as only the
/// compiler knows them.
///
/// `#[protobufify(reserved(N, "name", …))]` on the type (on a struct variant, for its payload
/// message) reserves the numbers and names of removed members: no member may
/// take them, and the schema lists them as `reserved`.
///
/// Names are `#[protobufify(name = "…")]` on the type, field or variant, verbatim, else the Rust
/// identifier converted to the protobuf style guide: upper camel case for types, snake case for
/// fields and oneof arms, `<ENUM>_<VARIANT>` in upper snake case for enum values. Generated
/// messages and enum value prefixes follow them. Every name must be a proto identifier (an ASCII
/// letter, then ASCII letters, digits or `_`); no two members of a type may share a name, no two
/// fields a JSON name (`page_count` and `pageCount`), and no two generated messages a name.
#[proc_macro_derive(ProtoSchema, attributes(protobufify))]
pub fn derive_proto_schema(input: TokenStream) -> TokenStream {
    derive(input, schema::expand)
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
///   unit variants are a `bool` arm, struct variants a `<Enum><Arm>` payload message.
#[proc_macro_derive(ProtoDecode, attributes(protobufify))]
pub fn derive_proto_decode(input: TokenStream) -> TokenStream {
    derive(input, decode::expand)
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
///   variants as `bool` `true`, struct variants as the `<Enum><Arm>` payload message.
#[proc_macro_derive(ProtoEncode, attributes(protobufify))]
pub fn derive_proto_encode(input: TokenStream) -> TokenStream {
    derive(input, encode::expand)
}

#[cfg(test)]
mod tests {
    use syn::parse_quote;

    use super::*;

    /// The error messages of the derives for `input`.
    fn errors(input: DeriveInput) -> Vec<String> {
        match model::analyze(&input) {
            Ok(_) => panic!("expected errors"),
            Err(errors) => errors.into_iter().map(|e| e.to_string()).collect(),
        }
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
        assert_eq!(errors, ["protobufify: expected `number`, `name`"]);
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
        assert_eq!(
            errors,
            ["protobufify: number and name the variant, not its field"]
        );
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
                enum Room { #[protobufify(number = 1)] Hall(u32) }
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
        assert_eq!(
            errors,
            [
                "protobufify: on a variant, `reserved` is that of the payload message of a struct variant; reserve arms on the enum"
            ]
        );
    }

    #[test]
    fn derives_return_all_errors_of_a_type() {
        // given:
        let input = parse_quote! {
            #[protobufify(reserved(5, "isbn"))]
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

    #[test]
    fn derives_return_error_when_name_is_not_a_proto_name() {
        // given:
        let input = parse_quote! {
            struct Note { #[protobufify(number = 1, name = "4th")] text: String }
        };

        // when:
        let errors = errors(input);

        // then:
        assert_eq!(
            errors,
            [
                "protobufify: \"4th\" is not a proto name: an ASCII letter, then ASCII letters, digits or `_`"
            ]
        );
    }

    #[test]
    fn derives_return_error_when_reserved_name_is_not_a_proto_name() {
        // given:
        let input = parse_quote! {
            #[protobufify(reserved("4"))]
            struct Note { #[protobufify(number = 1)] text: String }
        };

        // when:
        let errors = errors(input);

        // then:
        assert_eq!(
            errors,
            [
                "protobufify: \"4\" is not a proto name: an ASCII letter, then ASCII letters, digits or `_`"
            ]
        );
    }

    #[test]
    fn derives_return_errors_when_something_is_reserved_twice() {
        // given:
        let inputs: [DeriveInput; 2] = [
            parse_quote! {
                #[protobufify(reserved(2, 2))]
                struct Note { #[protobufify(number = 1)] text: String }
            },
            parse_quote! {
                #[protobufify(reserved("isbn", "isbn"))]
                struct Note { #[protobufify(number = 1)] text: String }
            },
        ];

        // when:
        let errors = inputs.map(errors);

        // then:
        assert_eq!(
            errors,
            [
                ["protobufify: 2 is reserved twice"],
                ["protobufify: `isbn` is reserved twice"]
            ]
        );
    }

    #[test]
    fn derives_return_error_when_name_is_given_twice() {
        // given:
        let input = parse_quote! {
            struct Note { #[protobufify(number = 1, name = "a")] #[protobufify(name = "b")] text: String }
        };

        // when:
        let errors = errors(input);

        // then:
        assert_eq!(errors, ["protobufify: `name` given twice"]);
    }

    #[test]
    fn derives_return_error_when_type_identifier_is_not_a_proto_name() {
        // given:
        let input = parse_quote! {
            struct Größe { #[protobufify(number = 1)] value: u32 }
        };

        // when:
        let errors = errors(input);

        // then:
        assert_eq!(
            errors,
            [
                "protobufify: `Größe` is not a proto name (an ASCII letter, then ASCII letters, digits or `_`): add `#[protobufify(name = \"…\")]`"
            ]
        );
    }

    #[test]
    fn derives_accept_type_identifier_that_is_not_a_proto_name_when_renamed() {
        // given:
        let input: DeriveInput = parse_quote! {
            #[protobufify(name = "Size")]
            struct Größe { #[protobufify(number = 1)] value: u32 }
        };

        // when:
        let model = model::analyze(&input);

        // then:
        assert!(model.is_ok());
    }

    #[test]
    fn derives_return_error_when_reserved_is_neither_a_number_nor_a_name() {
        // given:
        let input = parse_quote! {
            #[protobufify(reserved(true))]
            struct Note { #[protobufify(number = 1)] text: String }
        };

        // when:
        let errors = errors(input);

        // then:
        assert_eq!(errors, ["protobufify: expected a number or a \"name\""]);
    }
}
