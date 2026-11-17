//! Foreign distro package formats accepted by the sovereign PM PR bridge.

use crate::sigpkg::adapter::PackageFormat;

/// A foreign distro package format a PR can target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UniversalDistroPackageFormat {
    AptDeb,
    YumRpm,
    Pacman,
    Apk,
    Xbps,
    Zypper,
    Portage,
    Flatpak,
    Snap,
    Nix,
    Guix,
    FreeBsdPkg,
    OpenBsdPkg,
    SlackBuild,
    Pkgsrc,
}

impl UniversalDistroPackageFormat {
    /// Map to the canonical `PackageFormat` used by the adapter.
    pub fn to_package_format(self) -> PackageFormat {
        match self {
            Self::AptDeb => PackageFormat::DebianDeb,
            Self::YumRpm => PackageFormat::Yum,
            Self::Pacman => PackageFormat::Pacman,
            Self::Apk => PackageFormat::Apk,
            Self::Xbps => PackageFormat::Xbps,
            Self::Zypper => PackageFormat::Zypper,
            Self::Portage => PackageFormat::Portage,
            Self::Flatpak => PackageFormat::Flatpak,
            Self::Snap => PackageFormat::Snap,
            Self::Nix => PackageFormat::Nix,
            Self::Guix => PackageFormat::Guix,
            Self::FreeBsdPkg => PackageFormat::Pkg,
            Self::OpenBsdPkg => PackageFormat::OpenBsdPkg,
            Self::SlackBuild => PackageFormat::SlackBuild,
            Self::Pkgsrc => PackageFormat::Pkgsrc,
        }
    }
}
