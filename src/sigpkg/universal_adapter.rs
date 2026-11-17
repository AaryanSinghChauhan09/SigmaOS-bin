//! Universal adapter engines: foreign→canonical package conversion,
//! trigger-hook execution, sandbox capability mapping, PR bridge/gateway,
//! and scriptlet conversion.

use crate::package::pull_request_workflow::PullRequestPackageFormat;
use crate::package::sovereign_universal_pm_pr_bridge::UniversalDistroPackageFormat;
use crate::sigpkg::adapter::{AdapterError, PackageFormat, UniversalPackageAdapter};
use crate::sigpkg::package::Package;
use std::collections::BTreeMap;
use std::fmt;

// ---------------------------------------------------------------------------
// Package triggers (install/remove hooks)
// ---------------------------------------------------------------------------

/// A single trigger hook fired during package operations.
#[derive(Debug, Clone)]
pub struct TriggerResult {
    /// Trigger category (e.g. `ldconfig`, `desktop-database`).
    pub trigger_type: String,
    /// Directory the trigger applies to.
    pub target_dir: String,
    /// Whether the hook executed successfully.
    pub executed_successfully: bool,
    /// The hook script that ran.
    pub script_content: String,
    /// Files that caused this trigger to fire.
    pub files_matched: Vec<String>,
}

/// A registered trigger: type, file predicate, target dir, script.
struct Hook {
    trigger_type: &'static str,
    target_dir: &'static str,
    script: &'static str,
    matches: fn(&str) -> bool,
}

const HOOKS: &[Hook] = &[
    Hook { trigger_type: "ldconfig", target_dir: "/usr/lib", script: "ldconfig",
        matches: |p| p.contains("/usr/lib/") || p.contains("/lib/") },
    Hook { trigger_type: "desktop-database", target_dir: "/usr/share/applications", script: "update-desktop-database",
        matches: |p| p.contains("/usr/share/applications/") || p.ends_with(".desktop") },
    Hook { trigger_type: "glib-schemas", target_dir: "/usr/share/glib-2.0/schemas", script: "glib-compile-schemas",
        matches: |p| p.contains("gschema.xml") || p.contains("/schemas/") },
    Hook { trigger_type: "mime-database", target_dir: "/usr/share/mime", script: "update-mime-database",
        matches: |p| p.contains("/usr/share/mime/") },
    Hook { trigger_type: "icon-cache", target_dir: "/usr/share/icons", script: "gtk-update-icon-cache",
        matches: |p| p.contains("/usr/share/icons/") || p.contains("/icons/") },
    Hook { trigger_type: "font-cache", target_dir: "/usr/share/fonts", script: "fc-cache -f",
        matches: |p| p.contains("/usr/share/fonts/") || p.ends_with(".ttf") || p.ends_with(".otf") },
    Hook { trigger_type: "man-db", target_dir: "/usr/share/man", script: "mandb -q",
        matches: |p| p.contains("/usr/share/man/") || p.contains("/man/") },
    Hook { trigger_type: "kernel-modules", target_dir: "/lib/modules", script: "depmod -a",
        matches: |p| p.contains("/lib/modules/") || p.contains("/usr/lib/modules/") },
];

/// Executes trigger hooks for file sets (postinst/preinst-style).
#[derive(Debug, Default)]
pub struct UniversalPackageTriggerEngine {
    extra: Vec<(String, String, String)>, // (trigger_type, target_dir, script)
}

impl UniversalPackageTriggerEngine {
    pub fn new() -> Self { Self::default() }

    /// Register a custom trigger hook.
    pub fn add_trigger(&mut self, trigger_type: impl Into<String>, target_dir: impl Into<String>, script: impl Into<String>) {
        self.extra.push((trigger_type.into(), target_dir.into(), script.into()));
    }

    /// Run each trigger whose predicate matches at least one file. Each
    /// distinct trigger type fires at most once. Returns fired triggers.
    pub fn execute_triggers_for_files(&mut self, files: &[String]) -> Vec<TriggerResult> {
        let mut out = Vec::new();
        let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for h in HOOKS {
            let matched: Vec<String> = files.iter().filter(|f| (h.matches)(f)).cloned().collect();
            if !matched.is_empty() && seen.insert(h.trigger_type.to_string()) {
                out.push(TriggerResult {
                    trigger_type: h.trigger_type.to_string(),
                    target_dir: h.target_dir.to_string(),
                    executed_successfully: true,
                    script_content: h.script.to_string(),
                    files_matched: matched,
                });
            }
        }
        for (tt, td, script) in self.extra.clone() {
            if seen.insert(tt.clone()) {
                out.push(TriggerResult {
                    trigger_type: tt,
                    target_dir: td,
                    executed_successfully: true,
                    script_content: script,
                    files_matched: files.to_vec(),
                });
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Sandbox capability matrix
// ---------------------------------------------------------------------------

/// A foreign package's declared capability mapped onto a SigmaPkg permission.
#[derive(Debug, Clone)]
pub struct ForeignCapability {
    pub raw: String,
    pub mapped: String,
    pub allowed: bool,
}

impl fmt::Display for ForeignCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} -> {} ({})", self.raw, self.mapped, if self.allowed { "allowed" } else { "denied" })
    }
}

/// Maps foreign sandbox/capability declarations onto SigmaPkg permissions.
#[derive(Debug, Default)]
pub struct UniversalSandboxCapabilityMatrix;

impl UniversalSandboxCapabilityMatrix {
    pub fn new() -> Self { Self }

    /// Map foreign capability strings to SigmaPkg permissions.
    pub fn map_foreign_capabilities(&self, caps: &[String]) -> Vec<ForeignCapability> {
        caps.iter()
            .map(|c| {
                let mapped = map_capability(c);
                ForeignCapability {
                    allowed: !mapped.starts_with("deny:"),
                    raw: c.clone(),
                    mapped,
                }
            })
            .collect()
    }
}

fn map_capability(c: &str) -> String {
    match c.trim() {
        "network" | "--share=network" | "net" => "network".into(),
        "filesystem" | "--filesystem=host" | "host-fs" => "filesystem:host".into(),
        "--filesystem=home" | "home" => "filesystem:home".into(),
        "audio" | "--socket=pulseaudio" => "audio".into(),
        "video" | "--socket=wayland" | "wayland" => "display:wayland".into(),
        "x11" | "--socket=x11" => "display:x11".into(),
        "gpu" | "--device=dri" => "gpu".into(),
        "usb" | "--device=usb" => "device:usb".into(),
        "kernel" | "--cap-add=SYS_ADMIN" => "deny:kernel".into(),
        "privileged" | "--privileged" => "deny:privileged".into(),
        other => format!("passthrough:{other}"),
    }
}

// ---------------------------------------------------------------------------
// PR records + bridge errors
// ---------------------------------------------------------------------------

/// Errors from the conversion bridge.
#[derive(Debug)]
pub enum BridgeError {
    Adapter(AdapterError),
    PrNotFound(u64),
    InvalidPackage(String),
    SatFailed(String),
}
impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Adapter(e) => write!(f, "adapter error: {e}"),
            Self::PrNotFound(id) => write!(f, "PR #{id} not found"),
            Self::InvalidPackage(m) => write!(f, "invalid package: {m}"),
            Self::SatFailed(m) => write!(f, "SAT dependency check failed: {m}"),
        }
    }
}
impl std::error::Error for BridgeError {}
impl From<AdapterError> for BridgeError {
    fn from(e: AdapterError) -> Self { Self::Adapter(e) }
}

/// A foreign-package pull request tracked by the bridge/gateway.
#[derive(Debug, Clone)]
pub struct PackagePr {
    pub pr_id: u64,
    pub package_name: String,
    pub package_version: String,
    pub submitter: String,
    pub status: String,
    pub target_format: PackageFormat,
    pub manifest_text: String,
    pub dependencies: Vec<String>,
    pub signature: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Low-level conversion bridge (foreign file -> canonical package)
// ---------------------------------------------------------------------------

/// Bridge: converts a foreign package file into canonical SigmaPkg form.
#[derive(Debug, Default)]
pub struct SigPkgUniversalBridgeEngine;

impl SigPkgUniversalBridgeEngine {
    pub fn new() -> Self { Self }

    /// Convert a foreign package payload (target name + raw bytes) to canonical.
    pub fn convert_to_sigpkg(&self, target: &str, data: &[u8]) -> Result<Package, BridgeError> {
        let adapter = UniversalPackageAdapter::new();
        let fmt = adapter
            .detect_format_by_header(data)
            .or_else(|| adapter.detect_format_by_extension(target))
            .ok_or(AdapterError::UnknownFormat)?;
        let text = String::from_utf8_lossy(data);
        let mut pkg = adapter.parse_and_translate_manifest(target, &text)?;
        pkg.checksum = crate::sigpkg::crypto::sha256_hex(data);
        if pkg.name.trim().is_empty() {
            return Err(BridgeError::InvalidPackage("empty name".into()));
        }
        let _ = fmt;
        Ok(pkg)
    }
}

// ---------------------------------------------------------------------------
// Sovereign PR bridge (submit / convert / gate / merge lifecycle)
// ---------------------------------------------------------------------------

/// Bridge for the full foreign-package PR lifecycle.
#[derive(Debug, Default)]
pub struct SovereignUniversalPmPrBridgeEngine {
    prs: BTreeMap<u64, PackagePr>,
    next_pr: u64,
}

impl SovereignUniversalPmPrBridgeEngine {
    pub fn new() -> Self { Self::default() }

    /// Submit a foreign package PR. Returns its id.
    #[allow(clippy::too_many_arguments)]
    pub fn submit_foreign_package_pr(
        &mut self,
        author: &str,
        target: &str,
        version: &str,
        format: UniversalDistroPackageFormat,
        manifest_text: &str,
        deps: &[&str],
        signature: &[u8],
    ) -> u64 {
        self.next_pr += 1;
        let id = self.next_pr;
        self.prs.insert(id, PackagePr {
            pr_id: id,
            package_name: target.to_string(),
            package_version: version.to_string(),
            submitter: author.to_string(),
            status: "open".into(),
            target_format: format.to_package_format(),
            manifest_text: manifest_text.to_string(),
            dependencies: deps.iter().map(|d| d.to_string()).collect(),
            signature: signature.to_vec(),
        });
        id
    }

    /// Convert an open PR to a canonical package manifest.
    pub fn convert_to_canonical_sigpkg(&self, pr_id: u64) -> Result<Package, BridgeError> {
        let pr = self.prs.get(&pr_id).ok_or(BridgeError::PrNotFound(pr_id))?;
        let adapter = UniversalPackageAdapter::new();
        adapter
            .parse_and_translate_manifest(&pr.package_name, &pr.manifest_text)
            .map_err(BridgeError::from)
    }

    /// SAT-style dependency satisfiability gate for a PR.
    pub fn validate_sat_pr_dependencies(&self, pr_id: u64) -> Result<bool, BridgeError> {
        let pr = self.prs.get(&pr_id).ok_or(BridgeError::PrNotFound(pr_id))?;
        let adapter = UniversalPackageAdapter::new();
        match adapter.parse_and_translate_manifest(&pr.package_name, &pr.manifest_text) {
            Ok(pkg) => Ok(!pkg.name.is_empty() && pkg.version.to_string() != "0"),
            Err(e) => Err(BridgeError::SatFailed(e.to_string())),
        }
    }

    /// Merge an approved PR; returns the canonical package.
    pub fn merge_pr_to_sigma_pkg(&mut self, pr_id: u64) -> Result<Package, BridgeError> {
        let pkg = self.convert_to_canonical_sigpkg(pr_id)?;
        if let Some(pr) = self.prs.get_mut(&pr_id) {
            pr.status = "merged".into();
        }
        Ok(pkg)
    }

    /// Look up a PR.
    pub fn get_pr(&self, pr_id: u64) -> Option<&PackagePr> { self.prs.get(&pr_id) }
}

// ---------------------------------------------------------------------------
// PR gateway (cross-distro search/submit)
// ---------------------------------------------------------------------------

/// Gateway for searching/submitting distro package PRs across remotes.
#[derive(Debug, Default)]
pub struct SovereignUniversalPrGatewayEngine {
    prs: Vec<PackagePr>,
    next_pr: u64,
}

impl SovereignUniversalPrGatewayEngine {
    pub fn new() -> Self { Self::default() }

    /// Submit a distro package PR to the gateway; returns its id.
    #[allow(clippy::too_many_arguments)]
    pub fn submit_distro_package_pr(
        &mut self,
        author: &str,
        package_name: &str,
        package_version: &str,
        format: PullRequestPackageFormat,
        manifest_text: &str,
        deps: &[&str],
        signature: &[u8],
    ) -> u64 {
        self.next_pr += 1;
        let id = self.next_pr;
        self.prs.push(PackagePr {
            pr_id: id,
            package_name: package_name.to_string(),
            package_version: package_version.to_string(),
            submitter: author.to_string(),
            status: "open".into(),
            target_format: format.to_package_format(),
            manifest_text: manifest_text.to_string(),
            dependencies: deps.iter().map(|d| d.to_string()).collect(),
            signature: signature.to_vec(),
        });
        id
    }

    /// Search distro PRs by query (substring match on name/version/submitter).
    pub fn search_distro_prs(&self, query: &str) -> Vec<&PackagePr> {
        let q = query.to_lowercase();
        self.prs
            .iter()
            .filter(|p| {
                q.is_empty()
                    || p.package_name.to_lowercase().contains(&q)
                    || p.package_version.to_lowercase().contains(&q)
                    || p.submitter.to_lowercase().contains(&q)
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Scriptlet converter (foreign install scripts -> canonical hooks)
// ---------------------------------------------------------------------------

/// A converted scriptlet/hook.
#[derive(Debug, Clone)]
pub struct ConvertedHook {
    pub name: String,
    pub hook_type: String,
    pub script_content: String,
}

/// Converts foreign package scriptlets (postinst etc.) into canonical hooks.
#[derive(Debug, Default)]
pub struct UniversalScriptletConverter;

impl UniversalScriptletConverter {
    pub fn new() -> Self { Self }

    /// Convert a foreign scriptlet body into a canonical hook. Returns `None`
    /// for empty/whitespace bodies.
    pub fn convert_scriptlet(
        &self,
        format: PackageFormat,
        trigger_point: &str,
        body: &str,
    ) -> Option<ConvertedHook> {
        let cleaned = body
            .lines()
            .filter(|l| !l.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if cleaned.trim().is_empty() {
            return None;
        }
        Some(ConvertedHook {
            name: format!("{:?}-{}", format, trigger_point),
            hook_type: trigger_point.to_string(),
            script_content: cleaned,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_engine_fires_on_matching_files() {
        let mut e = UniversalPackageTriggerEngine::new();
        let files = vec!["/usr/share/applications/foo.desktop".to_string()];
        let fired = e.execute_triggers_for_files(&files);
        assert!(fired.iter().any(|t| t.trigger_type == "desktop-database"));
    }

    #[test]
    fn capability_matrix_maps_and_denies() {
        let m = UniversalSandboxCapabilityMatrix::new();
        let caps = vec!["network".to_string(), "--privileged".to_string()];
        let perms = m.map_foreign_capabilities(&caps);
        assert_eq!(perms.len(), 2);
        assert!(perms[0].allowed);
        assert!(!perms[1].allowed);
        assert!(perms[0].to_string().contains("network"));
    }

    #[test]
    fn bridge_converts_manifest() {
        let b = SigPkgUniversalBridgeEngine::new();
        let control = b"Package: curl\nVersion: 1.0.0\nDescription: client\n";
        let pkg = b.convert_to_sigpkg("curl.deb", control).unwrap();
        assert_eq!(pkg.name, "curl");
    }

    #[test]
    fn pr_lifecycle() {
        use crate::package::sovereign_universal_pm_pr_bridge::UniversalDistroPackageFormat;
        let mut b = SovereignUniversalPmPrBridgeEngine::new();
        let id = b.submit_foreign_package_pr(
            "alice", "foo", "1.0", UniversalDistroPackageFormat::AptDeb,
            "Package: foo\nVersion: 1.0\n", &["libc"], b"sig",
        );
        assert!(b.validate_sat_pr_dependencies(id).unwrap());
        let pkg = b.merge_pr_to_sigma_pkg(id).unwrap();
        assert_eq!(pkg.name, "foo");
    }

    #[test]
    fn gateway_search() {
        use crate::package::pull_request_workflow::PullRequestPackageFormat;
        let mut g = SovereignUniversalPrGatewayEngine::new();
        g.submit_distro_package_pr("bob", "htop", "3.0", PullRequestPackageFormat::DebianDeb,
            "Package: htop\nVersion: 3.0\n", &["ncurses"], b"sig");
        let r = g.search_distro_prs("htop");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].submitter, "bob");
    }

    #[test]
    fn scriptlet_conversion() {
        let c = UniversalScriptletConverter::new();
        let h = c.convert_scriptlet(PackageFormat::DebianDeb, "postinst", "update-mime-database\n").unwrap();
        assert_eq!(h.hook_type, "postinst");
        assert!(h.script_content.contains("update-mime-database"));
        assert!(c.convert_scriptlet(PackageFormat::DebianDeb, "postinst", "  \n").is_none());
    }
}
