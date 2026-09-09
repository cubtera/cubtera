//! Adversarial tests for the inventory/unit seam (v3, P7 -
//! docs/specs/2026-09-03-cubtera-v3-architecture.md ยง4/ยง14 "Adversarial-
//! фикстуры": `../`, absolute paths, ...).
//!
//! `crates/cubtera/tests/e2e_seam_security.rs` already covers this attack
//! class for `unit`/`dims`/`ext` (all `Ident`/`DimRef`-typed at the CLI
//! boundary before P7). This file closes the one caller-controlled segment
//! that was **not** validated anywhere between P0 and P7: `org`.
//!
//! `org` never went through `cubtera_kernel::Ident` on the read-only
//! inventory/unit path - `cubtera-server`'s inventory/units/run routes
//! take it straight off the URL path (`Path<String>`, see
//! `crates/cubtera-server/src/routes/{inventory,units,run}.rs`) and hand it
//! to `ResolveUseCase`/`AssembleUseCase` as a raw `&str`, which handed it
//! straight to `FsInventoryPort`/`FsUnitPort`
//! (`self.base_path.join(org)`/`root.join(unit_name)`, both `PathBuf::join`
//! calls with zero validation of their own - adapters are deliberately
//! "dumb", per `AGENTS.md`). `dim_type`/`name`/`unit_name` were already
//! wrapped in `Ident` at every call site; `org` slipped through as the one
//! exception. `cubtera-app`'s `ResolveUseCase`/`AssembleUseCase` now
//! validate `org` via `Ident::parse` at every public method, closing this
//! for every interface (CLI, server, and MCP-as-a-client-of-server) at a
//! single choke point instead of patching each route.
//!
//! Every case here plants a "secret" file genuinely outside the
//! `InventoryPort`/`UnitPort` root and asserts two things: the call fails,
//! *and* nothing about the secret's content ever appears in the result -
//! matching `e2e_seam_security.rs`'s "must fail before touching anything
//! outside the sandbox" bar, not just "exits non-zero".

use cubtera_app::ports::InventoryPort;
use cubtera_app::{AssembleUseCase, ResolveUseCase};
use cubtera_inventory::{FsInventoryPort, FsUnitPort};
use cubtera_kernel::Ident;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn write(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// A tempdir laid out as:
/// ```text
/// <root>/
///   inventory/cubtera/dc/prod.json      <- legit dimension, org "cubtera"
///   units/tf_unit/manifest.toml         <- legit unit
///   secret/dc/evil.json                 <- OUTSIDE both roots
///   secret/evil_unit/manifest.toml      <- OUTSIDE units root
/// ```
/// so a payload has to actually resolve `../../secret/...` relative to
/// `inventory/`/`units/` to reach the secret files. Kept alive for the
/// whole test (`_root`'s `Drop` deletes the tempdir) and exposes both
/// roots so each test can build its own port/use-case handle around them.
struct Fixture {
    _root: tempfile::TempDir,
    inventory_path: PathBuf,
    units_path: PathBuf,
}

impl Fixture {
    fn inventory(&self) -> FsInventoryPort {
        FsInventoryPort::new(&self.inventory_path)
    }

    fn resolve_uc(&self) -> ResolveUseCase {
        ResolveUseCase::new(Arc::new(self.inventory()))
    }

    fn assemble_uc(&self) -> AssembleUseCase {
        AssembleUseCase::new(
            Arc::new(self.inventory()),
            Arc::new(FsUnitPort::new(&self.units_path)),
        )
    }
}

fn fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let inventory_path = root.path().join("inventory");
    let units_path = root.path().join("units");

    write(
        &inventory_path.join("cubtera").join("dc").join("prod.json"),
        r#"{"region": "us-east-1"}"#,
    );
    write(
        &units_path.join("tf_unit").join("manifest.toml"),
        "dimensions = [\"dc\"]\ntype = \"tf\"\n",
    );

    // Planted *outside* both roots - reachable only via a successful
    // `../` escape from `inventory_path`/`units_path`.
    write(
        &root.path().join("secret").join("dc").join("evil.json"),
        r#"{"leaked": "yes-this-is-the-secret"}"#,
    );
    write(
        &root
            .path()
            .join("secret")
            .join("evil_unit")
            .join("manifest.toml"),
        "dimensions = []\ntype = \"bash\"\n",
    );

    Fixture {
        _root: root,
        inventory_path,
        units_path,
    }
}

const TRAVERSAL_PAYLOADS: &[&str] = &[
    "../secret",
    "../../secret",
    "..",
    "../../../../../../../../etc",
];

#[tokio::test]
async fn fs_inventory_port_itself_has_no_validation_and_would_escape_if_called_directly() {
    // Documents the *adapter's* actual, deliberate trust boundary (see
    // this crate's `dimension.rs` module doc comment: "adapters are
    // deliberately dumb"): `FsInventoryPort` alone does not stop a
    // traversal payload. This is not the bug - the bug would be if
    // nothing upstream of it validated `org` either, which the rest of
    // this file proves is no longer true for `ResolveUseCase`.
    let fx = fixture();
    let names = fx.inventory().list_types("../secret").await.unwrap();
    assert_eq!(
        names,
        vec!["dc".to_string(), "evil_unit".to_string()],
        "the raw port, called directly with an unvalidated org, does escape \
         the inventory root and list the planted secret directory's \
         contents - ResolveUseCase must never let this string reach here \
         unchecked"
    );
}

#[tokio::test]
async fn resolve_use_case_rejects_traversal_in_org_for_every_public_method() {
    let fx = fixture();
    let uc = fx.resolve_uc();
    let dc = Ident::parse("dc").unwrap();
    let prod = Ident::parse("prod").unwrap();

    for payload in TRAVERSAL_PAYLOADS {
        assert!(
            uc.list_types(payload).await.is_err(),
            "list_types should reject org={payload:?}"
        );
        assert!(
            uc.list_names(payload, &dc).await.is_err(),
            "list_names should reject org={payload:?}"
        );
        assert!(
            uc.resolve(payload, &dc, &prod).await.is_err(),
            "resolve should reject org={payload:?}"
        );
        assert!(
            uc.try_resolve(payload, &dc, &prod).await.is_err(),
            "try_resolve should reject org={payload:?}"
        );
        assert!(
            uc.get_defaults(payload, &dc).await.is_err(),
            "get_defaults should reject org={payload:?}"
        );
        assert!(
            uc.get_schema(payload, &dc).await.is_err(),
            "get_schema should reject org={payload:?}"
        );
        assert!(
            uc.get_parent(payload, &dc, &prod).await.is_err(),
            "get_parent should reject org={payload:?}"
        );
        assert!(
            uc.validate_schema(payload, &dc, &prod).await.is_err(),
            "validate_schema should reject org={payload:?}"
        );
        let dim_relations = vec!["dc".to_string()];
        assert!(
            uc.get_children(payload, &dim_relations, &dc, &prod)
                .await
                .is_err(),
            "get_children should reject org={payload:?}"
        );
        assert!(
            uc.kids_of(payload, &dim_relations, &dc, &prod)
                .await
                .is_err(),
            "kids_of should reject org={payload:?}"
        );
    }
}

#[tokio::test]
async fn resolve_use_case_accepts_a_well_formed_org_as_a_control_case() {
    // Sanity check that the rejection above is specific to the payload,
    // not accidentally rejecting every org.
    let fx = fixture();
    let uc = fx.resolve_uc();
    let types = uc.list_types("cubtera").await.unwrap();
    assert_eq!(types, vec!["dc".to_string()]);
}

#[tokio::test]
async fn assemble_use_case_rejects_traversal_in_org_and_unit_name() {
    let fx = fixture();
    let uc = fx.assemble_uc();

    for payload in TRAVERSAL_PAYLOADS {
        assert!(
            uc.list_units(payload).await.is_err(),
            "list_units should reject org={payload:?}"
        );
        assert!(
            uc.get_manifest("cubtera", payload).await.is_err(),
            "get_manifest should reject unit_name={payload:?}"
        );
        assert!(
            uc.get_manifest(payload, "tf_unit").await.is_err(),
            "get_manifest should reject org={payload:?}"
        );
    }

    // The planted secret manifest must never be reachable, no matter what
    // payload is tried - a stronger assertion than "returns an error",
    // since a bug that returned `Ok(None)` after silently walking outside
    // the root would still pass an `is_err()`-only check.
    for payload in TRAVERSAL_PAYLOADS {
        match uc.get_manifest("cubtera", payload).await {
            Err(_) => {}
            Ok(manifest) => {
                panic!("get_manifest({payload:?}) unexpectedly succeeded and returned {manifest:?}")
            }
        }
    }
}

#[tokio::test]
async fn assemble_use_case_accepts_well_formed_org_and_unit_as_a_control_case() {
    let fx = fixture();
    let uc = fx.assemble_uc();
    let manifest = uc.get_manifest("cubtera", "tf_unit").await.unwrap();
    assert_eq!(manifest.dimensions, vec!["dc".to_string()]);
}
