//! Signed, background self-update support shared by both egui applications.
//!
//! Release manifests are signed with an Ed25519 private key held only by
//! GitHub Actions. The application embeds the matching public key, verifies
//! the manifest first, and then verifies the downloaded executable's SHA-256
//! before offering any update action to the user.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signature, VerifyingKey};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{settings, theme};

const REPO_OWNER: &str = "Karl-Dai";
const REPO_NAME: &str = "OPCUASim";
const CHECK_THROTTLE_SECS: u64 = 6 * 60 * 60;
const MAX_MANIFEST_BYTES: u64 = 512 * 1024;
const MAX_SIGNATURE_BYTES: u64 = 1024;

// Public half of the Ed25519 key whose private half is stored in the
// OPCUASIM_UPDATE_SIGNING_KEY GitHub Actions secret.
const UPDATE_PUBLIC_KEY: [u8; 32] = [
    0x65, 0x39, 0xa9, 0xa5, 0xd2, 0xef, 0x11, 0x37, 0x6e, 0x76, 0xe6, 0x55, 0x95, 0x1e, 0xcf, 0xd0,
    0x22, 0x7c, 0x17, 0x1f, 0xbd, 0x1b, 0xfc, 0x41, 0x4a, 0xf5, 0x53, 0xb3, 0xf8, 0x92, 0x68, 0xef,
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateManifest {
    pub version: String,
    pub notes: String,
    pub pub_date: String,
    pub asset: String,
    pub sha256: String,
}

#[derive(Debug, Clone)]
struct PreparedUpdate {
    manifest: UpdateManifest,
    binary_path: PathBuf,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct UpdatePrefs {
    #[serde(default)]
    last_check_unix: Option<u64>,
    #[serde(default)]
    skipped_version: Option<String>,
    #[serde(default)]
    pending_version: Option<String>,
}

enum UpdateEvent {
    Ready(PreparedUpdate),
    NoUpdate,
    Failed(String),
}

#[derive(Clone, Copy)]
enum UpdateChoice {
    InstallNow,
    Skip,
    InstallOnNextLaunch,
}

/// Owns the background update check and renders the ready-to-install modal.
pub struct UpdateController {
    app_id: &'static str,
    bin_name: &'static str,
    current_version: &'static str,
    events: Receiver<UpdateEvent>,
    prepared: Option<PreparedUpdate>,
    action_error: Option<String>,
}

impl UpdateController {
    pub fn new(
        ctx: egui::Context,
        app_id: &'static str,
        bin_name: &'static str,
        current_version: &'static str,
    ) -> Self {
        let (tx, events) = mpsc::channel();
        let controller = Self {
            app_id,
            bin_name,
            current_version,
            events,
            prepared: None,
            action_error: None,
        };

        // Never replace a cargo-built development executable. Released builds
        // run the exact same code path with debug_assertions disabled.
        if cfg!(debug_assertions) || platform_asset(bin_name).is_none() {
            return controller;
        }

        let prefs = load_prefs(app_id).unwrap_or_default();
        if !should_check(prefs.last_check_unix, now_unix(), CHECK_THROTTLE_SECS) {
            return controller;
        }

        let thread_name = format!("{app_id}-update-check");
        if let Err(error) = std::thread::Builder::new()
            .name(thread_name)
            .spawn(move || {
                let event = match check_and_prepare(app_id, bin_name, current_version) {
                    Ok(Some(update)) => UpdateEvent::Ready(update),
                    Ok(None) => UpdateEvent::NoUpdate,
                    Err(error) => UpdateEvent::Failed(error),
                };
                let _ = tx.send(event);
                ctx.request_repaint();
            })
        {
            log::warn!("failed to start update checker: {error}");
        }

        controller
    }

    pub fn poll(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                UpdateEvent::Ready(update) => self.prepared = Some(update),
                UpdateEvent::NoUpdate => {}
                UpdateEvent::Failed(error) => {
                    // Automatic checks are deliberately silent in the UI.
                    log::warn!("automatic update check failed: {error}");
                }
            }
        }
    }

    /// Displays a true egui modal only after download and verification finish.
    /// Backdrop and Escape dismissal deliberately mean "skip this version".
    pub fn show(&mut self, ctx: &egui::Context, allow_modal: bool) {
        if !allow_modal {
            return;
        }
        let Some(prepared) = self.prepared.clone() else {
            return;
        };

        let response = egui::Modal::new(egui::Id::new((self.app_id, "update-ready")))
            .frame(
                egui::Frame::popup(&ctx.global_style())
                    .fill(theme::BG_RAISED())
                    .stroke(egui::Stroke::new(1.0_f32, theme::BORDER()))
                    .inner_margin(egui::Margin::same(18)),
            )
            .show(ctx, |ui| {
                ui.set_min_width(520.0);
                ui.set_max_width(620.0);
                ui.heading("检测到新版本");
                ui.label(
                    egui::RichText::new(format!(
                        "v{} 已在后台下载并验证完成",
                        prepared.manifest.version
                    ))
                    .color(theme::STATUS_OK()),
                );
                ui.add_space(10.0);
                ui.label(egui::RichText::new("更新说明").strong());
                egui::Frame::default()
                    .fill(theme::BG_PANEL())
                    .stroke(egui::Stroke::new(1.0_f32, theme::BORDER()))
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(260.0)
                            .show(ui, |ui| {
                                ui.label(&prepared.manifest.notes);
                            });
                    });

                if let Some(error) = &self.action_error {
                    ui.add_space(8.0);
                    ui.colored_label(theme::STATUS_BAD(), format!("更新失败：{error}"));
                }

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let skip = ui.button("跳过此版本").clicked();
                    let next = ui.button("下次启动自动更新").clicked();
                    let now = ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("立即更新").color(theme::BG_BASE()),
                            )
                            .fill(theme::ACCENT()),
                        )
                        .clicked();
                    if now {
                        Some(UpdateChoice::InstallNow)
                    } else if next {
                        Some(UpdateChoice::InstallOnNextLaunch)
                    } else if skip {
                        Some(UpdateChoice::Skip)
                    } else {
                        None
                    }
                })
                .inner
            });

        let choice = response
            .inner
            .or_else(|| response.should_close().then_some(UpdateChoice::Skip));
        if let Some(choice) = choice {
            self.apply_choice(choice, prepared);
        }
    }

    fn apply_choice(&mut self, choice: UpdateChoice, prepared: PreparedUpdate) {
        self.action_error = None;
        let result = match choice {
            UpdateChoice::InstallNow => install_and_restart(
                self.app_id,
                self.bin_name,
                self.current_version,
                &prepared.manifest.version,
            ),
            UpdateChoice::Skip => {
                let result = skip_version(self.app_id, &prepared.manifest.version);
                if result.is_ok() {
                    self.prepared = None;
                    remove_cached_update(self.app_id, &prepared.manifest.version);
                }
                result
            }
            UpdateChoice::InstallOnNextLaunch => {
                let result = schedule_for_next_launch(
                    self.app_id,
                    self.bin_name,
                    &prepared.manifest.version,
                );
                if result.is_ok() {
                    self.prepared = None;
                }
                result
            }
        };
        if let Err(error) = result {
            self.action_error = Some(error);
        }
    }
}

/// Called before the native window is created. A pending, still-valid signed
/// package replaces the current executable and immediately starts the new one.
pub fn install_pending_update(
    app_id: &str,
    bin_name: &str,
    current_version: &str,
) -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Ok(());
    }
    let mut prefs = load_prefs(app_id)?;
    let Some(version) = prefs.pending_version.clone() else {
        return Ok(());
    };

    let remote = Version::parse(&version).map_err(|e| e.to_string())?;
    let current = Version::parse(current_version).map_err(|e| e.to_string())?;
    if remote <= current {
        prefs.pending_version = None;
        save_prefs(app_id, &prefs)?;
        return Ok(());
    }

    if let Err(error) = install_and_restart(app_id, bin_name, current_version, &version) {
        // A corrupt or missing cache must not trap the app in a failing startup
        // loop. Clear the pending choice and permit an immediate re-download.
        prefs.pending_version = None;
        prefs.last_check_unix = None;
        let _ = save_prefs(app_id, &prefs);
        return Err(error);
    }
    Ok(())
}

fn check_and_prepare(
    app_id: &str,
    bin_name: &str,
    current_version: &str,
) -> Result<Option<PreparedUpdate>, String> {
    let asset =
        platform_asset(bin_name).ok_or_else(|| "unsupported update platform".to_string())?;
    let mut prefs = load_prefs(app_id).unwrap_or_default();
    prefs.last_check_unix = Some(now_unix());
    save_prefs(app_id, &prefs)?;

    let manifest_name = format!("{asset}.update.json");
    let base = format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/releases/latest/download");
    let manifest_bytes = fetch_bytes(&format!("{base}/{manifest_name}"), MAX_MANIFEST_BYTES)?;
    let signature = fetch_bytes(&format!("{base}/{manifest_name}.sig"), MAX_SIGNATURE_BYTES)?;
    verify_manifest_signature(&manifest_bytes, &signature)?;

    let manifest: UpdateManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| format!("invalid update manifest: {e}"))?;
    validate_manifest(&manifest, &asset)?;

    let remote = Version::parse(&manifest.version).map_err(|e| e.to_string())?;
    let current = Version::parse(current_version).map_err(|e| e.to_string())?;
    if remote <= current || is_skipped(prefs.skipped_version.as_deref(), &manifest.version) {
        return Ok(None);
    }

    let dir = update_cache_dir(app_id, &manifest.version)?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let binary_path = dir.join(&manifest.asset);
    let binary_url = format!(
        "https://github.com/{REPO_OWNER}/{REPO_NAME}/releases/download/v{}/{}",
        manifest.version, manifest.asset
    );
    download_verified(&binary_url, &binary_path, &manifest.sha256)?;

    fs::write(dir.join("manifest.json"), &manifest_bytes).map_err(|e| e.to_string())?;
    fs::write(dir.join("manifest.sig"), &signature).map_err(|e| e.to_string())?;

    Ok(Some(PreparedUpdate {
        manifest,
        binary_path,
    }))
}

fn schedule_for_next_launch(app_id: &str, bin_name: &str, version: &str) -> Result<(), String> {
    let _ = load_prepared(app_id, bin_name, version)?;
    let mut prefs = load_prefs(app_id).unwrap_or_default();
    prefs.pending_version = Some(version.to_string());
    prefs.skipped_version = None;
    save_prefs(app_id, &prefs)
}

fn skip_version(app_id: &str, version: &str) -> Result<(), String> {
    Version::parse(version).map_err(|e| e.to_string())?;
    let mut prefs = load_prefs(app_id).unwrap_or_default();
    prefs.skipped_version = Some(version.to_string());
    prefs.pending_version = None;
    save_prefs(app_id, &prefs)
}

fn install_and_restart(
    app_id: &str,
    bin_name: &str,
    current_version: &str,
    version: &str,
) -> Result<(), String> {
    let remote = Version::parse(version).map_err(|e| e.to_string())?;
    let current = Version::parse(current_version).map_err(|e| e.to_string())?;
    if remote <= current {
        return Err("refusing to install a non-newer version".to_string());
    }

    let prepared = load_prepared(app_id, bin_name, version)?;
    self_replace::self_replace(&prepared.binary_path).map_err(|e| e.to_string())?;

    let mut prefs = load_prefs(app_id).unwrap_or_default();
    prefs.pending_version = None;
    prefs.skipped_version = None;
    save_prefs(app_id, &prefs)?;

    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Command::new(current_exe)
        .spawn()
        .map_err(|e| format!("updated but failed to restart: {e}"))?;
    std::process::exit(0)
}

fn load_prepared(app_id: &str, bin_name: &str, version: &str) -> Result<PreparedUpdate, String> {
    Version::parse(version).map_err(|e| e.to_string())?;
    let asset =
        platform_asset(bin_name).ok_or_else(|| "unsupported update platform".to_string())?;
    let dir = update_cache_dir(app_id, version)?;
    let manifest_bytes = fs::read(dir.join("manifest.json")).map_err(|e| e.to_string())?;
    let signature = fs::read(dir.join("manifest.sig")).map_err(|e| e.to_string())?;
    verify_manifest_signature(&manifest_bytes, &signature)?;
    let manifest: UpdateManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| format!("invalid update manifest: {e}"))?;
    validate_manifest(&manifest, &asset)?;
    if manifest.version != version {
        return Err("cached update version does not match pending version".to_string());
    }
    let binary_path = dir.join(&manifest.asset);
    let actual = sha256_file(&binary_path)?;
    if actual != manifest.sha256.to_ascii_lowercase() {
        return Err("cached update failed SHA-256 verification".to_string());
    }
    Ok(PreparedUpdate {
        manifest,
        binary_path,
    })
}

fn validate_manifest(manifest: &UpdateManifest, expected_asset: &str) -> Result<(), String> {
    Version::parse(&manifest.version).map_err(|e| format!("invalid update version: {e}"))?;
    if manifest.asset != expected_asset {
        return Err("signed manifest targets a different application or platform".to_string());
    }
    if manifest.sha256.len() != 64 || !manifest.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("signed manifest contains an invalid SHA-256 digest".to_string());
    }
    Ok(())
}

fn verify_manifest_signature(manifest: &[u8], signature: &[u8]) -> Result<(), String> {
    verify_manifest_signature_with_key(manifest, signature, &UPDATE_PUBLIC_KEY)
}

fn verify_manifest_signature_with_key(
    manifest: &[u8],
    signature: &[u8],
    public_key: &[u8; 32],
) -> Result<(), String> {
    let key = VerifyingKey::from_bytes(public_key).map_err(|e| e.to_string())?;
    let signature = Signature::from_slice(signature).map_err(|e| e.to_string())?;
    key.verify_strict(manifest, &signature)
        .map_err(|e| format!("update manifest signature verification failed: {e}"))
}

fn fetch_bytes(url: &str, max_bytes: u64) -> Result<Vec<u8>, String> {
    let response = http_agent()
        .get(url)
        .set("User-Agent", "OPCUASim-Updater")
        .call()
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > max_bytes {
        return Err("update metadata exceeded size limit".to_string());
    }
    Ok(bytes)
}

fn download_verified(url: &str, destination: &Path, expected_sha256: &str) -> Result<(), String> {
    let response = http_agent()
        .get(url)
        .set("User-Agent", "OPCUASim-Updater")
        .call()
        .map_err(|e| e.to_string())?;
    let part = destination.with_extension("download");
    let mut output = File::create(&part).map_err(|e| e.to_string())?;
    let mut reader = response.into_reader();
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read])
            .map_err(|e| e.to_string())?;
        hasher.update(&buffer[..read]);
    }
    output.sync_all().map_err(|e| e.to_string())?;
    let actual = format!("{:x}", hasher.finalize());
    if actual != expected_sha256.to_ascii_lowercase() {
        let _ = fs::remove_file(&part);
        return Err("downloaded update failed SHA-256 verification".to_string());
    }

    if destination.exists() {
        fs::remove_file(destination).map_err(|e| e.to_string())?;
    }
    fs::rename(&part, destination).map_err(|e| e.to_string())?;
    set_executable(destination)?;
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path).map_err(|e| e.to_string())?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).map_err(|e| e.to_string())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn http_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(120))
        .build()
}

fn platform_asset(bin_name: &str) -> Option<String> {
    let suffix = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "macos-aarch64"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "macos-x86_64"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "linux-x86_64"
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "windows-x86_64"
    } else {
        return None;
    };
    let extension = if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    };
    Some(format!("{bin_name}-{suffix}{extension}"))
}

fn prefs_path(app_id: &str) -> Result<PathBuf, String> {
    let dir =
        settings::data_dir().ok_or_else(|| "cannot determine update data directory".to_string())?;
    Ok(dir.join(format!("{app_id}-update.json")))
}

fn load_prefs(app_id: &str) -> Result<UpdatePrefs, String> {
    let path = prefs_path(app_id)?;
    if !path.exists() {
        return Ok(UpdatePrefs::default());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

fn save_prefs(app_id: &str, prefs: &UpdatePrefs) -> Result<(), String> {
    let path = prefs_path(app_id)?;
    let bytes = serde_json::to_vec(prefs).map_err(|e| e.to_string())?;
    fs::write(path, bytes).map_err(|e| e.to_string())
}

fn update_cache_dir(app_id: &str, version: &str) -> Result<PathBuf, String> {
    let version = Version::parse(version).map_err(|e| e.to_string())?;
    let dir =
        settings::data_dir().ok_or_else(|| "cannot determine update data directory".to_string())?;
    Ok(dir.join("updates").join(app_id).join(version.to_string()))
}

fn remove_cached_update(app_id: &str, version: &str) {
    if let Ok(path) = update_cache_dir(app_id, version) {
        let _ = fs::remove_dir_all(path);
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn should_check(last_check: Option<u64>, now: u64, throttle_secs: u64) -> bool {
    match last_check {
        None => true,
        Some(last) => now.saturating_sub(last) >= throttle_secs,
    }
}

pub fn is_skipped(skipped_version: Option<&str>, remote_version: &str) -> bool {
    skipped_version == Some(remote_version)
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signer, SigningKey};

    use super::*;

    #[test]
    fn throttle_uses_elapsed_time() {
        assert!(should_check(None, 100, 60));
        assert!(!should_check(Some(80), 100, 60));
        assert!(should_check(Some(40), 100, 60));
    }

    #[test]
    fn skipped_release_does_not_hide_a_newer_version() {
        assert!(is_skipped(Some("0.6.0"), "0.6.0"));
        assert!(!is_skipped(Some("0.6.0"), "0.6.1"));
    }

    #[test]
    fn verifies_ed25519_manifest_and_rejects_tampering() {
        let signing = SigningKey::from_bytes(&[7_u8; 32]);
        let message = br#"{"version":"0.6.0"}"#;
        let signature = signing.sign(message);
        let public = signing.verifying_key().to_bytes();

        assert!(
            verify_manifest_signature_with_key(message, &signature.to_bytes(), &public).is_ok()
        );
        assert!(
            verify_manifest_signature_with_key(b"tampered", &signature.to_bytes(), &public)
                .is_err()
        );
    }

    #[test]
    fn verifies_rfc8032_ed25519_vector() {
        let public = [
            0xd7, 0x5a, 0x98, 0x01, 0x82, 0xb1, 0x0a, 0xb7, 0xd5, 0x4b, 0xfe, 0xd3, 0xc9, 0x64,
            0x07, 0x3a, 0x0e, 0xe1, 0x72, 0xf3, 0xda, 0xa6, 0x23, 0x25, 0xaf, 0x02, 0x1a, 0x68,
            0xf7, 0x07, 0x51, 0x1a,
        ];
        let signature = [
            0xe5, 0x56, 0x43, 0x00, 0xc3, 0x60, 0xac, 0x72, 0x90, 0x86, 0xe2, 0xcc, 0x80, 0x6e,
            0x82, 0x8a, 0x84, 0x87, 0x7f, 0x1e, 0xb8, 0xe5, 0xd9, 0x74, 0xd8, 0x73, 0xe0, 0x65,
            0x22, 0x49, 0x01, 0x55, 0x5f, 0xb8, 0x82, 0x15, 0x90, 0xa3, 0x3b, 0xac, 0xc6, 0x1e,
            0x39, 0x70, 0x1c, 0xf9, 0xb4, 0x6b, 0xd2, 0x5b, 0xf5, 0xf0, 0x59, 0x5b, 0xbe, 0x24,
            0x65, 0x51, 0x41, 0x43, 0x8e, 0x7a, 0x10, 0x0b,
        ];

        assert!(verify_manifest_signature_with_key(b"", &signature, &public).is_ok());
    }

    #[test]
    fn manifest_is_bound_to_the_expected_platform_asset() {
        let manifest = UpdateManifest {
            version: "0.6.0".into(),
            notes: String::new(),
            pub_date: "2026-08-13T00:00:00Z".into(),
            asset: "OPCUAMaster-macos-aarch64".into(),
            sha256: "a".repeat(64),
        };
        assert!(validate_manifest(&manifest, "OPCUAMaster-macos-aarch64").is_ok());
        assert!(validate_manifest(&manifest, "OPCUAServer-macos-aarch64").is_err());
    }
}
