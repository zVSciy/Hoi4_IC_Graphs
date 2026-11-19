//! DLLs compiled into the executable by `build.rs`.

use super::DllKind;

pub struct EmbeddedDll {
    pub kind: DllKind,
    pub version: &'static str,
    /// Uncompressed size in bytes.
    pub size: u64,
    /// zstd-compressed DLL.
    pub data: &'static [u8],
}

include!(concat!(env!("OUT_DIR"), "/embedded_dlls.rs"));

pub fn versions(kind: DllKind) -> impl Iterator<Item = &'static str> {
    EMBEDDED
        .iter()
        .filter(move |d| d.kind == kind)
        .map(|d| d.version)
}

pub fn find(kind: DllKind, version: &str) -> Option<&'static EmbeddedDll> {
    EMBEDDED
        .iter()
        .find(|d| d.kind == kind && d.version == version)
}
