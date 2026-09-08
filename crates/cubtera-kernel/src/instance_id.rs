use crate::digest::Digest;
use crate::dim_ref::DimRef;
use crate::error::KernelError;
use crate::ident::Ident;
use crate::safe_segment::SafeSegment;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

/// A boundary marker mixed into [`InstanceId::digest`] between the `dims`
/// and `ext` groups. `Ident`/`DimRef` can never contain a NUL byte, so this
/// value can never collide with a real dimension key - it exists purely to
/// stop `dims=[a,b], ext=[]` and `dims=[a], ext=[b]` from hashing the same
/// way, the v2 "dims vs ext path collision" bug class (H6/H15 in the prior
/// architecture review).
const GROUP_BOUNDARY: &str = "\0cubtera:ext-boundary\0";

/// The literal path/canonical-string component inserted between the
/// `dims` and `ext` groups whenever `ext` is non-empty. A real `DimRef::key`
/// can never render as this string or start with `.` (leading `.` is
/// unconditionally rejected by [`Ident::parse`]), so this marker can never
/// be produced by real dimension data - it is a reserved sentinel, not a
/// convention callers have to avoid colliding with.
const EXT_PATH_MARKER: &str = ".ext";

/// Canonical identity of one addressable unit instance: an org, a unit, and
/// the exact set of required dimensions and extensions it was resolved
/// against.
///
/// This is the *single* source every derived key comes from - the
/// filesystem workspace path, the state-mesh key, the lease/lock key, and
/// the log correlation key all call [`InstanceId::canonical`] or
/// [`InstanceId::digest`] rather than being computed independently. In v2,
/// the temp-folder path preserved CLI dimension order while the state-mesh
/// key sorted it, so the same logical instance could disagree with itself
/// across subsystems; that class of bug is structurally impossible here
/// because there is only one code path that turns dimensions into a key.
///
/// `dims`/`ext` are `BTreeSet<DimRef>` (not `Vec`), so two `InstanceId`s
/// built from the same dimensions in a different order are `Eq` and hash
/// equal - order was never meaningful and is no longer observable.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstanceId {
    org: Ident,
    unit: Ident,
    dims: BTreeSet<DimRef>,
    ext: BTreeSet<DimRef>,
}

impl InstanceId {
    /// Build an `InstanceId`, rejecting a dimension set that names the same
    /// `dim_type` twice within `dims` or within `ext` (with two different
    /// names) - v2 accepted this and unioned both ancestor chains into
    /// access-policy evaluation, which is how the undeclared-dimension
    /// access-policy bypass happened. A duplicate type is ambiguous by
    /// definition ("which `dc` is this instance actually in?") and is
    /// rejected here, once, rather than tolerated by every downstream
    /// consumer.
    pub fn try_new(
        org: Ident,
        unit: Ident,
        dims: impl IntoIterator<Item = DimRef>,
        ext: impl IntoIterator<Item = DimRef>,
    ) -> Result<Self, KernelError> {
        let dims = dedupe_or_reject(dims, "dims")?;
        let ext = dedupe_or_reject(ext, "ext")?;
        Ok(Self {
            org,
            unit,
            dims,
            ext,
        })
    }

    pub fn org(&self) -> &Ident {
        &self.org
    }

    pub fn unit(&self) -> &Ident {
        &self.unit
    }

    pub fn dims(&self) -> &BTreeSet<DimRef> {
        &self.dims
    }

    pub fn ext(&self) -> &BTreeSet<DimRef> {
        &self.ext
    }

    /// Every `type:name` entry across `dims` and `ext` combined, sorted -
    /// the ancestor/ownership set used by access-policy evaluation and by
    /// [`crate::InstanceId`] consumers that need "all dimensions this
    /// instance resolved", without caring which group a given entry came
    /// from.
    pub fn all_refs(&self) -> impl Iterator<Item = &DimRef> {
        self.dims.iter().chain(self.ext.iter())
    }

    /// `{org}/{unit}/{dim1}/{dim2}/.../.ext/{ext1}/...` with dims and
    /// extensions each sorted by `(type, name)`, and the `.ext` marker
    /// present only when `ext` is non-empty. Stable, human-readable, and -
    /// because it is the *only* function that turns this identity into a
    /// string - guaranteed to agree with [`InstanceId::path_segments`].
    ///
    /// The marker matters: without it, `dims=[dome:prod, index:0], ext=[]`
    /// and `dims=[dome:prod], ext=[index:0]` would render as the identical
    /// string once flattened and sorted - exactly the FS unit-state path
    /// collision v2 shipped (dims and extensions sharing one flat
    /// namespace with no boundary between them).
    pub fn canonical(&self) -> String {
        let mut parts: Vec<String> = vec![self.org.to_string(), self.unit.to_string()];
        parts.extend(self.dims.iter().map(DimRef::key));
        if !self.ext.is_empty() {
            parts.push(EXT_PATH_MARKER.to_string());
            parts.extend(self.ext.iter().map(DimRef::key));
        }
        parts.join("/")
    }

    /// A blake3 digest of the canonical identity - the primary key used by
    /// `cubtera-store` (instances/plans/runs/output_sets tables), the
    /// lease key, and the state-mesh key. Fixed-width and collision-
    /// resistant, unlike v2's `"{org}/{unit}@{dims_csv}#{ext_csv}"` string
    /// keys, which could collide when a raw name happened to contain `,`,
    /// `#`, or `@`.
    pub fn digest(&self) -> Digest {
        let mut parts: Vec<String> = vec![self.org.to_string(), self.unit.to_string()];
        parts.extend(self.dims.iter().map(DimRef::key));
        parts.push(GROUP_BOUNDARY.to_string());
        parts.extend(self.ext.iter().map(DimRef::key));
        Digest::of_parts(parts)
    }

    /// Path components for `Workspace::join` (`cubtera-exec`): org, unit,
    /// then each dimension/extension as a single `"type:name"` component,
    /// sorted. Every component is derived from already-validated
    /// [`Ident`]s, so `Workspace` can accept `&[SafeSegment]` and have
    /// "escapes the workspace root" be a type error, not a runtime check
    /// the caller has to remember to make.
    pub fn path_segments(&self) -> Vec<SafeSegment> {
        let seg =
            |s: &str| SafeSegment::parse(s).expect("Ident/marker is always a valid SafeSegment");
        let mut segments = vec![seg(self.org.as_str()), seg(self.unit.as_str())];
        segments.extend(self.dims.iter().map(|d| seg(&d.key())));
        if !self.ext.is_empty() {
            segments.push(seg(EXT_PATH_MARKER));
            segments.extend(self.ext.iter().map(|d| seg(&d.key())));
        }
        segments
    }
}

fn dedupe_or_reject(
    refs: impl IntoIterator<Item = DimRef>,
    scope: &'static str,
) -> Result<BTreeSet<DimRef>, KernelError> {
    let mut by_type: std::collections::BTreeMap<Ident, DimRef> = std::collections::BTreeMap::new();
    for r in refs {
        if let Some(existing) = by_type.get(&r.dim_type) {
            if existing.name != r.name {
                return Err(KernelError::DuplicateDimType {
                    scope,
                    dim_type: r.dim_type.to_string(),
                    first: existing.key(),
                    second: r.key(),
                });
            }
        } else {
            by_type.insert(r.dim_type.clone(), r.clone());
        }
    }
    Ok(by_type.into_values().collect())
}

impl fmt::Display for InstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical())
    }
}

/// Wire form for serde - a plain struct with `Vec<DimRef>` fields (JSON
/// arrays are more natural than sets on the wire), validated back into a
/// deduplicated `BTreeSet` through [`InstanceId::try_new`] on deserialize.
#[derive(Serialize, Deserialize)]
struct InstanceIdWire {
    org: Ident,
    unit: Ident,
    dims: Vec<DimRef>,
    ext: Vec<DimRef>,
}

impl Serialize for InstanceId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        InstanceIdWire {
            org: self.org.clone(),
            unit: self.unit.clone(),
            dims: self.dims.iter().cloned().collect(),
            ext: self.ext.iter().cloned().collect(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for InstanceId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = InstanceIdWire::deserialize(deserializer)?;
        InstanceId::try_new(wire.org, wire.unit, wire.dims, wire.ext)
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(dims: &[&str], ext: &[&str]) -> InstanceId {
        InstanceId::try_new(
            Ident::parse("cubtera").unwrap(),
            Ident::parse("network").unwrap(),
            dims.iter().map(|s| DimRef::parse(s).unwrap()),
            ext.iter().map(|s| DimRef::parse(s).unwrap()),
        )
        .unwrap()
    }

    #[test]
    fn canonical_is_order_independent() {
        let a = id(&["dome:prod", "env:prod"], &[]);
        let b = id(&["env:prod", "dome:prod"], &[]);
        assert_eq!(a.canonical(), b.canonical());
        assert_eq!(a, b);
        assert_eq!(a.digest(), b.digest());
    }

    #[test]
    fn digest_distinguishes_dims_from_ext_boundary() {
        // dims=[a,b], ext=[] vs dims=[a], ext=[b] must not collide (v2 bug
        // class: FS unit-state layout flattened dims and ext into one
        // path with no boundary between them).
        let a = id(&["dome:prod", "index:0"], &[]);
        let b = id(&["dome:prod"], &["index:0"]);
        assert_ne!(a.digest(), b.digest());
        assert_ne!(a.canonical(), b.canonical());
    }

    #[test]
    fn rejects_duplicate_dimension_type_with_different_names() {
        let err = InstanceId::try_new(
            Ident::parse("cubtera").unwrap(),
            Ident::parse("network").unwrap(),
            [
                DimRef::parse("dome:prod").unwrap(),
                DimRef::parse("dome:stg").unwrap(),
            ],
            [],
        )
        .unwrap_err();
        assert!(matches!(err, KernelError::DuplicateDimType { .. }));
    }

    #[test]
    fn allows_exact_duplicate_dim_ref() {
        // Same type *and* name twice is just a redundant input, not
        // ambiguous - the BTreeSet naturally collapses it.
        let inst = id(&["dome:prod", "dome:prod"], &[]);
        assert_eq!(inst.dims().len(), 1);
    }

    #[test]
    fn path_segments_match_canonical_order() {
        let inst = id(&["dome:prod", "env:stg"], &["index:0"]);
        let segments: Vec<String> = inst.path_segments().iter().map(|s| s.to_string()).collect();
        assert_eq!(
            segments,
            vec![
                "cubtera",
                "network",
                "dome:prod",
                "env:stg",
                ".ext",
                "index:0"
            ]
        );
        assert_eq!(segments.join("/"), inst.canonical());
    }

    #[test]
    fn path_segments_omit_marker_when_no_extensions() {
        let inst = id(&["dome:prod"], &[]);
        let segments: Vec<String> = inst.path_segments().iter().map(|s| s.to_string()).collect();
        assert_eq!(segments, vec!["cubtera", "network", "dome:prod"]);
    }

    #[test]
    fn serde_roundtrip() {
        let inst = id(&["dome:prod", "env:stg"], &["index:0"]);
        let s = serde_json::to_string(&inst).unwrap();
        let back: InstanceId = serde_json::from_str(&s).unwrap();
        assert_eq!(inst, back);
    }

    #[test]
    fn deserialize_rejects_duplicate_dimension_type() {
        // DimRef serializes as a plain "type:name" string (see DimRef's
        // Serialize impl), so the wire form is a flat array of strings.
        let json = r#"{"org":"cubtera","unit":"network","dims":["dome:a","dome:b"],"ext":[]}"#;
        let err = serde_json::from_str::<InstanceId>(json);
        assert!(err.is_err(), "expected duplicate dim_type to be rejected");
    }
}
