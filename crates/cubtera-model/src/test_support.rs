//! Shared test-only helpers for constructing [`cubtera_kernel::InstanceId`]
//! values without repeating the same boilerplate in every value-type's
//! `#[cfg(test)] mod tests`.
#![cfg(test)]

use cubtera_kernel::{DimRef, Ident, InstanceId};

pub fn instance(org: &str, unit: &str, dims: &[&str]) -> InstanceId {
    InstanceId::try_new(
        Ident::parse(org).unwrap(),
        Ident::parse(unit).unwrap(),
        dims.iter().map(|d| DimRef::parse(d).unwrap()),
        [],
    )
    .unwrap()
}
