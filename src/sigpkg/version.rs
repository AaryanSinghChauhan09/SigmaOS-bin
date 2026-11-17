//! Semantic version parsing, ordering, and constraint matching.
//!
//! `Version` is `major.minor.patch` with optional pre-release/build metadata.
//! `VersionConstraint` expresses dependency requirements (`=`, `>`, `>=`,
//! `<`, `<=`, `*`).

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;

/// Errors produced while parsing a version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersionError {
    /// The version string was empty or malformed.
    Invalid(String),
}

impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(v) => write!(f, "invalid version: {v}"),
        }
    }
}
impl std::error::Error for VersionError {}

/// A parsed semantic version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// Optional pre-release tag (e.g. `-rc1`).
    pub prerelease: String,
    /// Optional build metadata (e.g. `+build5`).
    pub build: String,
}

impl Version {
    /// Parse `major[.minor[.patch]][-prerelease][+build]`.
    pub fn parse(s: &str) -> Result<Self, VersionError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(VersionError::Invalid(s.to_string()));
        }
        // Split build metadata first (+... is ignored for precedence).
        let (no_build, build) = match s.split_once('+') {
            Some((a, b)) => (a, b.to_string()),
            None => (s, String::new()),
        };
        // Split pre-release (-...).
        let (core, prerelease) = match no_build.split_once('-') {
            Some((a, b)) => (a, b.to_string()),
            None => (no_build, String::new()),
        };
        let mut parts = core.split('.');
        let major = parse_num(parts.next(), s)?;
        let minor = parse_num(parts.next(), s)?;
        let patch = parse_num(parts.next(), s)?;
        if parts.next().is_some() {
            return Err(VersionError::Invalid(s.to_string()));
        }
        Ok(Self { major, minor, patch, prerelease, build })
    }
}

fn parse_num(part: Option<&str>, orig: &str) -> Result<u64, VersionError> {
    match part {
        None => Ok(0),
        Some(p) => {
            if p.is_empty() || !p.chars().all(|c| c.is_ascii_digit()) {
                return Err(VersionError::Invalid(orig.to_string()));
            }
            p.parse::<u64>().map_err(|_| VersionError::Invalid(orig.to_string()))
        }
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch))
            .then_with(|| compare_prerelease(&self.prerelease, &other.prerelease))
    }
}
impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Pre-release precedence: a version WITHOUT prerelease > one WITH it;
/// numeric identifiers compare numerically, alphanumeric lexically.
fn compare_prerelease(a: &str, b: &str) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            let mut ai = a.split('.');
            let mut bi = b.split('.');
            loop {
                match (ai.next(), bi.next()) {
                    (None, None) => return Ordering::Equal,
                    (None, Some(_)) => return Ordering::Less,
                    (Some(_), None) => return Ordering::Greater,
                    (Some(x), Some(y)) => {
                        let ord = compare_ident(x, y);
                        if ord != Ordering::Equal {
                            return ord;
                        }
                    }
                }
            }
        }
    }
}

fn compare_ident(a: &str, b: &str) -> Ordering {
    let an = a.chars().all(|c| c.is_ascii_digit());
    let bn = b.chars().all(|c| c.is_ascii_digit());
    match (an, bn) {
        (true, true) => {
            let (x, y) = (a.trim_start_matches('0'), b.trim_start_matches('0'));
            x.len().cmp(&y.len()).then_with(|| x.cmp(y))
        }
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => a.cmp(b),
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.prerelease.is_empty() {
            write!(f, "-{}", self.prerelease)?;
        }
        if !self.build.is_empty() {
            write!(f, "+{}", self.build)?;
        }
        Ok(())
    }
}

/// A dependency version requirement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersionConstraint {
    Any,
    Exact(Version),
    GreaterThan(Version),
    GreaterOrEqual(Version),
    LessThan(Version),
    LessOrEqual(Version),
}

impl VersionConstraint {
    /// Parse a constraint spec like `=1.2.3`, `>=2.0`, `*`, `1.2.3`.
    pub fn parse(s: &str) -> Result<Self, VersionError> {
        let s = s.trim();
        if s.is_empty() || s == "*" {
            return Ok(Self::Any);
        }
        for (prefix, mk) in [
            (">=", Self::GreaterOrEqual as fn(Version) -> Self),
            ("<=", Self::LessOrEqual as fn(Version) -> Self),
            (">", Self::GreaterThan as fn(Version) -> Self),
            ("<", Self::LessThan as fn(Version) -> Self),
            ("=", Self::Exact as fn(Version) -> Self),
        ] {
            if let Some(rest) = s.strip_prefix(prefix) {
                return Ok(mk(Version::parse(rest)?));
            }
        }
        // Bare version means exact match.
        Ok(Self::Exact(Version::parse(s)?))
    }

    /// Does `version` satisfy this constraint?
    pub fn satisfies(&self, version: &Version) -> bool {
        match self {
            Self::Any => true,
            Self::Exact(v) => version == v,
            Self::GreaterThan(v) => version > v,
            Self::GreaterOrEqual(v) => version >= v,
            Self::LessThan(v) => version < v,
            Self::LessOrEqual(v) => version <= v,
        }
    }
}

impl fmt::Display for VersionConstraint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Any => write!(f, "*"),
            Self::Exact(v) => write!(f, "={v}"),
            Self::GreaterThan(v) => write!(f, ">{v}"),
            Self::GreaterOrEqual(v) => write!(f, ">={v}"),
            Self::LessThan(v) => write!(f, "<{v}"),
            Self::LessOrEqual(v) => write!(f, "<={v}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_versions() {
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!((v.major, v.minor, v.patch), (1, 2, 3));
        assert_eq!(v.to_string(), "1.2.3");
    }

    #[test]
    fn parses_partial_and_metadata() {
        assert_eq!(Version::parse("2").unwrap().minor, 0);
        let v = Version::parse("1.0.0-rc1+build").unwrap();
        assert_eq!(v.prerelease, "rc1");
        assert_eq!(v.build, "build");
    }

    #[test]
    fn orders_versions() {
        assert!(Version::parse("1.0.0").unwrap() < Version::parse("1.0.1").unwrap());
        assert!(Version::parse("1.2.0").unwrap() > Version::parse("1.1.9").unwrap());
        assert!(Version::parse("2.0.0").unwrap() < Version::parse("10.0.0").unwrap());
        // prerelease < release
        assert!(Version::parse("1.0.0-rc1").unwrap() < Version::parse("1.0.0").unwrap());
    }

    #[test]
    fn rejects_bad_versions() {
        assert!(Version::parse("").is_err());
        assert!(Version::parse("1.2.3.4").is_err());
        assert!(Version::parse("a.b.c").is_err());
    }

    #[test]
    fn constraints_parse_and_match() {
        let v = Version::parse("1.5.0").unwrap();
        assert!(VersionConstraint::parse("*").unwrap().satisfies(&v));
        assert!(VersionConstraint::parse(">=1.0").unwrap().satisfies(&v));
        assert!(!VersionConstraint::parse("<1.0").unwrap().satisfies(&v));
        assert!(VersionConstraint::parse("=1.5.0").unwrap().satisfies(&v));
        assert!(VersionConstraint::parse("1.5.0").unwrap().satisfies(&v)); // bare = exact
    }
}
