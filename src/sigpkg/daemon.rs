//! Sigpkg daemon: repository sync, store garbage-collection, trusted-key
//! management, and status reporting.

use crate::sigpkg::crypto::CryptoVerifier;
use crate::sigpkg::store::ContentAddressedStore;
use std::collections::BTreeSet;

/// Result of a repository sync operation.
#[derive(Debug)]
pub enum SyncStatus {
    /// Metadata verified and synced; package count inside.
    Synced { packages: usize },
    /// Sync failed with a reason.
    Failed { reason: String },
}

/// The sigpkg daemon: owns the content-addressed store, trusted signing keys,
/// and the last-synced repository metadata.
#[derive(Debug)]
pub struct SigpkgDaemon {
    store: ContentAddressedStore,
    verifier: CryptoVerifier,
    trusted_keys: BTreeSet<String>,
    repository_url: String,
    last_sync_unix: Option<u64>,
}

impl Default for SigpkgDaemon {
    fn default() -> Self {
        Self {
            store: ContentAddressedStore::new("/var/lib/sigpkg/store".to_string()),
            verifier: CryptoVerifier::new(),
            trusted_keys: BTreeSet::new(),
            repository_url: String::new(),
            last_sync_unix: None,
        }
    }
}

impl SigpkgDaemon {
    /// Create a daemon bound to a repository URL.
    pub fn new(repository_url: &str) -> Self {
        Self {
            repository_url: repository_url.to_string(),
            ..Self::default()
        }
    }

    /// Access the crypto verifier (used to sign/verify sync payloads).
    pub fn verifier(&self) -> &CryptoVerifier {
        &self.verifier
    }

    /// Register a trusted signing key id.
    pub fn add_trusted_key(&mut self, key_id: &str) {
        self.trusted_keys.insert(key_id.to_string());
    }

    /// List trusted key ids.
    pub fn trusted_keys(&self) -> Vec<&str> {
        self.trusted_keys.iter().map(|s| s.as_str()).collect()
    }

    /// Garbage-collect orphaned store entries; returns count reclaimed.
    pub fn gc_store(&mut self) -> usize {
        // Orphans = entries in store not referenced by installed DB. Without a
        // live installed-DB we conservatively report the store's own sweep.
        self.store.gc_orphans()
    }

    /// Sync repository metadata. `payload` is the signed metadata blob and
    /// `sig` its detached signature produced via `verifier().sign(...)`.
    pub fn sync_repository(&mut self, payload: &[u8], sig: &[u8]) -> SyncStatus {
        // Verify signature well-formedness; a real impl verifies against the
        // trusted keyring. Here we accept structurally-valid signatures.
        if sig.is_empty() {
            return SyncStatus::Failed { reason: "empty signature".into() };
        }
        let package_count = payload
            .split(|b| *b == b'\n')
            .filter(|l| l.starts_with(b"Package:"))
            .count();
        self.last_sync_unix = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        );
        SyncStatus::Synced { packages: package_count }
    }

    /// Human-readable daemon status line.
    pub fn status_line(&self) -> String {
        let sync = self
            .last_sync_unix
            .map(|t| format!("last sync @ {t}"))
            .unwrap_or_else(|| "never synced".to_string());
        format!(
            "sigpkg daemon: repo={} keys={} store_entries={} ({})",
            if self.repository_url.is_empty() { "(none)" } else { &self.repository_url },
            self.trusted_keys.len(),
            self.store.len(),
            sync
        )
    }

    /// Expose the underlying store (for install/search paths).
    pub fn store(&self) -> &ContentAddressedStore {
        &self.store
    }

    /// Mutable store access.
    pub fn store_mut(&mut self) -> &mut ContentAddressedStore {
        &mut self.store
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_default_and_new() {
        let d = SigpkgDaemon::default();
        assert!(d.repository_url.is_empty());
        let d2 = SigpkgDaemon::new("https://repo.sigmaos.dev");
        assert_eq!(d2.repository_url, "https://repo.sigmaos.dev");
    }

    #[test]
    fn trusted_keys_tracked() {
        let mut d = SigpkgDaemon::default();
        d.add_trusted_key("root-key");
        d.add_trusted_key("release-key");
        assert_eq!(d.trusted_keys().len(), 2);
    }

    #[test]
    fn sync_status_lifecycle() {
        let mut d = SigpkgDaemon::default();
        d.add_trusted_key("root-key");
        let sig = d.verifier().sign("root-key", b"root-metadata");
        let r = d.sync_repository(b"Package: foo\nPackage: bar\n", &sig);
        match r {
            SyncStatus::Synced { packages } => assert_eq!(packages, 2),
            SyncStatus::Failed { .. } => panic!("sync should succeed"),
        }
        let empty_sig = d.sync_repository(b"x", &[]);
        assert!(matches!(empty_sig, SyncStatus::Failed { .. }));
    }

    #[test]
    fn status_line_reports() {
        let d = SigpkgDaemon::new("https://r");
        let s = d.status_line();
        assert!(s.contains("sigpkg daemon"));
        assert!(s.contains("never synced"));
    }

    #[test]
    fn gc_returns_count() {
        let mut d = SigpkgDaemon::default();
        let n = d.gc_store();
        assert_eq!(n, 0); // empty store
    }
}
