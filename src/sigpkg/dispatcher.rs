//! Universal PM command dispatcher.
//!
//! Translates commands from ~120 foreign package managers (apt, dnf, pacman,
//! apk, emerge, zypper, brew, flatpak, nix, BSD pkg, etc.) into canonical
//! `DispatchedPmAction`s that SigmaPkg executes.

use crate::sigpkg::universal_engine::{DispatchedPmAction, UniversalPmOperation};

/// Errors from command dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchError {
    /// Empty command line.
    Empty,
    /// Unrecognized package-manager name.
    UnknownPm(String),
    /// Command had no operation subcommand.
    NoOperation,
}

impl std::fmt::Display for DispatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "empty command"),
            Self::UnknownPm(p) => write!(f, "unknown package manager: {p}"),
            Self::NoOperation => write!(f, "no operation subcommand"),
        }
    }
}
impl std::error::Error for DispatchError {}

/// Recognized foreign package-manager / distro-tool names.
const KNOWN_PMS: &[&str] = &[
    // Debian/Ubuntu family
    "apt", "apt-get", "aptitude", "apt-cache", "dpkg", "dpkg-query", "debian", "ubuntu",
    "mint", "pop", "devuan", "nala", "aura", "pkcon", "synaptic",
    // RPM/Fedora/RHEL family
    "dnf", "dnf5", "yum", "rpm", "microdnf", "fedora", "rhel", "centos", "rocky", "almalinux",
    "alma", "mageia", "openmandriva", "urpmi", "urpmq", "urpme", "smartos", "omnios", "illumos",
    "rpm-ostree",
    // Arch family
    "pacman", "yay", "paru", "trizen", "pikaur", "makepkg", "cachyos", "manjaro", "artix",
    "garuda", "endeavour", "arch", "cachy",
    // Alpine/BusyBox
    "apk", "alpine", "postmarketos",
    // openSUSE
    "zypper", "opensuse", "suse",
    // Void
    "xbps", "xbps-install", "xbps-query", "xbps-remove", "void",
    // Gentoo/Portage
    "emerge", "portage", "gentoo", "eix", "layman",
    // Nix
    "nix", "nix-env", "nix-shell", "nixos", "guix", "guixsd",
    // Flatpak/Snap/AppImage
    "flatpak", "snap", "appimage", "snapd",
    // BSD pkg
    "pkg", "pkg_add", "pkg_delete", "pkg_info", "pkgin", "freebsd", "openbsd", "netbsd",
    "ghostbsd", "dragonfly",
    // Slackware
    "slackpkg", "slapt-get", "slackware", "removepkg", "installpkg",
    // Solus
    "eopkg", "solus",
    // Others
    "brew", "pisi", "hpkg", "kiss", "swupd", "clearlinux", "pkgsrc", "poudriere",
    "cports", "dports", "crux", "installpkg", "spack", "vcpkg", "conan", "cargo",
    "pip", "pip3", "gem", "npm", "nuget", "wheel", "opkg", "moss", "bedrock", "ipkg",
    "pkgman", "pkgmk", "pkgrm", "pkginfo", "pkgsend", "pkg_add",
];

/// PMs where a bare package argument implies install (no subcommand needed).
const IMPLIES_INSTALL: &[&str] = &["emerge", "ebuild", "portage", "kiss", "swupd"];

/// Subcommand aliases → canonical operation.
fn operation_for(sub: &str) -> Option<UniversalPmOperation> {
    match sub {
        // install
        "install" | "add" | "in" | "i" | "get" | "fetch" | "sync-install" | "source-install" => {
            Some(UniversalPmOperation::Install)
        }
        // remove
        "remove" | "rm" | "del" | "delete" | "purge" | "erase" | "uninstall" | "deinstall" => {
            Some(UniversalPmOperation::Remove)
        }
        // upgrade / system upgrade
        "upgrade" | "update" | "dist-upgrade" | "full-upgrade" | "up" | "sysupgrade" | "-syu"
        | "-suy" | "-uy" | "world" | "world-update" => Some(UniversalPmOperation::Upgrade),
        // search
        "search" | "find" | "query" | "list" | "sl" | "apropos" | "search-file" => {
            Some(UniversalPmOperation::Search)
        }
        // info / show
        "info" | "show" | "describe" | "view" | "display" | "details" | "whatprovides" => {
            Some(UniversalPmOperation::QueryInfo)
        }
        // clean / cache clean
        "clean" | "autoclean" | "paccache" | "autoremove" | "cleanup" | "prune" | "gc" => {
            Some(UniversalPmOperation::CleanCache)
        }
        _ => None,
    }
}

/// Flags that mean "dry run / simulate" across PM dialects.
fn is_dry_run_flag(tok: &str) -> bool {
    matches!(
        tok,
        "--dry-run" | "--dryrun" | "--simulate" | "--no-act" | "--just-print" | "--print"
            | "--pretend" | "-p" | "-pv" | "-pva" | "-pvu" | "-pvs"
            | "-n" | "--assume-no" | "-s" | "--dry"
    )
}

/// The universal PM command dispatcher.
#[derive(Debug, Default)]
pub struct UniversalPmCommandDispatcher;

impl UniversalPmCommandDispatcher {
    pub fn new() -> Self { Self::default() }

    /// Translate a foreign PM command line into a canonical action.
    ///
    /// `command` is the full command *without* the leading program name of the
    /// host binary — e.g. `"apt install nginx -y"`, `"pacman -Syu --dryrun"`.
    pub fn dispatch_command(&self, command: &str) -> Result<DispatchedPmAction, DispatchError> {
        let tokens: Vec<&str> = command.split_whitespace().collect();
        if tokens.is_empty() {
            return Err(DispatchError::Empty);
        }
        let pm = tokens[0].to_lowercase();
        if !KNOWN_PMS.contains(&pm.as_str()) {
            return Err(DispatchError::UnknownPm(pm));
        }

        // Find the operation subcommand and collect target packages.
        let mut operation = None;
        let mut target_packages = Vec::new();
        let mut dry_run = false;
        let mut flags = Vec::new();
        let mut seen_op = false;

        for tok in &tokens[1..] {
            let t = tok.to_lowercase();
            if is_dry_run_flag(tok) {
                dry_run = true;
                continue;
            }
            if !seen_op {
                // Strip leading dashes for flag-like subcommands (pacman -S, -R).
                let sub = t.trim_start_matches('-');
                // pacman-style combined flags: -Syu -> upgrade, -S pkg -> install
                // Only dash-prefixed tokens are PM flags; bare words are
                // subcommands/targets (prevents "search"->CleanCache via 'c').
                if tok.starts_with('-') {
                    if let Some(op) = pacman_flag_op(&t) {
                        operation = Some(op);
                        seen_op = true;
                        continue;
                    }
                }
                if let Some(op) = operation_for(sub) {
                    operation = Some(op);
                    seen_op = true;
                    continue;
                }
                // Not a known subcommand — could be `pm pkgname` (install implied)
                // for PMs like `brew install` we still need a subcommand, so
                // treat first bare token as potential target only after op found.
                // For safety: unknown leading token -> no operation yet; keep scanning.
                // If it's clearly a package (no dash), hold as target.
                if !t.starts_with('-') {
                    // defer: might be target if op found later; store provisionally
                    target_packages.push(tok.to_string());
                } else {
                    flags.push(tok.to_string());
                }
                continue;
            }
            // After operation: flags vs targets.
            if t.starts_with('-') {
                if is_dry_run_flag(tok) {
                    dry_run = true;
                } else {
                    flags.push(tok.to_string());
                }
            } else {
                target_packages.push(tok.to_string());
            }
        }

        // If we collected provisional targets but never found an operation,
        // the first bare token was likely the subcommand we didn't recognize —
        // try interpreting the whole thing conservatively.
        let operation = match operation {
            Some(op) => op,
            None => {
                // PMs like emerge/ebuild imply install for bare package args.
                if IMPLIES_INSTALL.contains(&pm.as_str()) {
                    UniversalPmOperation::Install
                } else if tokens.len() > 1 {
                    let sub = tokens[1].to_lowercase();
                    let sub_clean = sub.trim_start_matches('-');
                    operation_for(sub_clean).ok_or(DispatchError::NoOperation)?
                } else {
                    return Err(DispatchError::NoOperation);
                }
            }
        };

        // If targets were collected provisionally before op and op is install-like,
        // they're valid targets. If op is search, they're the query.
        Ok(DispatchedPmAction {
            source_pm: pm,
            operation,
            target_packages,
            dry_run,
            flags,
        })
    }
}

/// pacman-style single-dash operation flags: `-S` install, `-R` remove,
/// `-Syu`/`-Su`/`-Syy` upgrade, `-Q` query, `-Ss` search, `-Sc`/`-Scc` clean.
fn pacman_flag_op(tok: &str) -> Option<UniversalPmOperation> {
    let t = tok.trim_start_matches('-').to_lowercase();
    if t.is_empty() {
        return None;
    }
    // Pacman op flags are a single letter class: -S/-R/-Q (plus modifiers).
    let first = t.chars().next()?;
    if !matches!(first, 's' | 'r' | 'q') {
        return None;
    }
    match first {
        'r' => Some(UniversalPmOperation::Remove), // -R, -Rs
        'q' => {
            if t.contains('s') {
                Some(UniversalPmOperation::Search) // -Qs
            } else {
                Some(UniversalPmOperation::QueryInfo) // -Q, -Qi, -Ql
            }
        }
        's' => {
            // upgrade: -Syu, -Syy, -Su (system sync/upgrade)
            if t.contains('y') || t.starts_with("su") {
                Some(UniversalPmOperation::Upgrade)
            // search: -Ss (double s)
            } else if t.contains("ss") {
                Some(UniversalPmOperation::Search)
            // clean: -Sc, -Scc
            } else if t.contains('c') {
                Some(UniversalPmOperation::CleanCache)
            } else {
                Some(UniversalPmOperation::Install) // -S pkg
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disp(cmd: &str) -> DispatchedPmAction {
        UniversalPmCommandDispatcher::new()
            .dispatch_command(cmd)
            .unwrap_or_else(|e| panic!("dispatch failed for {:?}: {:?}", cmd, e))
    }

    #[test]
    fn apt_install() {
        let a = disp("apt install nginx curl -y");
        assert_eq!(a.source_pm, "apt");
        assert_eq!(a.operation, UniversalPmOperation::Install);
        assert_eq!(a.target_packages, vec!["nginx", "curl"]);
        assert!(!a.dry_run);
    }

    #[test]
    fn dnf_remove() {
        let a = disp("dnf remove httpd");
        assert_eq!(a.operation, UniversalPmOperation::Remove);
        assert_eq!(a.target_packages, vec!["httpd"]);
    }

    #[test]
    fn pacman_upgrade_dryrun() {
        let a = disp("pacman -Syu --dryrun");
        assert_eq!(a.source_pm, "pacman");
        assert_eq!(a.operation, UniversalPmOperation::Upgrade);
        assert!(a.dry_run);
    }

    #[test]
    fn apk_add() {
        let a = disp("apk add musl");
        assert_eq!(a.operation, UniversalPmOperation::Install);
        assert_eq!(a.target_packages, vec!["musl"]);
    }

    #[test]
    fn bsd_pkg_dryrun() {
        let a = disp("pkg install -n postgresql15-server");
        assert_eq!(a.source_pm, "pkg");
        assert_eq!(a.operation, UniversalPmOperation::Install);
        assert!(a.dry_run);
        assert_eq!(a.target_packages, vec!["postgresql15-server"]);
    }

    #[test]
    fn distro_aliases() {
        for (cmd, pm) in [
            ("debian install nginx", "debian"),
            ("fedora install htop", "fedora"),
            ("arch install ripgrep", "arch"),
            ("freebsd install postgresql", "freebsd"),
            ("void install xbps", "void"),
            ("zypper in vlc", "zypper"),
            ("emerge -pv portage", "emerge"),
            ("flatpak install org.gimp.GIMP", "flatpak"),
            ("brew install wget", "brew"),
            ("alpine add musl", "alpine"),
            ("gentoo install portage", "gentoo"),
            ("opensuse in vlc", "opensuse"),
            ("nixos install firefox", "nixos"),
            ("nala install htop", "nala"),
            ("rpm-ostree install podman", "rpm-ostree"),
            ("ghostbsd install xfce4", "ghostbsd"),
            ("postmarketos add waybar", "postmarketos"),
            ("garuda -S hyprland", "garuda"),
        ] {
            let a = disp(cmd);
            assert_eq!(a.source_pm, pm, "cmd: {cmd}");
            assert_eq!(a.operation, UniversalPmOperation::Install, "cmd: {cmd}");
        }
    }

    #[test]
    fn search_and_info() {
        assert_eq!(disp("apt search nginx").operation, UniversalPmOperation::Search);
        assert_eq!(disp("apt show nginx").operation, UniversalPmOperation::QueryInfo);
        assert_eq!(disp("pacman -Ss vim").operation, UniversalPmOperation::Search);
        assert_eq!(disp("pacman -Qi bash").operation, UniversalPmOperation::QueryInfo);
    }

    #[test]
    fn unknown_pm_rejected() {
        assert!(UniversalPmCommandDispatcher::new().dispatch_command("notapm install x").is_err());
        assert!(UniversalPmCommandDispatcher::new().dispatch_command("").is_err());
    }
}
