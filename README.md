# protobufify

Protocol Buffers for your existing Rust types — without writing a `.proto` file first.

- **`#[derive(ProtoSchema)]`** describes a type in protobuf terms. Render the `.proto` schema of
  the types you export with `ProtoDefs::render`.
- **Every field, oneof arm and enum value has a fixed number**, given with
  `#[protobufify(number = N)]` or by an enum's discriminants. A member without one does not
  compile, and neither does one taking the number of a removed, `reserved` member.
- **`#[derive(ProtoDecode)]`** and **`#[derive(ProtoEncode)]`** decode protobuf messages of that
  schema into the same Rust types and encode them back, on top of
  [prost](https://docs.rs/prost)'s wire-format primitives.

Your Rust types stay the source of truth; the `.proto` file is generated from them, never the
other way round.

```rust
use protobufify::{ProtoDecode, ProtoEncode, ProtoSchema};

/// A book on a shelf.
#[derive(ProtoSchema, ProtoDecode, ProtoEncode)]
pub struct Book {
    #[protobufify(number = 1)]
    pub title: String,
    #[protobufify(number = 2)]
    pub pages: Option<u16>,
    #[protobufify(number = 3)]
    pub genre: Option<Genre>,
}

#[derive(ProtoSchema, ProtoDecode, ProtoEncode)]
#[protobufify(numbers_from_discriminants)]
#[repr(u32)]
pub enum Genre {
    Novel = 1,
    Poetry = 2,
}
```

```proto
// A book on a shelf.
message Book {
  string title = 1;
  optional uint32 pages = 2;
  optional Genre genre = 3;
}

enum Genre {
  GENRE_UNSPECIFIED = 0;
  GENRE_NOVEL = 1;
  GENRE_POETRY = 2;
}
```

## How Rust maps to protobuf

| Rust | proto3 |
|---|---|
| struct with named fields | `message`, fields in declaration order |
| `Option<T>` | `optional T` |
| `Vec<T>` | `repeated T` |
| `Option<Vec<T>>` | `optional <Struct><Field>List`, a wrapper message with `repeated T items = 1`, so an absent list stays distinguishable from an empty one |
| enum without fields | `enum`, values prefixed with the enum name, plus `<ENUM>_UNSPECIFIED = 0` |
| enum with fields | `message` with a `oneof value`, one arm per variant (struct variants get a `<Enum><Variant>` payload message, unit variants a `bool` arm) |
| `bool`, `String`, `f32`, `f64`, `u32`, `u64`, `i32`, `i64` | the matching scalar |
| `u8`, `u16`, `i8`, `i16` | `uint32`, `int32` (range-checked when decoding) |
| `///` doc comments | `//` comments |

Names are the Rust identifiers; numbers are [given explicitly](#numbers).

## Numbers

The number of a field, oneof arm or enum value is `#[protobufify(number = N)]` on the field or
variant, else, for an enum without fields marked `#[protobufify(numbers_from_discriminants)]`,
the variant's discriminant (an explicit `= N` or the implicit one). A member without a number
is a compile error. The numbers apply alike to the schema, encoding and decoding, and never
depend on declaration order.

```rust
#[derive(ProtoSchema, ProtoDecode, ProtoEncode)]
#[protobufify(numbers_from_discriminants)]
#[repr(u32)]
pub enum Room {
    Library = 0x101,
    Study = 0x201,
    #[protobufify(number = 0x301)]
    Attic,
}
```

```proto
enum Room {
  ROOM_UNSPECIFIED = 0;
  ROOM_LIBRARY = 257;
  ROOM_STUDY = 513;
  ROOM_ATTIC = 769;
}
```

- A field or oneof arm number must be in `1..=536870911`, except `19000..=19999` (reserved by
  protobuf); an enum value number in `1..=i32::MAX`, `0` being the `…_UNSPECIFIED` value.
  Anything else, or two members of a type with the same number, is a compile error, for
  discriminants too.
- Without `numbers_from_discriminants`, discriminants do not number anything.

### Removing members

The number of a removed member must never be reused: old messages would decode its values into
the new member. Reserve it, and its name, on the type:

```rust
#[derive(ProtoSchema, ProtoDecode, ProtoEncode)]
#[protobufify(reserved(4, 5), reserved_names("isbn", "year"))]
pub struct Book {
    #[protobufify(number = 1)]
    pub title: String,
    // …
}
```

```proto
message Book {
  string title = 1;
  reserved 4, 5;
  reserved "isbn", "year";
}
```

A member with a reserved number or name is a compile error. Names are those of the schema: a
field or arm name, or the full name of an enum value (`GENRE_COMICS`). For the payload message of
a struct variant, put `reserved` and `reserved_names` on the variant, next to its `number`.

## Encoding and decoding

`encode(&value)` writes what `decode` reads back:

| Rust | Encoded | Decoded when absent |
|---|---|---|
| plain scalar | omitted if it is the proto3 default | the default |
| plain struct | always | the struct with all its fields absent |
| plain enum or data enum | always | error `Missing` |
| `Option<T>` | iff `Some`, also `Some(0)` | `None` |
| `Vec<T>` | numbers and enums packed, others one field each | empty (packed and unpacked both accepted) |
| `Option<Vec<T>>` | `Some` → the `…List` wrapper, also when empty | `None` |
| data enum | exactly its variant's `oneof` arm; a unit variant as `bool` `true` | error `Missing` (any value of a `bool` arm selects its variant) |

Fields are written in declaration order. When decoding, unknown fields are skipped; an unknown
enum number or an integer out of range is an error.

## Features

| Feature | Default | |
|---|---|---|
| `derive` | yes | the `ProtoSchema`, `ProtoDecode` and `ProtoEncode` derives |
| `decode` | yes | decoding, via prost |
| `encode` | yes | encoding, via prost |
| `std` | yes | prost's `std`; without it, the crate is `no_std` (with `alloc`) |

## Limitations

- Generic types are not supported.
- prost is pinned to a minor version: the derived decoders and encoders use `prost::encoding`,
  which prost does not cover by semver.

## Examples

The example crate [`examples/bookshelf`](examples/bookshelf) is a small model set up the way a
crate using protobufify would be: numbered types, a reserved removed field, and the list of
exported types.

```sh
cargo run -p bookshelf --example schema      # the .proto of the model
cargo run -p bookshelf --example decode      # encoding a shelf and decoding it back
```

The integration tests live in `protobufify-tests`, with their own fixtures. The derives' error
messages are unit-tested in `protobufify-derive`; compiler rejections are tested by the
`compile_fail` doctests in [`protobufify/src/compile_fail.rs`](protobufify/src/compile_fail.rs).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
this crate by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without
any additional terms or conditions.
