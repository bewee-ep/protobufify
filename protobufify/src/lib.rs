//! Protocol Buffers for your existing Rust types, without writing a `.proto` file first.
//!
//! - `#[derive(ProtoSchema)]` describes a type in protobuf terms; [`ProtoDefs::render`] writes the
//!   `.proto` schema of the types you export.
//! - Every field, oneof arm and enum value is numbered explicitly, with
//!   `#[protobufify(number = N)]` or, for an enum `#[protobufify(numbers_from_discriminants)]`,
//!   by its discriminants, so that inserting, reordering or removing members never changes the
//!   wire format. A member without a number is a compile error, and so is one that takes a
//!   number of a removed member, listed in `#[protobufify(reserved(…))]`.
//! - `#[derive(ProtoDecode)]` (feature `decode`) decodes protobuf messages of that schema into
//!   the same types, `#[derive(ProtoEncode)]` (feature `encode`) encodes them, both on top of
//!   [prost](https://docs.rs/prost)'s wire-format primitives.

#![no_std]

extern crate alloc;

#[cfg(doctest)]
mod compile_fail;
#[cfg(feature = "decode")]
mod decode;
#[cfg(feature = "encode")]
mod encode;
mod schema;

#[cfg(feature = "decode")]
pub use decode::{ProtoDecode, ProtoDecodeMessage, ProtoError, decode};
#[cfg(feature = "encode")]
pub use encode::{ProtoEncode, ProtoEncodeMessage, encode};
#[cfg(feature = "derive")]
pub use protobufify_derive::{ProtoDecode, ProtoEncode, ProtoSchema};
pub use schema::{
    Arm, Definition, EnumValue, Field, Kind, Label, ProtoDefs, ProtoSchema, ProtoType,
};

/// What the derived code uses; not part of the public API.
#[doc(hidden)]
pub mod __private {
    pub use alloc::vec;
    pub use alloc::vec::Vec;

    pub use crate::schema::LIST_ITEMS_NUMBER;

    #[cfg(feature = "decode")]
    pub use crate::decode::{
        Buf, Bytes, DecodeContext, ProtoDecode, ProtoDecodeMessage, ProtoError, WireType,
        absent_message, decode_key, length_delimited, merge_list, merge_message, skip_field,
    };
    #[cfg(feature = "encode")]
    pub use crate::encode::{
        BufMut, ProtoEncode, ProtoEncodeMessage, encode_length_delimiter, encode_list,
        encode_message_field, length_delimited_len, list_len, message_field_len,
    };
}
