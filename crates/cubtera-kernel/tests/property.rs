//! Property-based tests for the kernel identity types.
//!
//! These target exactly the two invariants the v3 spec claims for
//! `cubtera-kernel` (docs/specs/2026-09-03-cubtera-v3-architecture.md ยง4):
//! 1. A validated `Ident`/`DimRef`/`InstanceId` can never encode a path-
//!    traversal or absolute-path payload, for *any* input the fuzzer finds,
//!    not just the handful of examples in the unit tests.
//! 2. `InstanceId::canonical`/`digest` are order-independent, so a caller
//!    can never construct two "logically equal" instances that disagree
//!    with each other (the root cause of several v2 key-collision bugs).

use cubtera_kernel::{DimRef, Ident, InstanceId, SafeSegment};
use proptest::prelude::*;

/// Strings that, if accepted, would let an `Ident`/`SafeSegment` escape a
/// workspace root or collide with a reserved inventory record name.
fn adversarial_string() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("..".to_string()),
        Just(".".to_string()),
        "[a-z]{0,4}".prop_map(|s| format!("../{s}")),
        "[a-z]{1,4}".prop_map(|s| format!("{s}/../{s}")),
        "[a-z]{0,4}".prop_map(|s| format!("/{s}")),
        "[a-z]{0,4}".prop_map(|s| format!(".{s}")),
        "[a-z]{0,4}".prop_map(|s| format!("#{s}")),
        Just(String::new()),
        Just("a\0b".to_string()),
        "[a-z]{1,4}".prop_map(|s| format!("{s}:{s}")),
    ]
}

/// Strings that a real org/unit/dimension name would plausibly look like -
/// used to make sure the grammar isn't accidentally rejecting everything.
fn plausible_ident() -> impl Strategy<Value = String> {
    "[a-zA-Z][a-zA-Z0-9_-]{0,30}"
}

proptest! {
    #[test]
    fn ident_rejects_every_adversarial_input(raw in adversarial_string()) {
        prop_assert!(
            Ident::parse(&raw).is_err(),
            "Ident::parse accepted adversarial input {raw:?}"
        );
    }

    #[test]
    fn ident_accepts_plausible_names_and_roundtrips_lowercase(raw in plausible_ident()) {
        let parsed = Ident::parse(&raw).expect("plausible identifier should parse");
        prop_assert_eq!(parsed.as_str(), raw.to_ascii_lowercase());
    }

    #[test]
    fn ident_output_is_always_a_single_safe_segment(raw in plausible_ident()) {
        let ident = Ident::parse(&raw).unwrap();
        // Whatever a valid Ident renders as, SafeSegment::parse must accept
        // it as a single path component - Workspace::join only accepts
        // SafeSegment, so this is what makes every Ident-derived path
        // component safe to actually join.
        prop_assert!(SafeSegment::parse(ident.as_str()).is_ok());
    }

    #[test]
    fn split_relative_path_never_accepts_a_traversal_component(
        segments in prop::collection::vec(prop_oneof![plausible_ident(), Just("..".to_string()), Just(".".to_string())], 1..6)
    ) {
        let raw = segments.join("/");
        let contains_traversal = segments.iter().any(|s| s == ".." || s == ".");
        let result = SafeSegment::split_relative_path(&raw);
        if contains_traversal {
            prop_assert!(result.is_err(), "expected {raw:?} to be rejected (contains . or ..)");
        } else {
            prop_assert!(result.is_ok(), "expected {raw:?} to be accepted");
        }
    }

    #[test]
    fn instance_id_canonical_and_digest_are_permutation_invariant(
        mut dim_names in prop::collection::vec("[a-z][a-z0-9]{0,6}", 1..5)
    ) {
        dim_names.sort();
        dim_names.dedup();
        let dims: Vec<DimRef> = dim_names
            .iter()
            .enumerate()
            .map(|(i, name)| DimRef::new(Ident::parse(&format!("dt{i}")).unwrap(), Ident::parse(name).unwrap()))
            .collect();

        let forward = InstanceId::try_new(
            Ident::parse("org").unwrap(),
            Ident::parse("unit").unwrap(),
            dims.clone(),
            [],
        ).unwrap();

        let mut reversed = dims.clone();
        reversed.reverse();
        let backward = InstanceId::try_new(
            Ident::parse("org").unwrap(),
            Ident::parse("unit").unwrap(),
            reversed,
            [],
        ).unwrap();

        prop_assert_eq!(forward.canonical(), backward.canonical());
        prop_assert_eq!(forward.digest(), backward.digest());
        prop_assert_eq!(forward, backward);
    }

    #[test]
    fn instance_id_path_segments_never_escape_when_joined(
        dims in prop::collection::vec(("[a-z][a-z0-9]{0,6}", "[a-z][a-z0-9]{0,6}"), 0..4)
    ) {
        let dim_refs: Vec<DimRef> = dims
            .into_iter()
            .map(|(t, n)| DimRef::new(Ident::parse(&t).unwrap(), Ident::parse(&n).unwrap()))
            .collect();
        let inst = InstanceId::try_new(
            Ident::parse("org").unwrap(),
            Ident::parse("unit").unwrap(),
            dim_refs,
            [],
        );
        // Construction can fail (duplicate dim types from the generator),
        // which is fine - the property under test is that *if* it
        // succeeds, every resulting path segment is traversal-free.
        if let Ok(inst) = inst {
            for segment in inst.path_segments() {
                let s = segment.to_string();
                prop_assert!(!s.contains(".."));
                prop_assert!(!s.starts_with('/'));
            }
        }
    }
}
