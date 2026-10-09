# protobufify

Protocol Buffers for your existing Rust types — without writing a `.proto` file first.

- **`#[derive(ProtoSchema)]`** describes a type in protobuf terms. Render the `.proto` schema of
  the types you export with `ProtoDefs::render`.
- **A checked-in lock file** pins the number of every field and enum value, so inserting,
  reordering or removing members never changes the wire format.
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
    pub title: String,
    pub pages: Option<u16>,
    pub genre: Option<Genre>,
}

#[derive(ProtoSchema, ProtoDecode, ProtoEncode)]
pub enum Genre {
    Novel,
    Poetry,
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
| `Option<Vec<T>>` | `optional <Struct><Field>List`, a wrapper message with `repeated T items`, so an absent list stays distinguishable from an empty one |
| enum without fields | `enum`, values prefixed with the enum name, plus `<ENUM>_UNSPECIFIED = 0` |
| enum with fields | `message` with a `oneof value`, one arm per variant (struct variants get a `<Enum><Variant>` payload message, unit variants a `bool` arm) |
| `bool`, `String`, `f32`, `f64`, `u32`, `u64`, `i32`, `i64` | the matching scalar |
| `u8`, `u16`, `i8`, `i16` | `uint32`, `int32` (range-checked when decoding) |
| `///` doc comments | `//` comments |

Names are the Rust identifiers; field numbers come from the lock file.

## The lock file

Numbers are not derived from declaration order. They are kept in a lock file (by default: `proto.lock`
in your crate root; overwritable with the `PROTOBUFIFY_LOCK_FILE` environment variable, e.g. via
`[env]` in your `.cargo/config.toml`):

```text
Book.title = 1
Book.author = 2
Book.year = 3 reserved
Book.subtitle = 4
```

- A new member gets the next free number of its type.
- A removed member stays `reserved` (also in the `.proto`), so its number is never reused.
- A reserved member that reappears gets its old number back.
- Renaming a member counts as removing and adding it; to keep its number, rename its lock file
  entry before updating.

`update_lock_file(path, &defs, frozen)` keeps it up to date. It needs the definitions of your
exported types, so call it from a small binary next to them, as the example crate does in
[`examples/bookshelf/src/bin/update-lock.rs`](examples/bookshelf/src/bin/update-lock.rs):

```rust
fn main() -> std::process::ExitCode {
    let frozen = std::env::args().skip(1).any(|arg| arg == "--frozen");
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/proto.lock");
    match protobufify::update_lock_file(path, &bookshelf::definitions(), frozen) {
        Ok((_, changes)) => {
            changes.iter().for_each(|change| println!("{change}"));
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{path}: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
```

Run it after changing exported types (`cargo run --bin update-lock`), and in CI with
`-- --frozen`, which fails instead of writing when the lock file is out of date. Commit the
lock file.

`ProtoDecode` and `ProtoEncode` read the lock file at compile time. A member without a number
yet is not decoded, and encoding a value of it fails with `EncodeError::Unnumbered`, until the
lock file is updated. Changing only `PROTOBUFIFY_LOCK_FILE` does not trigger a rebuild.

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
| `std` | yes | `update_lock_file`; without it, the crate is `no_std` (with `alloc`) |

## Limitations

- Generic types are not supported.
- prost is pinned to a minor version: the derived decoders and encoders use `prost::encoding`,
  which prost does not cover by semver.

## Examples

The example crate [`examples/bookshelf`](examples/bookshelf) is a small model set up the way a
crate using protobufify would be: its types, their `proto.lock`, and the `update-lock` binary.

```sh
cargo run -p bookshelf --example schema      # the .proto of the model
cargo run -p bookshelf --example lock_file   # how numbers stay stable while a type evolves
cargo run -p bookshelf --example decode      # encoding a shelf and decoding it back
cargo run -p bookshelf --bin update-lock     # updating its proto.lock (-- --frozen: checking)
```

The integration tests live in `protobufify-tests`, with their own fixtures and `proto.lock`.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
this crate by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without
any additional terms or conditions.
