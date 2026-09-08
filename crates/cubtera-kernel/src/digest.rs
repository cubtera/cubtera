use serde::{Deserialize, Serialize};
use std::fmt;

/// A blake3-256 content digest: 32 bytes, rendered as 64 lowercase hex
/// characters. Used for [`crate::InstanceId::digest`] (the primary key in
/// `cubtera-store`, the lease/lock key, and the state-mesh key - one value
/// instead of v2's three independently-derived, occasionally-disagreeing
/// keys), for `UnitPackage`/artifact content-addressing, and anywhere else
/// this workspace needs a stable, collision-resistant identifier for a
/// byte string.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digest([u8; 32]);

impl Digest {
    pub fn of(bytes: impl AsRef<[u8]>) -> Self {
        Self(blake3::hash(bytes.as_ref()).into())
    }

    /// Hash a sequence of parts as a single digest, with each part's length
    /// mixed in ahead of its bytes. This is *not* the same as hashing the
    /// concatenation of the parts: `["ab", "c"]` and `["a", "bc"]` hash
    /// differently, which matters wherever a digest is built from several
    /// semantically-distinct fields (e.g. `InstanceId::digest`, which mixes
    /// org/unit/dims/ext) and must not let a value straddling two fields
    /// collide with a different split of the same bytes.
    pub fn of_parts<I, B>(parts: I) -> Self
    where
        I: IntoIterator<Item = B>,
        B: AsRef<[u8]>,
    {
        let mut hasher = blake3::Hasher::new();
        for part in parts {
            let bytes = part.as_ref();
            hasher.update(&(bytes.len() as u64).to_le_bytes());
            hasher.update(bytes);
        }
        Self(hasher.finalize().into())
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn from_hex(hex: &str) -> Option<Self> {
        if hex.len() != 64 {
            return None;
        }
        let mut bytes = [0u8; 32];
        for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
            let s = std::str::from_utf8(chunk).ok()?;
            bytes[i] = u8::from_str_radix(s, 16).ok()?;
        }
        Some(Self(bytes))
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Digest({})", self.to_hex())
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl Serialize for Digest {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Digest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Digest::from_hex(&raw).ok_or_else(|| serde::de::Error::custom("invalid digest hex"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let d = Digest::of(b"hello");
        let hex = d.to_hex();
        assert_eq!(hex.len(), 64);
        assert_eq!(Digest::from_hex(&hex), Some(d));
    }

    #[test]
    fn deterministic() {
        assert_eq!(Digest::of(b"hello"), Digest::of(b"hello"));
        assert_ne!(Digest::of(b"hello"), Digest::of(b"world"));
    }

    #[test]
    fn of_parts_is_not_naive_concatenation() {
        let a = Digest::of_parts(["ab", "c"]);
        let b = Digest::of_parts(["a", "bc"]);
        assert_ne!(
            a, b,
            "length-prefixing must prevent field-boundary collisions"
        );
    }

    #[test]
    fn of_parts_order_matters() {
        let a = Digest::of_parts(["a", "b"]);
        let b = Digest::of_parts(["b", "a"]);
        assert_ne!(a, b);
    }

    #[test]
    fn serde_roundtrip() {
        let d = Digest::of(b"payload");
        let json = serde_json::to_string(&d).unwrap();
        let back: Digest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, d);
    }
}
