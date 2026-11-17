//! Repository model and multi-repo manager.
//!
//! `Repository` is a single package source (name + URL + format + mirror
//! list). `RepositoryManager` owns the configured set, sync state, and
//! trusted-signing-key policy.

use crate::sigpkg::adapter::PackageFormat;
use std::collections::BTreeMap;

/// Errors from repository operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoError {
    /// Repository name or URL is empty.
    Invalid(String),
    /// No repository with that name exists.
    NotFound(String),
    /// Duplicate repository name.
    Duplicate(String),
}

impl std::fmt::Display for RepoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(m) => write!(f, "invalid repository: {m}"),
            Self::NotFound(n) => write!(f, "repository not found: {n}"),
            Self::Duplicate(n) => write!(f, "duplicate repository: {n}"),
        }
    }
}
impl std::error::Error for RepoError {}

/// A single configured package repository.
#[derive(Debug, Clone)]
pub struct Repository {
    pub name: String,
    pub url: String,
    pub format: PackageFormat,
    /// Mirror URLs tried in order.
    pub mirrors: Vec<String>,
    /// Whether the repo requires signature verification.
    pub verify_signatures: bool,
    /// Sync timestamp (unix seconds) of last successful refresh; 0 = never.
    pub last_synced: u64,
    /// Number of packages seen at last sync.
    pub package_count: usize,
    /// Component names this repository provides.
    pub components: Vec<String>,
}

impl Repository {
    /// Create a repository. `format` is inferred from the URL when obvious.
    /// Empty name/url are stored as-is (validated by `validate`).
    pub fn new(name: &str, url: &str) -> Self {
        let format = infer_format(url).unwrap_or(PackageFormat::Sigma);
        Self {
            name: name.to_string(),
            url: url.to_string(),
            format,
            mirrors: Vec::new(),
            verify_signatures: true,
            last_synced: 0,
            package_count: 0,
            components: Vec::new(),
        }
    }

    /// Builder: set provided component names.
    pub fn with_components(mut self, components: Vec<String>) -> Self {
        self.components = components;
        self
    }

    /// Validate the repository fields.
    pub fn validate(&self) -> Result<(), RepoError> {
        if self.name.trim().is_empty() {
            return Err(RepoError::Invalid("empty name".into()));
        }
        if self.url.trim().is_empty() {
            return Err(RepoError::Invalid("empty url".into()));
        }
        Ok(())
    }

    /// Builder: add mirror URLs.
    pub fn with_mirrors(mut self, mirrors: Vec<String>) -> Self {
        self.mirrors = mirrors;
        self
    }

    /// Builder: set signature verification policy.
    pub fn with_verify(mut self, verify: bool) -> Self {
        self.verify_signatures = verify;
        self
    }

    /// Whether the repo has been synced.
    pub fn is_synced(&self) -> bool {
        self.last_synced > 0
    }
}

fn infer_format(url: &str) -> Option<PackageFormat> {
    let u = url.to_lowercase();
    if u.contains(".deb") || u.contains("debian") || u.contains("ubuntu") {
        Some(PackageFormat::DebianDeb)
    } else if u.contains("rpm") || u.contains("fedora") || u.contains("centos") || u.contains("rhel") {
        Some(PackageFormat::Yum)
    } else if u.contains("arch") || u.contains("cachyos") || u.contains("manjaro") {
        Some(PackageFormat::Pacman)
    } else if u.contains("alpine") {
        Some(PackageFormat::Apk)
    } else if u.contains("flatpak") {
        Some(PackageFormat::Flatpak)
    } else if u.contains("snap") {
        Some(PackageFormat::Snap)
    } else if u.contains("nix") {
        Some(PackageFormat::Nix)
    } else if u.contains("sigma") {
        Some(PackageFormat::Sigma)
    } else {
        None
    }
}

/// Manages the set of configured repositories and their sync state.
#[derive(Debug, Default)]
pub struct RepositoryManager {
    repos: BTreeMap<String, Repository>,
}

impl RepositoryManager {
    pub fn new() -> Self { Self::default() }

    /// Add a repository; fails on duplicate name.
    pub fn add(&mut self, repo: Repository) -> Result<(), RepoError> {
        if self.repos.contains_key(&repo.name) {
            return Err(RepoError::Duplicate(repo.name.clone()));
        }
        self.repos.insert(repo.name.clone(), repo);
        Ok(())
    }

    /// Remove a repository by name.
    pub fn remove(&mut self, name: &str) -> Result<(), RepoError> {
        self.repos.remove(name).map(|_| ()).ok_or_else(|| RepoError::NotFound(name.to_string()))
    }

    /// Get a repository by name.
    pub fn get(&self, name: &str) -> Option<&Repository> {
        self.repos.get(name)
    }

    /// List all repositories sorted by name.
    pub fn list(&self) -> Vec<&Repository> {
        self.repos.values().collect()
    }

    /// Mark a repo synced now with `count` packages.
    pub fn mark_synced(&mut self, name: &str, package_count: usize) -> Result<(), RepoError> {
        let repo = self.repos.get_mut(name).ok_or_else(|| RepoError::NotFound(name.to_string()))?;
        repo.last_synced = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        repo.package_count = package_count;
        Ok(())
    }

    /// Total configured repositories.
    pub fn len(&self) -> usize { self.repos.len() }
    pub fn is_empty(&self) -> bool { self.repos.is_empty() }

    /// Alias for `add` used by CLI wiring.
    pub fn add_repository(&mut self, repo: Repository) -> Result<(), RepoError> {
        self.add(repo)
    }

    /// Alias for `list` returning owned clones for CLI iteration.
    pub fn list_repositories(&self) -> Vec<Repository> {
        self.repos.values().cloned().collect()
    }

    /// Pick the best mirror URL for `name`. Prefers the repo's own mirrors,
    /// else its primary URL. Errors if the repo is unknown.
    pub fn select_best_mirror(&self, name: &str) -> Result<String, RepoError> {
        let repo = self.repos.get(name).ok_or_else(|| RepoError::NotFound(name.to_string()))?;
        if let Some(m) = repo.mirrors.first() {
            return Ok(m.clone());
        }
        if !repo.url.is_empty() {
            return Ok(repo.url.clone());
        }
        Err(RepoError::Invalid(format!("no mirror for {name}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_list_remove() {
        let mut m = RepositoryManager::new();
        m.add(Repository::new("core", "https://repo.sigmaos.dev/core")).unwrap();
        m.add(Repository::new("extra", "https://ftp.debian.org/debian")).unwrap();
        assert_eq!(m.len(), 2);
        assert!(m.add(Repository::new("core", "https://x")).is_err()); // dup
        assert_eq!(m.list()[0].name, "core"); // BTreeMap sorted
        m.mark_synced("extra", 1234).unwrap();
        assert!(m.get("extra").unwrap().is_synced());
        m.remove("core").unwrap();
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn infers_format() {
        let r = Repository::new("deb", "https://deb.debian.org/debian");
        assert_eq!(r.format, PackageFormat::DebianDeb);
        let r2 = Repository::new("arch", "https://mirror.archlinux.org");
        assert_eq!(r2.format, PackageFormat::Pacman);
    }
}
