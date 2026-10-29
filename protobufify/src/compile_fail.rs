//! Doctests that the derives reject invalid numbers: each `compile_fail` block has a twin that
//! differs only in the invalid part and compiles, so that it cannot fail for another reason.
//! Only the compiler evaluates discriminants, so their checks are tested here; the messages of
//! the derives' own errors are tested in protobufify-derive. (Stable rustdoc does not check the
//! error code of a `compile_fail` block, so none is given.)
//!
//! A member without a number:
//!
//! ```compile_fail
//! #[derive(protobufify::ProtoSchema, protobufify::ProtoDecode, protobufify::ProtoEncode)]
//! struct Note {
//!     text: String,
//! }
//! ```
//!
//! ```
//! #[derive(protobufify::ProtoSchema, protobufify::ProtoDecode, protobufify::ProtoEncode)]
//! struct Note {
//!     #[protobufify(number = 1)]
//!     text: String,
//! }
//! ```
//!
//! A discriminant out of range, `0` being the `…_UNSPECIFIED` value:
//!
//! ```compile_fail
//! #[derive(protobufify::ProtoSchema, protobufify::ProtoDecode, protobufify::ProtoEncode)]
//! #[protobufify(numbers_from_discriminants)]
//! #[repr(u64)]
//! enum Room {
//!     Hall = 0,
//! }
//! ```
//!
//! ```compile_fail
//! #[derive(protobufify::ProtoSchema, protobufify::ProtoDecode, protobufify::ProtoEncode)]
//! #[protobufify(numbers_from_discriminants)]
//! #[repr(u64)]
//! enum Room {
//!     Hall = 1 << 31,
//! }
//! ```
//!
//! ```
//! #[derive(protobufify::ProtoSchema, protobufify::ProtoDecode, protobufify::ProtoEncode)]
//! #[protobufify(numbers_from_discriminants)]
//! #[repr(u64)]
//! enum Room {
//!     Hall = (1 << 31) - 1,
//! }
//! ```
//!
//! A discriminant that is another value's explicit number:
//!
//! ```compile_fail
//! #[derive(protobufify::ProtoSchema, protobufify::ProtoDecode, protobufify::ProtoEncode)]
//! #[protobufify(numbers_from_discriminants)]
//! #[repr(u8)]
//! enum Room {
//!     Hall = 1,
//!     #[protobufify(number = 1)]
//!     Study = 2,
//! }
//! ```
//!
//! ```
//! #[derive(protobufify::ProtoSchema, protobufify::ProtoDecode, protobufify::ProtoEncode)]
//! #[protobufify(numbers_from_discriminants)]
//! #[repr(u8)]
//! enum Room {
//!     Hall = 1,
//!     #[protobufify(number = 3)]
//!     Study = 2,
//! }
//! ```
//!
//! A discriminant that is reserved:
//!
//! ```compile_fail
//! #[derive(protobufify::ProtoSchema, protobufify::ProtoDecode, protobufify::ProtoEncode)]
//! #[protobufify(numbers_from_discriminants, reserved(2))]
//! #[repr(u8)]
//! enum Room {
//!     Hall = 2,
//! }
//! ```
//!
//! ```
//! #[derive(protobufify::ProtoSchema, protobufify::ProtoDecode, protobufify::ProtoEncode)]
//! #[protobufify(numbers_from_discriminants, reserved(3))]
//! #[repr(u8)]
//! enum Room {
//!     Hall = 2,
//! }
//! ```
//!
//! Each derive checks discriminants alone, also without `ProtoSchema`:
//!
//! ```compile_fail
//! #[derive(protobufify::ProtoDecode)]
//! #[protobufify(numbers_from_discriminants)]
//! #[repr(u8)]
//! enum Room {
//!     Hall = 0,
//! }
//! ```
//!
//! ```compile_fail
//! #[derive(protobufify::ProtoEncode)]
//! #[protobufify(numbers_from_discriminants)]
//! #[repr(u8)]
//! enum Room {
//!     Hall = 0,
//! }
//! ```
//!
//! ```
//! #[derive(protobufify::ProtoDecode, protobufify::ProtoEncode)]
//! #[protobufify(numbers_from_discriminants)]
//! #[repr(u8)]
//! enum Room {
//!     Hall = 1,
//! }
//! ```
