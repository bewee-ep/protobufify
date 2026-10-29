//! Helpers shared by the integration tests.

/// Appends the length-delimited field `number` holding `payload`.
pub fn message(number: u32, payload: &[u8], buf: &mut Vec<u8>) {
    prost::encoding::encode_key(number, prost::encoding::WireType::LengthDelimited, buf);
    prost::encoding::encode_varint(payload.len() as u64, buf);
    buf.extend_from_slice(payload);
}
