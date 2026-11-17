//! SigmaPkg — the universal package manager.
//!
//! Translates ~30+ foreign package-manager dialects into a canonical store
//! with content-addressed payloads, checksum/signature verification,
//! dependency canonicalization, repository sync, snapshots/rollback, triggers,
//! and a PR bridge for foreign package contributions.

pub mod adapter;
pub mod crypto;
pub mod daemon;
pub mod dispatcher;
pub mod mapper;
pub mod package;
pub mod repository_manager;
pub mod store;
pub mod universal_adapter;
pub mod universal_engine;
pub mod version;

// Canonical re-exports so `sigmaos::sigpkg::X` resolves directly.
pub use adapter::{AdapterError, PackageFormat, UniversalPackageAdapter};
pub use crypto::{sha256_hex, verify_sha256, CryptoVerifier, VerifyError};
pub use daemon::{SigpkgDaemon, SyncStatus};
pub use dispatcher::{DispatchError, UniversalPmCommandDispatcher};
pub use mapper::UniversalDependencyMapper;
pub use package::{Dependency, Package};
pub use repository_manager::{RepoError, Repository, RepositoryManager};
pub use store::{ContentAddressedStore, StoreError};
pub use universal_adapter::{
    BridgeError, ConvertedHook, ForeignCapability, PackagePr,
    SigPkgUniversalBridgeEngine, SovereignUniversalPmPrBridgeEngine,
    SovereignUniversalPrGatewayEngine, UniversalPackageTriggerEngine,
    UniversalSandboxCapabilityMatrix, UniversalScriptletConverter,
};
pub use universal_engine::{
    DispatchedPmAction, SimulationResult, SnapshotError,
    SovereignPackageSnapshotRollbackEngine, UniversalDryRunSimulator, UniversalPmOperation,
};
pub use version::{Version, VersionConstraint, VersionError};
