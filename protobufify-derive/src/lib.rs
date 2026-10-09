//! Derive macros of [protobufify](https://docs.rs/protobufify): `ProtoSchema`, `ProtoDecode` and
//! `ProtoEncode`. Use them through the `protobufify` crate, which re-exports them.

mod decode;
mod encode;
mod lock;
mod names;
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
///   `Option<Vec<T>>` → `optional` `<Struct><Field>List` wrapper message, so that an absent list
///   stays distinguishable from an empty one.
/// - Enum without fields → enum, prefixed with the enum name, plus an `…_UNSPECIFIED` zero value.
/// - Enum with fields → message with a `oneof value`, one arm per variant. Struct variants get a
///   payload message `<Enum><Variant>`, a `Vec<T>` payload a `<Enum><Variant>List` wrapper, unit
///   variants a `bool` arm.
#[proc_macro_derive(ProtoSchema)]
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
/// The field and enum value numbers come from the checked-in lock file, read at compile time by
/// the names `ProtoSchema` exports (`<Type>.<member>`): `proto.lock` in the crate root, or the
/// path in the `PROTOBUFIFY_LOCK_FILE` environment variable, relative to the crate root. A member
/// without a number cannot be on the wire yet and is not decoded; updating the lock file
/// (`protobufify::update_lock_file`) assigns it one.
///
/// - Struct with named fields → message. An absent `Option` field is `None`, an absent `Vec` empty,
///   an absent plain field the proto3 default (an enum or a oneof has none: an error).
/// - Enum without fields → enum; an unknown number is an error.
/// - Enum with fields → message with a `oneof value`. Tuple variants need exactly one field;
///   unit variants are a `bool` arm, struct variants a `<Enum><Variant>` payload message.
#[proc_macro_derive(ProtoDecode)]
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
/// The numbers come from the lock file, as for `ProtoDecode`. A member without a number cannot
/// be written: encoding it fails with `EncodeError::Unnumbered` (update the lock file), unless
/// there is nothing to write (a default scalar, `None`, an empty `Vec`).
///
/// - Struct with named fields → message, fields in declaration order. A plain scalar field is
///   omitted when it is the proto3 default; an `Option` field is written iff `Some`; a `Vec` is
///   packed for numbers and enums; enums and messages are always written.
/// - Enum without fields → enum, as the value's number.
/// - Enum with fields → message with a `oneof value` holding exactly the variant's arm: unit
///   variants as `bool` `true`, struct variants as the `<Enum><Variant>` payload message.
#[proc_macro_derive(ProtoEncode)]
pub fn derive_proto_encode(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match encode::expand(&input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}
