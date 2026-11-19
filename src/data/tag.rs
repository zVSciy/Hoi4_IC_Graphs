//! Country tags (`GER`, `SOV`, ...) packed into a single `u64`.
//!
//! Up to eight ASCII bytes are stored left-aligned, so integer ordering equals
//! lexicographic ordering and hashing/comparing a tag is a single word operation.

use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tag(u64);

impl Tag {
    pub const MAX_LEN: usize = 8;

    /// Packs 1..=8 ASCII bytes. Returns `None` for empty or over-long input.
    #[inline]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() || bytes.len() > Self::MAX_LEN || !bytes.is_ascii() {
            return None;
        }
        let mut buf = [0u8; 8];
        buf[..bytes.len()].copy_from_slice(bytes);
        Some(Self(u64::from_be_bytes(buf)))
    }

    /// Const constructor for the built-in faction tables. Panics on invalid input.
    pub const fn from_static(s: &str) -> Self {
        let bytes = s.as_bytes();
        assert!(!bytes.is_empty() && bytes.len() <= Self::MAX_LEN);
        let mut buf = [0u8; 8];
        let mut i = 0;
        while i < bytes.len() {
            buf[i] = bytes[i];
            i += 1;
        }
        Self(u64::from_be_bytes(buf))
    }

    pub fn as_str(&self) -> TagStr {
        let bytes = self.0.to_be_bytes();
        let len = bytes.iter().position(|&b| b == 0).unwrap_or(Self::MAX_LEN) as u8;
        TagStr { bytes, len }
    }

    /// The packed representation, for storage.
    pub fn to_bits(self) -> u64 {
        self.0
    }

    /// Inverse of [`Self::to_bits`]; `None` unless `bits` is a valid packed tag.
    pub fn from_bits(bits: u64) -> Option<Self> {
        let bytes = bits.to_be_bytes();
        let len = bytes.iter().position(|&b| b == 0).unwrap_or(Self::MAX_LEN);
        Self::from_bytes(&bytes[..len]).filter(|tag| tag.0 == bits)
    }
}

/// Stack-allocated string view of a [`Tag`].
pub struct TagStr {
    bytes: [u8; 8],
    len: u8,
}

impl std::ops::Deref for TagStr {
    type Target = str;

    fn deref(&self) -> &str {
        // Tags are only ever built from ASCII bytes.
        std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("?")
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(&self.as_str())
    }
}

impl fmt::Debug for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Tag({})", &*self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_and_orders() {
        let ger = Tag::from_bytes(b"GER").unwrap();
        assert_eq!(&*ger.as_str(), "GER");
        assert_eq!(ger, Tag::from_static("GER"));
        assert!(Tag::from_static("AB") < Tag::from_static("ABC"));
        assert!(Tag::from_static("ABC") < Tag::from_static("ABD"));
        assert!(Tag::from_bytes(b"").is_none());
        assert!(Tag::from_bytes(b"TOOLONGTAG").is_none());
    }
}
