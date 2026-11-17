//! Package formats accepted by the pull-request workflow gateway.

use crate::sigpkg::adapter::PackageFormat;

/// A package format a distro PR can be submitted in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullRequestPackageFormat {
    DebianDeb,
    RedHatRpm,
    ArchPkg,
    AlpineApk,
    VoidXbps,
    OpenSuse,
    Gentoo,
    Nix,
    Guix,
    Flatpak,
    Snap,
}

impl PullRequestPackageFormat {
    /// Map to the canonical `PackageFormat`.
    pub fn to_package_format(self) -> PackageFormat {
        match self {
            Self::DebianDeb => PackageFormat::DebianDeb,
            Self::RedHatRpm => PackageFormat::Yum,
            Self::ArchPkg => PackageFormat::Pacman,
            Self::AlpineApk => PackageFormat::Apk,
            Self::VoidXbps => PackageFormat::Xbps,
            Self::OpenSuse => PackageFormat::Zypper,
            Self::Gentoo => PackageFormat::Portage,
            Self::Nix => PackageFormat::Nix,
            Self::Guix => PackageFormat::Guix,
            Self::Flatpak => PackageFormat::Flatpak,
            Self::Snap => PackageFormat::Snap,
        }
    }
}
