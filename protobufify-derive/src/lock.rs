//! The lock file of the crate being compiled, read by `ProtoDecode` and `ProtoEncode`.

use proc_macro2::TokenStream;
use quote::quote;

/// The lock file name in the crate root if `PROTOBUFIFY_LOCK_FILE` is not set.
pub(crate) const DEFAULT_LOCK_FILE: &str = "proto.lock";

/// The lock file of the crate being compiled: `PROTOBUFIFY_LOCK_FILE` (or [`DEFAULT_LOCK_FILE`])
/// relative to its root. Cargo does not rebuild when only the variable changes.
pub(crate) fn lock_path() -> Option<std::path::PathBuf> {
    let dir = std::env::var_os("CARGO_MANIFEST_DIR")?;
    let file =
        std::env::var_os("PROTOBUFIFY_LOCK_FILE").unwrap_or_else(|| DEFAULT_LOCK_FILE.into());
    Some(std::path::Path::new(&dir).join(file))
}

/// The numbers of the lock file by `<Type>.<member>`, without the reserved ones; empty
/// without the file. Read on every expansion rather than cached, so that a long-running
/// proc-macro server (an IDE) never decodes with outdated numbers.
pub(crate) fn lock_numbers() -> std::collections::HashMap<String, u32> {
    let text = lock_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_default();
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let (key, value) = line.split_once(" = ")?;
            let number = value.parse().ok()?; // `<number> reserved` does not parse
            Some((key.to_string(), number))
        })
        .collect()
}

/// Makes cargo rebuild when the lock file changes. A statement, placed in a generated
/// function body, where cbindgen (which reads the expanded crate for the C header) ignores it.
pub(crate) fn lock_dependency() -> TokenStream {
    match lock_path().filter(|path| path.exists()) {
        Some(path) => {
            let path = path.to_string_lossy().into_owned();
            quote!(
                const _: &[u8] = include_bytes!(#path);
            )
        }
        None => quote!(),
    }
}
