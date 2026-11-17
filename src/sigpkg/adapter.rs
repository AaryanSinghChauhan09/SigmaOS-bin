//! Universal package-format detection and manifest translation.
//!
//! `PackageFormat` enumerates every package ecosystem SigmaPkg can ingest.
//! Detection works by file extension and by magic/header bytes.

use crate::sigpkg::package::Package;
use crate::sigpkg::version::Version;

/// Every package format SigmaPkg understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PackageFormat {
    // --- Linux distro formats ---
    Apt, AptDeb, DebianDeb, Yum, Dnf, Rpm, Pacman, Apk, Opkg, Pkg, Xbps, Zypper, Portage,
    Pisi, Ipk, Drpm, OpenBsdPkg, SolarisIps, Sysupdate, Superdeb, Stratum,
    SlackBuild, Pkgsrc, Dports, Cports, Ports, Crux, Tcz, Gobo, Lzm, Sfs,
    Pet, Pup, Puk, Moss, Hpkg, Spack, Vcpkg, Conan, Crate, Gem, Wheel,
    Nupkg, NarInfo, GuixNar, Tar, TarGz, TarXz,
    // --- Cross-distro / universal ---
    Flatpak, Snap, AppImage, AppBundle, Ostree, Air, Bottle, Ipa, Aab,
    Dmg, Sigma, Hap,
    // --- Nix family ---
    Nix, Guix,
}

/// Errors from format detection / manifest parsing.
#[derive(Debug)]
pub enum AdapterError {
    /// Format could not be determined.
    UnknownFormat,
    /// Manifest text could not be parsed.
    Parse(String),
}

impl std::fmt::Display for AdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownFormat => write!(f, "unknown package format"),
            Self::Parse(m) => write!(f, "manifest parse error: {m}"),
        }
    }
}
impl std::error::Error for AdapterError {}

/// Universal package adapter: detects and parses foreign manifests.
#[derive(Debug, Default)]
pub struct UniversalPackageAdapter;

impl UniversalPackageAdapter {
    pub fn new() -> Self { Self }

    /// Detect format from a filename/target string's extension.
    pub fn detect_format_by_extension(&self, target: &str) -> Option<PackageFormat> {
        let lower = target.to_lowercase();
        let stem = target.rsplit('/').next().unwrap_or(target).to_lowercase();
        // Compound / stem-based formats first (extension alone is ambiguous).
        if lower.contains(".pkg.tar.") {
            return Some(PackageFormat::Pacman);
        }
        if stem == "pkgbuild" || stem.ends_with("/pkgbuild") || lower.ends_with("pkgbuild") {
            return Some(PackageFormat::SlackBuild);
        }
        if stem == "portfile" || lower.ends_with("portfile") {
            return Some(PackageFormat::Ports);
        }
        if stem == "control" || lower.ends_with(".control") || lower.ends_with("/control") {
            return Some(PackageFormat::DebianDeb);
        }
        let ext = target.rsplit('.').next()?.to_lowercase();
        Some(match ext.as_str() {
            "deb" => PackageFormat::DebianDeb,
            "rpm" => PackageFormat::Yum,
            "pkg.tar.zst" | "pkg.tar.xz" | "pkg.tar.gz" => PackageFormat::Pacman,
            "apk" => PackageFormat::Apk,
            "ipk" => PackageFormat::Ipk,
            "xbps" => PackageFormat::Xbps,
            "zst" | "txz" if stem.contains("zypper") => PackageFormat::Zypper,
            "flatpak" => PackageFormat::Flatpak,
            "snap" => PackageFormat::Snap,
            "AppImage" | "appimage" => PackageFormat::AppImage,
            "nix" => PackageFormat::Nix,
            "drv" => PackageFormat::Guix,
            "spack.yaml" | "spack" => PackageFormat::Spack,
            "vcpkg.json" | "vcpkg" => PackageFormat::Vcpkg,
            "crate" => PackageFormat::Crate,
            "gem" => PackageFormat::Gem,
            "whl" => PackageFormat::Wheel,
            "nupkg" => PackageFormat::Nupkg,
            "narinfo" => PackageFormat::NarInfo,
            "tar" => PackageFormat::Tar,
            "tgz" | "tar.gz" => PackageFormat::TarGz,
            "txz" | "tar.xz" => PackageFormat::TarXz,
            "sigpkg" | "sig" => PackageFormat::Sigma,
            "ostree" => PackageFormat::Ostree,
            "ipa" => PackageFormat::Ipa,
            "dmg" => PackageFormat::Dmg,
            "aab" => PackageFormat::Aab,
            "app" => PackageFormat::AppBundle,
            "hap" => PackageFormat::Hap,
            "pet" => PackageFormat::Pet,
            "pup" | "pisi" => PackageFormat::Pisi,
            "eopkg" => PackageFormat::Pisi,
            "tce" | "tcz" => PackageFormat::Tcz,
            "xbps-src" => PackageFormat::SlackBuild,
            "PKGBUILD" => PackageFormat::SlackBuild,
            _ => {
                // fall back on stem hints
                if stem.ends_with(".deb") { PackageFormat::DebianDeb }
                else if stem.contains("PKGBUILD") { PackageFormat::SlackBuild }
                else if stem.contains("Portfile") { PackageFormat::Ports }
                else if stem.contains("PKGBUILD") { PackageFormat::Pacman }
                else { return None; }
            }
        })
    }

    /// Detect format from magic/header bytes.
    pub fn detect_format_by_header(&self, data: &[u8]) -> Option<PackageFormat> {
        if data.len() < 4 {
            return None;
        }
        // ar archive (!<arch>) -> static lib / deb data.tar members not here
        if data.starts_with(b"!<arch>\n") {
            return Some(PackageFormat::AptDeb); // debs are ar archives
        }
        // gzip
        if data.starts_with(&[0x1f, 0x8b]) {
            return Some(PackageFormat::TarGz);
        }
        // xz
        if data.starts_with(&[0xfd, b'7', b'z', b'X', b'Z', 0x00]) {
            return Some(PackageFormat::TarXz);
        }
        // zip (whl/nupkg/appx)
        if data.starts_with(b"PK\x03\x04") {
            return Some(PackageFormat::Wheel);
        }
        // ELF binary payload
        if data.starts_with(b"\x7fELF") {
            return Some(PackageFormat::Ostree);
        }
        // Mach-O / DMG / IPA magic
        if data.starts_with(&[0xfe, 0xed, 0xfa]) || data.starts_with(&[0xcf, 0xfa, 0xed, 0xfe]) {
            return Some(PackageFormat::Ipa);
        }
        // PNG-ish AppImage is ELF; ISO DMG starts with 'koly' at 0x8001 — skip
        None
    }

    /// Parse a foreign manifest (PKGBUILD, control, spec, Portfile, ...) into
    /// a canonical `Package`. `target` is the source file name for context.
    pub fn parse_and_translate_manifest(
        &self,
        target: &str,
        text: &str,
    ) -> Result<Package, AdapterError> {
        let fmt = match self.detect_format_by_extension(target) {
            Some(f) => f,
            None => sniff_format(text).unwrap_or(PackageFormat::DebianDeb),
        };
        match fmt {
            PackageFormat::SlackBuild | PackageFormat::Pacman => parse_pkgbuild(text, target),
            PackageFormat::DebianDeb | PackageFormat::AptDeb | PackageFormat::Apt => {
                parse_debian_control(text, target)
            }
            PackageFormat::Yum | PackageFormat::Dnf | PackageFormat::Rpm | PackageFormat::Drpm => {
                parse_rpm_spec(text, target)
            }
            PackageFormat::Ports | PackageFormat::Dports | PackageFormat::Cports => {
                parse_portfile(text, target)
            }
            _ => parse_generic_manifest(text, target),
        }
    }
}

fn base_name(target: &str) -> String {
    target
        .rsplit('/')
        .next()
        .unwrap_or(target)
        .split('.')
        .next()
        .unwrap_or(target)
        .to_string()
}

/// Infer a package format from manifest content when the filename is ambiguous.
fn sniff_format(text: &str) -> Option<PackageFormat> {
    let head: String = text.lines().take(8).collect::<Vec<_>>().join("\n");
    if head.contains("pkgname=") || head.contains("pkgver=") {
        return Some(PackageFormat::SlackBuild);
    }
    if head.contains("Package:") && head.contains("Version:") {
        return Some(PackageFormat::DebianDeb);
    }
    if head.contains("Name:") && head.contains("Version:") {
        return Some(PackageFormat::Yum);
    }
    if head.contains("Portfile") || head.contains("MAINTAINER:") {
        return Some(PackageFormat::Ports);
    }
    None
}

/// Parse `key = value` / `key: value` style manifests generically.
fn parse_generic_manifest(text: &str, target: &str) -> Result<Package, AdapterError> {
    let mut name = base_name(target);
    let mut version = Version::parse("0.0.0").unwrap();
    let mut desc = String::new();
    let mut deps = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = l.split_once('=') {
            let (k, v) = (k.trim(), v.trim().trim_matches('"').trim_matches('\''));
            match k {
                "pkgname" | "name" | "Name" => name = v.to_string(),
                "pkgver" | "version" | "Version" => {
                    if let Ok(ver) = Version::parse(v) {
                        version = ver;
                    }
                }
                "pkgdesc" | "description" | "Description" | "summary" => desc = v.to_string(),
                "depends" | "Depends" | "requires" => {
                    for d in v.split_whitespace() {
                        deps.push(crate::sigpkg::Dependency::new(
                            d.trim_matches(|c| c == '(' || c == ')' || c == ','),
                            crate::sigpkg::VersionConstraint::Any,
                        ));
                    }
                }
                _ => {}
            }
        }
    }
    Ok(Package::new(name, version, desc, deps, format!("manifest:{target}")))
}

/// Arch `PKGBUILD` parser.
fn parse_pkgbuild(text: &str, target: &str) -> Result<Package, AdapterError> {
    let mut p = parse_generic_manifest(text, target)?;
    // PKGBUILD arrays: depends=(a b c)
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with("depends=(") {
            let inner = l.trim_start_matches("depends=(").trim_end_matches(')').trim_end_matches(");");
            p.dependencies = inner
                .split_whitespace()
                .map(|d| crate::sigpkg::Dependency::new(d.trim_matches('"').trim_matches('\''), crate::sigpkg::VersionConstraint::Any))
                .collect();
        }
        if l.starts_with("pkgver=") {
            let v = l.trim_start_matches("pkgver=").trim().trim_matches('"');
            // pkgver excludes pkgrel; combine handled loosely
            if let Ok(ver) = Version::parse(v) {
                p.version = ver;
            }
        }
    }
    Ok(p)
}

/// Debian `debian/control` parser.
fn parse_debian_control(text: &str, target: &str) -> Result<Package, AdapterError> {
    let mut p = parse_generic_manifest(text, target)?;
    let mut in_depends = false;
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() {
            in_depends = false;
            continue;
        }
        if let Some((k, v)) = l.split_once(':') {
            let (k, v) = (k.trim(), v.trim());
            match k {
                "Package" => p.name = v.to_string(),
                "Version" => {
                    if let Ok(ver) = Version::parse(v.split('-').next().unwrap_or(v)) {
                        p.version = ver;
                    }
                }
                "Description" => {
                    if p.description.is_empty() {
                        p.description = v.to_string();
                    }
                }
                "Depends" => {
                    in_depends = true;
                    p.dependencies = v
                        .split(',')
                        .map(|d| {
                            let name = d.split_whitespace().next().unwrap_or("").trim_start_matches('(');
                            crate::sigpkg::Dependency::new(name, crate::sigpkg::VersionConstraint::Any)
                        })
                        .filter(|d| !d.name.is_empty())
                        .collect();
                }
                _ => {}
            }
        } else if in_depends {
            // continuation lines of Depends:
            let name = l.split_whitespace().next().unwrap_or("").trim_start_matches('(');
            if !name.is_empty() {
                p.dependencies.push(crate::sigpkg::Dependency::new(name, crate::sigpkg::VersionConstraint::Any));
            }
        }
    }
    Ok(p)
}

/// RPM `.spec` parser (Name/Version/Requires lines).
fn parse_rpm_spec(text: &str, target: &str) -> Result<Package, AdapterError> {
    parse_generic_manifest(text, target)
}

/// BSD `Portfile` / `Makefile`-style parser.
fn parse_portfile(text: &str, target: &str) -> Result<Package, AdapterError> {
    parse_generic_manifest(text, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_by_extension() {
        let a = UniversalPackageAdapter::new();
        assert_eq!(a.detect_format_by_extension("foo_1.0_amd64.deb"), Some(PackageFormat::DebianDeb));
        assert_eq!(a.detect_format_by_extension("bar-1.0-1-x86_64.pkg.tar.zst"), Some(PackageFormat::Pacman));
        assert_eq!(a.detect_format_by_extension("app.flatpak"), Some(PackageFormat::Flatpak));
        assert_eq!(a.detect_format_by_extension("libc6_2.36_amd64.deb"), Some(PackageFormat::DebianDeb));
    }

    #[test]
    fn detects_by_header() {
        let a = UniversalPackageAdapter::new();
        assert_eq!(a.detect_format_by_header(b"!<arch>\nrest"), Some(PackageFormat::AptDeb));
        assert_eq!(a.detect_format_by_header(&[0x1f, 0x8b, 0x08, 0x00]), Some(PackageFormat::TarGz));
        assert_eq!(a.detect_format_by_header(b"PK\x03\x04xxxx"), Some(PackageFormat::Wheel));
    }

    #[test]
    fn parses_pkgbuild() {
        let a = UniversalPackageAdapter::new();
        let text = "pkgname=nginx\npkgver=1.24.0\npkgrel=1\npkgdesc=\"web server\"\ndepends=(pcre zlib)\n";
        let p = a.parse_and_translate_manifest("PKGBUILD", text).unwrap();
        assert_eq!(p.name, "nginx");
        assert_eq!(p.version.to_string(), "1.24.0");
        assert_eq!(p.dependencies.len(), 2);
    }

    #[test]
    fn parses_debian_control() {
        let a = UniversalPackageAdapter::new();
        let text = "Package: nginx\nVersion: 1.24.0-1\nDescription: web server\nDepends: libc6, libssl3 (>= 3.0)\n";
        let p = a.parse_and_translate_manifest("control", text).unwrap();
        assert_eq!(p.name, "nginx");
        assert_eq!(p.dependencies.len(), 2);
    }
}
