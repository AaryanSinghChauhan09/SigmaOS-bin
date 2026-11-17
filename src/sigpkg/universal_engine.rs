//! Universal package-management engine types.
//!
//! `PackageFormat` lives in `adapter`; this module re-exports it under the
//! `universal_engine` path used across the codebase, and hosts the canonical
//! operation model, dry-run simulator, and snapshot/rollback engine.

pub use crate::sigpkg::adapter::PackageFormat;

use crate::sigpkg::version::Version;
use std::collections::BTreeMap;

/// Canonical package-manager operations SigmaPkg dispatches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UniversalPmOperation {
    Install,
    Remove,
    Upgrade,
    Search,
    QueryInfo,
    CleanCache,
}

/// Result of translating a foreign PM command into a canonical action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchedPmAction {
    /// The foreign package manager name (e.g. `apt`, `dnf`, `pacman`).
    pub source_pm: String,
    /// The canonical operation.
    pub operation: UniversalPmOperation,
    /// Target package names / search terms.
    pub target_packages: Vec<String>,
    /// Whether the command requested a dry run.
    pub dry_run: bool,
    /// Remaining unrecognized flags.
    pub flags: Vec<String>,
}

/// Result of a dry-run install simulation.
#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub package_name: String,
    pub target_format: PackageFormat,
    pub is_valid: bool,
    pub resolved_dependencies: Vec<String>,
    pub required_permissions: Vec<String>,
    pub estimated_size: u64,
    pub warnings: Vec<String>,
}

/// Dry-run simulator: converts a foreign package payload into a canonical
/// install plan without touching the store.
#[derive(Debug, Default)]
pub struct UniversalDryRunSimulator;

impl UniversalDryRunSimulator {
    pub fn new() -> Self { Self }

    /// Simulate installing a package of `format` from raw `content` bytes.
    pub fn simulate_install(
        &self,
        format: PackageFormat,
        content: &[u8],
    ) -> Result<SimulationResult, String> {
        let adapter = crate::sigpkg::adapter::UniversalPackageAdapter::new();
        // Try to parse a manifest if the payload is text; else treat as binary blob.
        let text = String::from_utf8_lossy(content);
        let mut name = "package".to_string();
        let mut deps = Vec::new();
        let mut valid = true;
        if !content.is_empty() && !text.chars().take(64).any(|c| c == '\u{0}') {
            if let Ok(pkg) = adapter.parse_and_translate_manifest("manifest", &text) {
                name = pkg.name.clone();
                deps = pkg.dependencies.iter().map(|d| d.name.clone()).collect();
            }
        } else if content.is_empty() {
            valid = false;
        }
        Ok(SimulationResult {
            package_name: name,
            target_format: format,
            is_valid: valid,
            resolved_dependencies: deps,
            required_permissions: vec!["filesystem:pkg-store".to_string()],
            estimated_size: content.len() as u64,
            warnings: Vec::new(),
        })
    }
}

/// A point-in-time snapshot of installed package state.
#[derive(Debug, Clone)]
pub struct PackageSnapshot {
    pub id: String,
    pub description: String,
    pub packages: BTreeMap<String, Version>,
    pub created_unix: u64,
}

/// Snapshot + rollback engine. Generations are monotonic u32 ids.
#[derive(Debug, Default)]
pub struct SovereignPackageSnapshotRollbackEngine {
    snapshots: BTreeMap<u32, PackageSnapshot>,
    next_gen: u32,
    /// In-memory package set captured at snapshot time.
    current: BTreeMap<String, Version>,
}

impl SovereignPackageSnapshotRollbackEngine {
    pub fn new() -> Self { Self::default() }

    /// Capture current state as a new generation; returns the generation id.
    pub fn create_snapshot(&mut self, description: &str) -> u32 {
        self.next_gen += 1;
        let gen = self.next_gen;
        self.snapshots.insert(gen, PackageSnapshot {
            id: format!("gen-{gen}"),
            description: description.to_string(),
            packages: self.current.clone(),
            created_unix: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        });
        gen
    }

    /// Record the currently-installed set (called by the daemon on changes).
    pub fn record_installed(&mut self, installed: BTreeMap<String, Version>) {
        self.current = installed;
    }

    /// Roll the store back to `generation`. Errors if generation unknown.
    pub fn rollback_to_snapshot(&mut self, generation: u32) -> Result<(), SnapshotError> {
        let snap = self.snapshots.get(&generation).ok_or(SnapshotError::UnknownGeneration(generation))?;
        self.current = snap.packages.clone();
        Ok(())
    }

    /// List snapshot generation ids.
    pub fn list_generations(&self) -> Vec<u32> {
        self.snapshots.keys().copied().collect()
    }
}

/// Snapshot rollback errors.
#[derive(Debug)]
pub enum SnapshotError {
    UnknownGeneration(u32),
}
impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownGeneration(g) => write!(f, "unknown snapshot generation {g}"),
        }
    }
}
impl std::error::Error for SnapshotError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_run_simulates_manifest() {
        let sim = UniversalDryRunSimulator::new();
        let r = sim.simulate_install(PackageFormat::DebianDeb, b"Package: curl\nVersion: 1.0\n").unwrap();
        assert_eq!(r.package_name, "curl");
        assert!(r.is_valid);
    }

    #[test]
    fn snapshot_rollback() {
        let mut e = SovereignPackageSnapshotRollbackEngine::new();
        let gen = e.create_snapshot("baseline");
        assert!(e.rollback_to_snapshot(gen).is_ok());
        assert!(e.rollback_to_snapshot(9999).is_err());
        assert!(e.list_generations().contains(&gen));
    }
}
