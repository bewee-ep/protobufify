//! Protocol Buffers for your existing Rust types, without writing a `.proto` file first.
//!
//! - `#[derive(ProtoSchema)]` describes a type in protobuf terms; [`ProtoDefs::render`] writes the
//!   `.proto` schema of the types you export.
//! - [`ProtoLock`] (the checked-in lock file) pins the number of every field and enum value, so
//!   that inserting, reordering or removing them never changes the wire format.
//! - `#[derive(ProtoDecode)]` (feature `decode`) decodes protobuf messages of that schema into
//!   the same types, `#[derive(ProtoEncode)]` (feature `encode`) encodes them, both on top of
//!   [prost](https://docs.rs/prost)'s wire-format primitives.
//!
//! The derives read the lock file at compile time: `proto.lock` in the crate root, or the path in
//! the `PROTOBUFIFY_LOCK_FILE` environment variable, relative to the crate root.

#![no_std]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "decode")]
mod decode;
#[cfg(feature = "encode")]
mod encode;
mod lock;
mod schema;

#[cfg(feature = "decode")]
pub use decode::{ProtoDecode, ProtoDecodeMessage, ProtoError, decode};
#[cfg(feature = "encode")]
pub use encode::{EncodeError, ProtoEncode, ProtoEncodeMessage, encode};
pub use lock::{LOCK_FILE_HEADER, LockAction, LockChange, ParseLockError, ProtoLock};
#[cfg(feature = "std")]
pub use lock::{LockFileError, update_lock_file};
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

    #[cfg(feature = "decode")]
    pub use crate::decode::{
        Buf, Bytes, DecodeContext, ProtoDecode, ProtoDecodeMessage, ProtoError, WireType,
        absent_message, decode_key, length_delimited, merge_list, merge_message, skip_field,
    };
    #[cfg(feature = "encode")]
    pub use crate::encode::{
        BufMut, EncodeError, ProtoEncode, ProtoEncodeMessage, encode_length_delimiter, encode_list,
        encode_message_field, length_delimited_len, list_len, message_field_len,
    };
}
