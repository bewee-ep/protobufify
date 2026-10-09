//! Protobuf encoding: the traits `#[derive(ProtoEncode)]` implements, and the helpers its
//! encoders use. prost does the wire-format work; the field numbers come from the lock file.
use alloc::vec::Vec;
use core::fmt;

pub use prost::bytes::BufMut;
use prost::encoding::{self, WireType};

/// Why a value could not be encoded.
#[derive(Debug)]
pub enum EncodeError {
    /// A member with something to write has no number in the lock file yet (update it with
    /// `update_lock_file`).
    Unnumbered {
        ty: &'static str,
        member: &'static str,
    },
}

impl core::error::Error for EncodeError {}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unnumbered { ty, member } => {
                write!(f, "{ty}.{member}: no number in the lock file")
            }
        }
    }
}

/// A type a protobuf field can hold: a scalar, an enum or a message.
pub trait ProtoEncode {
    /// Writes `self` as the field `number`: key and value.
    fn encode_field(&self, number: u32, buf: &mut impl BufMut) -> Result<(), EncodeError>;

    /// The length [`Self::encode_field`] writes.
    fn encoded_field_len(&self, number: u32) -> usize;

    /// Writes the repeated field `number`; nothing if `values` is empty.
    fn encode_repeated(
        values: &[Self],
        number: u32,
        buf: &mut impl BufMut,
    ) -> Result<(), EncodeError>
    where
        Self: Sized,
    {
        for value in values {
            value.encode_field(number, buf)?;
        }
        Ok(())
    }

    /// The length [`Self::encode_repeated`] writes.
    fn encoded_repeated_len(values: &[Self], number: u32) -> usize
    where
        Self: Sized,
    {
        values.iter().map(|v| v.encoded_field_len(number)).sum()
    }

    /// Whether a plain (non-optional) field holding `self` is omitted: the proto3 default of a
    /// scalar, which the decoder fills in. Enums and messages are always written.
    fn is_default(&self) -> bool {
        false
    }
}

/// A type encoded as a protobuf message (a struct, or a data enum as a message with a oneof).
pub trait ProtoEncodeMessage: ProtoEncode {
    /// Writes the fields of the message, without key and length.
    fn encode_message(&self, buf: &mut impl BufMut) -> Result<(), EncodeError>;

    /// The length [`Self::encode_message`] writes.
    fn message_len(&self) -> usize;
}

/// Scalars proto has a type for; `$repeated` is how prost writes them repeated (numbers packed).
macro_rules! scalars {
    ($($ty:ty => $encoding:ident, $repeated:ident, $repeated_len:ident;)*) => {
        $(
            impl ProtoEncode for $ty {
                fn encode_field(&self, number: u32, buf: &mut impl BufMut) -> Result<(), EncodeError> {
                    encoding::$encoding::encode(number, self, buf);
                    Ok(())
                }

                fn encoded_field_len(&self, number: u32) -> usize {
                    encoding::$encoding::encoded_len(number, self)
                }

                fn encode_repeated(
                    values: &[Self],
                    number: u32,
                    buf: &mut impl BufMut,
                ) -> Result<(), EncodeError> {
                    encoding::$encoding::$repeated(number, values, buf);
                    Ok(())
                }

                fn encoded_repeated_len(values: &[Self], number: u32) -> usize {
                    encoding::$encoding::$repeated_len(number, values)
                }

                fn is_default(&self) -> bool {
                    *self == Self::default()
                }
            }
        )*
    };
}

scalars! {
    bool => bool, encode_packed, encoded_len_packed;
    alloc::string::String => string, encode_repeated, encoded_len_repeated;
    f32 => float, encode_packed, encoded_len_packed;
    f64 => double, encode_packed, encoded_len_packed;
    u32 => uint32, encode_packed, encoded_len_packed;
    u64 => uint64, encode_packed, encoded_len_packed;
    i32 => int32, encode_packed, encoded_len_packed;
    i64 => int64, encode_packed, encoded_len_packed;
}

/// Integers proto has no type for, carried in the wider `$wire` type (as in the schema).
macro_rules! narrowed {
    ($($ty:ty => $wire:ty,)*) => {
        $(
            impl ProtoEncode for $ty {
                fn encode_field(&self, number: u32, buf: &mut impl BufMut) -> Result<(), EncodeError> {
                    <$wire>::from(*self).encode_field(number, buf)
                }

                fn encoded_field_len(&self, number: u32) -> usize {
                    <$wire>::from(*self).encoded_field_len(number)
                }

                fn encode_repeated(
                    values: &[Self],
                    number: u32,
                    buf: &mut impl BufMut,
                ) -> Result<(), EncodeError> {
                    let wide: Vec<$wire> = values.iter().map(|&v| v.into()).collect();
                    <$wire>::encode_repeated(&wide, number, buf)
                }

                fn encoded_repeated_len(values: &[Self], number: u32) -> usize {
                    let wide: Vec<$wire> = values.iter().map(|&v| v.into()).collect();
                    <$wire>::encoded_repeated_len(&wide, number)
                }

                fn is_default(&self) -> bool {
                    *self == 0
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

/// Writes the key and length of the length-delimited field `number` holding `len` bytes.
pub fn encode_length_delimiter(number: u32, len: usize, buf: &mut impl BufMut) {
    encoding::encode_key(number, WireType::LengthDelimited, buf);
    encoding::encode_varint(len as u64, buf);
}

/// The length of the length-delimited field `number` holding `len` bytes.
pub fn length_delimited_len(number: u32, len: usize) -> usize {
    encoding::key_len(number) + encoding::encoded_len_varint(len as u64) + len
}

/// [`ProtoEncode::encode_field`] of a message type: the field holds the message length-delimited.
pub fn encode_message_field<T: ProtoEncodeMessage>(
    message: &T,
    number: u32,
    buf: &mut impl BufMut,
) -> Result<(), EncodeError> {
    encode_length_delimiter(number, message.message_len(), buf);
    message.encode_message(buf)
}

/// [`ProtoEncode::encoded_field_len`] of a message type.
pub fn message_field_len<T: ProtoEncodeMessage>(message: &T, number: u32) -> usize {
    length_delimited_len(number, message.message_len())
}

/// Writes a list field: the `<Owner><Field>List` wrapper message of the schema, whose `items`
/// field (number `items`) holds the elements, so an absent list stays distinguishable from an
/// empty one.
pub fn encode_list<T: ProtoEncode>(
    values: &[T],
    items: u32,
    number: u32,
    buf: &mut impl BufMut,
) -> Result<(), EncodeError> {
    encode_length_delimiter(number, T::encoded_repeated_len(values, items), buf);
    T::encode_repeated(values, items, buf)
}

/// The length [`encode_list`] writes.
pub fn list_len<T: ProtoEncode>(values: &[T], items: u32, number: u32) -> usize {
    length_delimited_len(number, T::encoded_repeated_len(values, items))
}

/// Encodes `value` as a message.
pub fn encode<T: ProtoEncodeMessage>(value: &T) -> Result<Vec<u8>, EncodeError> {
    let mut buf = Vec::with_capacity(value.message_len());
    value.encode_message(&mut buf)?;
    Ok(buf)
}
