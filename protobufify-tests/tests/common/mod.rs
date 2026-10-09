//! Helpers shared by the integration tests.

#![allow(dead_code)]

use protobufify::ProtoLock;

/// The crate's `proto.lock`, which the derived decoders and encoders were compiled with.
pub fn lock() -> ProtoLock {
    include_str!("../../proto.lock").parse().unwrap()
}

/// The number of `ty.member` in the lock file.
pub fn number(ty: &str, member: &str) -> u32 {
    lock().number_of(ty, member).unwrap()
}

/// Appends the length-delimited field `ty.member` holding `payload`.
pub fn message(ty: &str, member: &str, payload: &[u8], buf: &mut Vec<u8>) {
    prost::encoding::encode_key(
        number(ty, member),
        prost::encoding::WireType::LengthDelimited,
        buf,
    );
    prost::encoding::encode_varint(payload.len() as u64, buf);
    buf.extend_from_slice(payload);
}
