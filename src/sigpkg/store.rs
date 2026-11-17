//! Content-addressed package store (CAS).
//!
//! Blobs are keyed by their SHA-256 digest; package metadata is kept in a
//! JSON index alongside them. Mirrors `sigpkg`'s `/var/lib/sigpkg/store`.

use crate::sigpkg::crypto;
use crate::sigpkg::package::Package;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

/// Errors from the content-addressed store.
#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    /// Package name is empty.
    EmptyName,
    /// No package with that name is stored.
    NotFound(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "store io error: {e}"),
            Self::EmptyName => write!(f, "package name must not be empty"),
            Self::NotFound(n) => write!(f, "package not found: {n}"),
        }
    }
}
impl std::error::Error for StoreError {}
impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self { Self::Io(e) }
}

/// On-disk + in-memory content-addressed package store.
#[derive(Debug)]
pub struct ContentAddressedStore {
    root: PathBuf,
    index: BTreeMap<String, Package>,
}

impl ContentAddressedStore {
    /// Open (creating if needed) a store rooted at `root`.
    pub fn new(root: impl Into<String>) -> Self {
        let root = PathBuf::from(root.into());
        let _ = fs::create_dir_all(root.join("blobs"));
        let _ = fs::create_dir_all(root.join("meta"));
        let mut s = Self { root, index: BTreeMap::new() };
        s.reload();
        s
    }

    fn blob_path(&self, hash: &str) -> PathBuf { self.root.join("blobs").join(hash) }
    fn meta_path(&self, name: &str) -> PathBuf { self.root.join("meta").join(format!("{}.json", safe(name))) }

    fn reload(&mut self) {
        self.index.clear();
        if let Ok(rd) = fs::read_dir(self.root.join("meta")) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(txt) = fs::read_to_string(&p) {
                    if let Ok(pkg) = serde_json::from_str::<Package>(&txt) {
                        self.index.insert(pkg.name.clone(), pkg);
                    }
                }
            }
        }
    }

    /// Store a package + its payload. Returns the content hash (SHA-256 hex).
    pub fn add(&mut self, pkg: Package, payload: &[u8]) -> Result<String, StoreError> {
        if pkg.name.trim().is_empty() {
            return Err(StoreError::EmptyName);
        }
        let hash = crypto::sha256_hex(payload);
        fs::write(self.blob_path(&hash), payload)?;
        self.index.insert(pkg.name.clone(), pkg.clone());
        let meta = serde_json::to_string(&pkg).unwrap_or_default();
        fs::write(self.meta_path(&pkg.name), meta)?;
        Ok(hash)
    }

    /// Retrieve a package's metadata by name.
    pub fn get(&self, name: &str) -> Option<&Package> {
        self.index.get(name)
    }

    /// List all stored packages (sorted by name for determinism).
    pub fn list(&self) -> Vec<Package> {
        self.index.values().cloned().collect()
    }

    /// Remove a package's metadata (blob is left; CAS is append-only).
    pub fn remove(&mut self, name: &str) -> Result<(), StoreError> {
        if self.index.remove(name).is_none() {
            return Err(StoreError::NotFound(name.to_string()));
        }
        let _ = fs::remove_file(self.meta_path(name));
        Ok(())
    }

    /// Read a blob by content hash.
    /// Number of packages in the store.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Garbage-collect orphaned payload blobs not referenced by any package's
    /// checksum. Returns the number of blobs reclaimed.
    pub fn gc_orphans(&mut self) -> usize {
        // Collect hashes still referenced by stored packages.
        let referenced: std::collections::BTreeSet<String> = self
            .index
            .values()
            .map(|p| p.checksum.trim_start_matches("sha256:").to_string())
            .collect();
        let mut reclaimed = 0;
        if let Ok(rd) = fs::read_dir(self.root.join("blobs")) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if !referenced.contains(&name) {
                    if fs::remove_file(e.path()).is_ok() {
                        reclaimed += 1;
                    }
                }
            }
        }
        reclaimed
    }

    pub fn read_blob(&self, hash: &str) -> Result<Vec<u8>, StoreError> {
        Ok(fs::read(self.blob_path(hash))?)
    }
}

fn safe(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sigpkg::version::Version;

    #[test]
    fn add_get_list_remove() {
        let dir = std::env::temp_dir().join(format!("sigcas_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut s = ContentAddressedStore::new(dir.to_string_lossy().to_string());
        let p = Package::new("curl", Version::parse("8.0.0").unwrap(), "client", vec![], "x");
        let h = s.add(p.clone(), b"payload").unwrap();
        assert_eq!(h.len(), 64); // sha256 hex
        assert!(s.get("curl").is_some());
        assert_eq!(s.list().len(), 1);
        assert_eq!(s.read_blob(&h).unwrap(), b"payload");
        s.remove("curl").unwrap();
        assert!(s.get("curl").is_none());
        let _ = fs::remove_dir_all(&dir);
    }
}
