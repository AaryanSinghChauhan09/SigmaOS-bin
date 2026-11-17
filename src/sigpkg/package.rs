//! Package and dependency data model.

use serde::{Deserialize, Serialize};
use crate::sigpkg::version::{Version, VersionConstraint};

/// A single dependency edge: a package name plus a version constraint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub version_constraint: VersionConstraint,
}

impl Dependency {
    pub fn new(name: impl Into<String>, constraint: VersionConstraint) -> Self {
        Self { name: name.into(), version_constraint: constraint }
    }
}

/// A package record: identity, version, description, deps, checksum.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: Version,
    pub description: String,
    pub dependencies: Vec<Dependency>,
    pub checksum: String,
    /// Optional install size in bytes.
    pub installed_size: u64,
    /// Target architecture (`any`, `x86_64`, ...).
    pub architecture: String,
    /// Mirror URLs serving this package's payload.
    pub mirrors: Vec<String>,
}

impl Package {
    /// Create a package. `checksum` is the content hash of the payload.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: impl Into<String>,
        version: Version,
        description: impl Into<String>,
        dependencies: Vec<Dependency>,
        checksum: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            version,
            description: description.into(),
            dependencies,
            checksum: checksum.into(),
            installed_size: 0,
            architecture: "any".to_string(),
            mirrors: Vec::new(),
        }
    }

    /// Builder-style: set installed size.
    pub fn with_size(mut self, size: u64) -> Self {
        self.installed_size = size;
        self
    }

    /// Builder-style: set architecture.
    pub fn with_arch(mut self, arch: impl Into<String>) -> Self {
        self.architecture = arch.into();
        self
    }

    /// Builder-style: set mirror URLs.
    pub fn with_mirrors(mut self, mirrors: Vec<String>) -> Self {
        self.mirrors = mirrors;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_package_with_deps() {
        let deps = vec![
            Dependency::new("libc", VersionConstraint::Any),
            Dependency::new("ssl", VersionConstraint::parse(">=3.0").unwrap()),
        ];
        let p = Package::new("nginx", Version::parse("1.24.0").unwrap(), "web server", deps, "sha256-abc");
        assert_eq!(p.name, "nginx");
        assert_eq!(p.dependencies.len(), 2);
        assert_eq!(p.dependencies[1].version_constraint.to_string(), ">=3.0.0");
    }
}

impl std::fmt::Display for Package {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {} — {}", self.name, self.version, self.description)
    }
}
