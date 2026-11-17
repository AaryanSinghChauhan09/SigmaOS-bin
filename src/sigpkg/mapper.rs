//! Cross-distro package-name canonicalization.
//!
//! Maps distro-specific package names (Debian `libc6`, Arch `glibc`, RPM
//! `glibc`, Alpine `musl`, etc.) onto SigmaPkg canonical names so dependency
//! resolution works across ecosystems.

use std::collections::HashMap;

/// Maps foreign package names to SigmaPkg canonical names.
#[derive(Debug)]
pub struct UniversalDependencyMapper {
    table: HashMap<String, String>,
}

impl UniversalDependencyMapper {
    pub fn new() -> Self {
        let mut table = HashMap::new();
        // libc family
        for n in ["libc6", "glibc", "glibc-locale", "musl", "musl-utils", "dietlibc"] {
            table.insert(n.to_string(), "libc".to_string());
        }
        // ssl/tls
        for n in ["libssl-dev", "libssl3", "openssl", "openssl-libs", "libressl"] {
            table.insert(n.to_string(), "openssl".to_string());
        }
        // common libs
        for n in ["zlib1g", "zlib", "zlib-ng", "minizip"] { table.insert(n.into(), "zlib".into()); }
        for n in ["libcurl4", "curl", "curl-minimal"] { table.insert(n.into(), "curl".into()); }
        for n in ["libxml2", "libxml2-dev", "expat"] { table.insert(n.into(), "libxml2".into()); }
        for n in ["sqlite", "sqlite-libs", "sqlite-dev", "libsqlite3-0"] { table.insert(n.into(), "sqlite".into()); }
        for n in ["python3", "python3-minimal", "python", "py3-"] { table.insert(n.into(), "python".into()); }
        for n in ["perl", "perl-base", "perl-interpreter"] { table.insert(n.into(), "perl".into()); }
        for n in ["git", "git-core", "git-minimal"] { table.insert(n.into(), "git".into()); }
        for n in ["bash", "bash-completion", "bash-static"] { table.insert(n.into(), "bash".into()); }
        for n in ["coreutils", "coreutils-single", "findutils", "sed", "grep", "gawk", "gawk-mpfr"] {
            table.insert(n.into(), n.trim_end_matches("-mpfr").to_string());
        }
        Self { table }
    }

    /// Map a foreign package name to its canonical SigmaPkg name.
    /// Unknown names pass through unchanged (already canonical). Common
    /// `-dev`/`-devel`/`-static`/... suffixes are stripped and re-mapped so
    /// e.g. `musl-dev` resolves via `musl` -> `libc`.
    pub fn to_canonical_name(&self, name: &str) -> String {
        if let Some(c) = self.table.get(name) {
            return c.clone();
        }
        for suf in [
            "-devel", "-dev", "-static", "-utils", "-minimal", "-libs", "-data",
            "-doc", "-dbg", "-common", "-lang", "-locale", "-bin", "-lib",
        ] {
            if let Some(base) = name.strip_suffix(suf) {
                if !base.is_empty() {
                    return self.to_canonical_name(base);
                }
            }
        }
        name.to_string()
    }

    /// Whether `name` is a known foreign alias.
    pub fn is_alias(&self, name: &str) -> bool {
        self.table.contains_key(name)
    }
}

impl Default for UniversalDependencyMapper {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_aliases() {
        let m = UniversalDependencyMapper::new();
        assert_eq!(m.to_canonical_name("libc6"), "libc");
        assert_eq!(m.to_canonical_name("glibc"), "libc");
        assert_eq!(m.to_canonical_name("musl"), "libc");
        assert_eq!(m.to_canonical_name("libssl-dev"), "openssl");
    }

    #[test]
    fn unknown_passes_through() {
        let m = UniversalDependencyMapper::new();
        assert_eq!(m.to_canonical_name("my-custom-pkg"), "my-custom-pkg");
        assert!(!m.is_alias("my-custom-pkg"));
    }
}
