//! Protobuf decoding: the traits `#[derive(ProtoDecode)]` implements, and the helpers its
//! decoders use. prost does the wire-format work; the field numbers come from the lock file.
use alloc::{string::String, vec::Vec};
use core::fmt;

pub use prost::bytes::{Buf, Bytes};
use prost::encoding;
pub use prost::encoding::{DecodeContext, WireType, decode_key, skip_field};

/// Why protobuf bytes could not be decoded.
#[derive(Debug)]
pub enum ProtoError {
    /// Malformed wire format.
    Wire(prost::DecodeError),
    /// A length-delimited field longer than the remaining input.
    Truncated,
    /// An integer that does not fit the field's type.
    OutOfRange,
    /// An enum number without a value.
    UnknownEnumValue { ty: &'static str, number: i32 },
    /// A field that has no proto3 default (an enum or a oneof) was absent.
    Missing {
        ty: &'static str,
        field: &'static str,
    },
}

impl From<prost::DecodeError> for ProtoError {
    fn from(e: prost::DecodeError) -> Self {
        Self::Wire(e)
    }
}

impl core::error::Error for ProtoError {}

impl fmt::Display for ProtoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wire(e) => write!(f, "{e}"),
            Self::Truncated => write!(f, "field longer than the remaining input"),
            Self::OutOfRange => write!(f, "integer out of range"),
            Self::UnknownEnumValue { ty, number } => write!(f, "{ty}: unknown value {number}"),
            Self::Missing { ty, field } => write!(f, "{ty}.{field}: missing"),
        }
    }
}

/// A type a protobuf field can hold: a scalar, an enum or a message.
pub trait ProtoDecode: Sized {
    /// Whether the lock file has numbers for the type; a type without any decodes nothing (an
    /// unexported type, or one the lock file has not been updated for yet).
    const LOCKED: bool = true;

    /// Merges one occurrence of a field of this type into `value`.
    fn merge(
        value: &mut Option<Self>,
        wire_type: WireType,
        buf: &mut Bytes,
        ctx: DecodeContext,
    ) -> Result<(), ProtoError>;

    /// Merges one occurrence of a repeated field into `values`; numbers may come packed.
    fn merge_repeated(
        values: &mut Vec<Self>,
        wire_type: WireType,
        buf: &mut Bytes,
        ctx: DecodeContext,
    ) -> Result<(), ProtoError> {
        let mut value = None;
        Self::merge(&mut value, wire_type, buf, ctx)?;
        values.extend(value);
        Ok(())
    }

    /// The value of an absent non-optional field (the proto3 default), if the type has one.
    fn absent() -> Option<Self>;
}

/// A type encoded as a protobuf message (a struct, or a data enum as a message with a oneof).
pub trait ProtoDecodeMessage: ProtoDecode {
    /// Decodes the message filling all of `buf`.
    fn decode_message(buf: &mut Bytes, ctx: DecodeContext) -> Result<Self, ProtoError>;
}

macro_rules! scalars {
    ($($ty:ty => $encoding:ident,)*) => {
        $(
            impl ProtoDecode for $ty {
                fn merge(
                    value: &mut Option<Self>,
                    wire_type: WireType,
                    buf: &mut Bytes,
                    ctx: DecodeContext,
                ) -> Result<(), ProtoError> {
                    let mut decoded = Self::default();
                    encoding::$encoding::merge(wire_type, &mut decoded, buf, ctx)?;
                    *value = Some(decoded);
                    Ok(())
                }

                fn merge_repeated(
                    values: &mut Vec<Self>,
                    wire_type: WireType,
                    buf: &mut Bytes,
                    ctx: DecodeContext,
                ) -> Result<(), ProtoError> {
                    Ok(encoding::$encoding::merge_repeated(wire_type, values, buf, ctx)?)
                }

                fn absent() -> Option<Self> {
                    Some(Self::default())
                }
            }
        )*
    };
}

scalars! {
    bool => bool,
    String => string,
    f32 => float,
    f64 => double,
    u32 => uint32,
    u64 => uint64,
    i32 => int32,
    i64 => int64,
}

/// Integers proto has no type for, carried in the wider `$wire` type (as in the schema).
macro_rules! narrowed {
    ($($ty:ty => $wire:ty,)*) => {
        $(
            impl ProtoDecode for $ty {
                fn merge(
                    value: &mut Option<Self>,
                    wire_type: WireType,
                    buf: &mut Bytes,
                    ctx: DecodeContext,
                ) -> Result<(), ProtoError> {
                    let mut wide = None;
                    <$wire>::merge(&mut wide, wire_type, buf, ctx)?;
                    *value = wide.map(Self::try_from).transpose().map_err(|_| ProtoError::OutOfRange)?;
                    Ok(())
                }

                fn merge_repeated(
                    values: &mut Vec<Self>,
                    wire_type: WireType,
                    buf: &mut Bytes,
                    ctx: DecodeContext,
                ) -> Result<(), ProtoError> {
                    let mut wide = Vec::new();
                    <$wire>::merge_repeated(&mut wide, wire_type, buf, ctx)?;
                    for value in wide {
                        values.push(Self::try_from(value).map_err(|_| ProtoError::OutOfRange)?);
                    }
                    Ok(())
                }

                fn absent() -> Option<Self> {
                    Some(0)
                }
            }
        )*
    };
}

narrowed! {
    u8 => u32,
    u16 => u32,
    i8 => i32,
    i16 => i32,
}

/// The payload of a length-delimited field.
pub fn length_delimited(wire_type: WireType, buf: &mut Bytes) -> Result<Bytes, ProtoError> {
    encoding::check_wire_type(WireType::LengthDelimited, wire_type)?;
    let len = encoding::decode_varint(buf)?;
    if len > buf.remaining() as u64 {
        return Err(ProtoError::Truncated);
    }
    Ok(buf.copy_to_bytes(len as usize))
}

/// [`ProtoDecode::merge`] of a message type: the field holds the message length-delimited.
pub fn merge_message<T: ProtoDecodeMessage>(
    value: &mut Option<T>,
    wire_type: WireType,
    buf: &mut Bytes,
    ctx: DecodeContext,
) -> Result<(), ProtoError> {
    let mut payload = length_delimited(wire_type, buf)?;
    *value = Some(T::decode_message(&mut payload, ctx)?);
    Ok(())
}

/// [`ProtoDecode::absent`] of a message type: the message with all fields absent.
pub fn absent_message<T: ProtoDecodeMessage>() -> Option<T> {
    T::decode_message(&mut Bytes::new(), DecodeContext::default()).ok()
}

/// Merges a list field into `value`: the `<Owner><Field>List` wrapper message of the schema,
/// whose `items` field (number `items`) holds the elements, so an absent list stays
/// distinguishable from an empty one.
pub fn merge_list<T: ProtoDecode>(
    value: &mut Option<Vec<T>>,
    items: u32,
    wire_type: WireType,
    buf: &mut Bytes,
    ctx: DecodeContext,
) -> Result<(), ProtoError> {
    let mut payload = length_delimited(wire_type, buf)?;
    let list = value.get_or_insert_with(Vec::new);
    while payload.has_remaining() {
        let (number, wire_type) = decode_key(&mut payload)?;
        if number == items {
            T::merge_repeated(list, wire_type, &mut payload, ctx.clone())?;
        } else {
            skip_field(wire_type, number, &mut payload, ctx.clone())?;
        }
    }
    Ok(())
}

/// Decodes `bytes` as the message `T`.
pub fn decode<T: ProtoDecodeMessage>(bytes: &[u8]) -> Result<T, ProtoError> {
    T::decode_message(&mut Bytes::copy_from_slice(bytes), DecodeContext::default())
}
