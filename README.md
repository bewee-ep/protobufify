# protobufify

Protocol Buffers for your existing Rust types, without writing a `.proto` file first. Derive the
schema from your types, then encode and decode messages with the same types. Your Rust code stays
the source of truth.

## Getting started

```toml
[dependencies]
protobufify = "0.2"
```

Derive the traits and give every field and enum value a number:

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
pub enum Genre {
    #[protobufify(number = 1)]
    Novel,
    #[protobufify(number = 2)]
    Poetry,
}
```

Render the schema, and encode and decode values:

```rust
use protobufify::{ProtoDefs, ProtoSchema, decode, encode};

// The schema: `define` adds a type and every type it uses.
let mut defs = ProtoDefs::default();
Book::define(&mut defs);
println!("syntax = \"proto3\";\n\npackage library;\n\n{}", defs.render());

// The wire format.
let bytes: Vec<u8> = encode(&book);
let book: Book = decode(&bytes)?;
```

`render()` writes only the definitions; add the `syntax` and `package` lines yourself:

```proto
syntax = "proto3";

package library;

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

The example crate [`examples/bookshelf`](examples/bookshelf) shows a complete setup:

```sh
cargo run -p bookshelf --example schema   # prints the .proto
cargo run -p bookshelf --example decode   # encodes a value and decodes it back
```

## How Rust maps to protobuf

| Rust | protobuf |
|---|---|
| struct with named fields | `message` |
| `Option<T>` | `optional T` |
| `Vec<T>` | `repeated T` |
| `Option<Vec<T>>` | `optional` wrapper message holding the list, so that `None` and an empty list differ |
| enum without fields | `enum`, plus a `…_UNSPECIFIED = 0` value |
| enum with fields | `message` with a `oneof`, one field per variant |
| `bool`, `String`, `f32`, `f64`, `u32`, `u64`, `i32`, `i64` | the matching scalar |
| `u8`, `u16`, `i8`, `i16` | `uint32`, `int32` |
| `///` doc comments | `//` comments |

For an enum with fields, see `Recommendation` in [the example](examples/bookshelf/src/lib.rs):
its variants hold one value (`Book(Book)`), a struct (`ByAuthor { … }`, which becomes a message of
its own), or nothing (`Surprise`, a `bool` field).

Encoding and decoding follow protobuf: unknown fields are skipped, absent fields decode as `None`,
an empty `Vec` or the default value. Protobuf has no default for enums, so a non-`Option` enum
that is absent fails to decode; wrap it in `Option` if it may be missing.

## Numbers

Every field and variant needs `#[protobufify(number = N)]`; a missing, duplicate or invalid
number is a compile error. Numbers, not names or the order of fields, identify fields on the
wire, so never change them once messages are in use.

An enum without fields can use its discriminants instead:

```rust
#[derive(ProtoSchema, ProtoDecode, ProtoEncode)]
#[protobufify(numbers_from_discriminants)]
pub enum Genre {
    Novel = 1,
    Poetry = 2,
}
```

### Removing fields

Never reuse the number of a removed field: old messages would decode its values into the new one.
Reserve the number, and the name, on the type:

```rust
#[derive(ProtoSchema, ProtoDecode, ProtoEncode)]
#[protobufify(reserved(4, "isbn"))]
pub struct Book {
    #[protobufify(number = 1)]
    pub title: String,
}
```

```proto
message Book {
  string title = 1;
  reserved 4;
  reserved "isbn";
}
```

Using a reserved number or name is then a compile error. The same works for enum values, with
their full name as in the `.proto`.

## Names

Names follow the [protobuf style guide](https://protobuf.dev/programming-guides/style/): types
become `UpperCamelCase`, fields `snake_case`, and enum values `UPPER_SNAKE_CASE` prefixed with
the enum (`Genre::Novel` → `GENRE_NOVEL`).

To choose a name yourself, use `#[protobufify(name = "…")]` on a type, field or variant:

```rust
#[derive(ProtoSchema, ProtoDecode, ProtoEncode)]
pub struct Book {
    #[protobufify(number = 1, name = "author")]
    pub writer: String,
}
```

## Features

| Feature | Default | |
|---|---|---|
| `derive` | yes | the derives |
| `decode` | yes | decoding |
| `encode` | yes | encoding |
| `std` | yes | prost's `std` support; without it, protobufify works on `no_std` with `alloc` |

## Limitations

- Generic types, tuple structs and tuple variants with more than one field are not supported.
- No `bytes` and no maps: `Vec<u8>` becomes `repeated uint32`.
- `Option` and `Vec` are recognized by name, so a type alias for them is not.
- prost is pinned to a minor version, as protobufify uses parts of it that prost does not cover by
  semver.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
this crate by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without
any additional terms or conditions.
