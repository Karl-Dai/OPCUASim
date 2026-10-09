use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_store::StoreExt;
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::Mutex;

const STORE_FILE: &str = "update_state.json";
const KEY_LAST_CHECK: &str = "last_check_at";
const KEY_SKIPPED_VERSION: &str = "skipped_version";
const KEY_INSTALL_ON_NEXT_LAUNCH: &str = "install_on_next_launch";
const KEY_INSTALL_ID: &str = "install_id";
const THROTTLE_HOURS: i64 = 6;
const CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const DOWNLOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

#[derive(Clone, Serialize)]
struct UpdateProgress {
    stage: &'static str,
    downloaded: u64,
    total: Option<u64>,
}

fn progress(app: &AppHandle, stage: &'static str, downloaded: u64, total: Option<u64>) {
    let _ = app.emit(
        "update-progress",
        UpdateProgress {
            stage,
            downloaded,
            total,
        },
    );
}

#[derive(Serialize, Clone)]
pub struct UpdateMeta {
    pub version: String,
    pub notes: String,
    pub pub_date: Option<String>,
}

struct PreparedUpdate {
    meta: UpdateMeta,
    update: Update,
    bytes: Vec<u8>,
}

/// Serializes updater operations and keeps the signature-verified package in
/// memory until the user chooses what to do with it.
#[derive(Default)]
pub struct UpdateState {
    prepared: Mutex<Option<PreparedUpdate>>,
}

fn read_str(app: &AppHandle, key: &str) -> Option<String> {
    let store = app.store(STORE_FILE).ok()?;
    store.get(key).and_then(|v| v.as_str().map(String::from))
}

fn read_bool(app: &AppHandle, key: &str) -> bool {
    let Ok(store) = app.store(STORE_FILE) else {
        return false;
    };
    store.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
}

fn write_str(app: &AppHandle, key: &str, value: &str) {
    if let Ok(store) = app.store(STORE_FILE) {
        store.set(key, serde_json::Value::String(value.to_string()));
        let _ = store.save();
    }
}

fn write_bool(app: &AppHandle, key: &str, value: bool) {
    if let Ok(store) = app.store(STORE_FILE) {
        store.set(key, serde_json::Value::Bool(value));
        let _ = store.save();
    }
}

fn remove_value(app: &AppHandle, key: &str) {
    if let Ok(store) = app.store(STORE_FILE) {
        store.delete(key);
        let _ = store.save();
    }
}

fn parse_ts(s: Option<String>) -> Option<DateTime<Utc>> {
    s.and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

// 匿名安装 ID:首次启动生成 UUID 并持久化,随更新检查经 X-Install-Id 头
// 上报到 cn0 加速源做匿名活跃统计,不含任何个人信息。
fn install_id(app: &AppHandle) -> String {
    if let Some(id) = read_str(app, KEY_INSTALL_ID) {
        return id;
    }
    let id = uuid::Uuid::new_v4().to_string();
    write_str(app, KEY_INSTALL_ID, &id);
    id
}

fn build_updater(app: &AppHandle) -> Result<tauri_plugin_updater::Updater, String> {
    app.updater_builder()
        .timeout(std::time::Duration::from_secs(10))
        .header("X-Install-Id", install_id(app))
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())
}

fn update_meta(update: &Update) -> UpdateMeta {
    UpdateMeta {
        version: update.version.clone(),
        notes: update.body.clone().unwrap_or_default(),
        pub_date: update.date.map(|d| d.to_string()),
    }
}

async fn find_update(app: &AppHandle) -> Result<Option<Update>, String> {
    log::info!("checking OPCUAMaster release updates");
    progress(app, "checking", 0, None);
    tokio::time::timeout(CHECK_TIMEOUT, build_updater(app)?.check())
        .await
        .map_err(|_| "Update check timed out".to_string())?
        .map_err(|e| e.to_string())
}

async fn download_update(app: &AppHandle, update: &mut Update) -> Result<Vec<u8>, String> {
    update.timeout = Some(DOWNLOAD_TIMEOUT);
    let mut downloaded = 0;
    let mut last_emit = std::time::Instant::now();
    progress(app, "downloading", 0, None);
    tokio::time::timeout(
        DOWNLOAD_TIMEOUT,
        update.download(
            |chunk, total| {
                downloaded += chunk as u64;
                if last_emit.elapsed() >= std::time::Duration::from_millis(100)
                    || total == Some(downloaded)
                {
                    progress(app, "downloading", downloaded, total);
                    last_emit = std::time::Instant::now();
                }
            },
            || {
                progress(app, "verifying", 0, None);
                log::info!("update download finished; verifying release signature");
            },
        ),
    )
    .await
    .map_err(|_| "Update download timed out".to_string())?
    .map_err(|e| e.to_string())
}

// `force = true` (toolbar button) bypasses the 6h throttle and a skipped
// version. The command only returns metadata after the package is fully
// downloaded and signature-verified, so the frontend dialog always means
// "ready to install".
#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
    state: State<'_, UpdateState>,
    force: Option<bool>,
) -> Result<Option<UpdateMeta>, String> {
    let force = force.unwrap_or(false);

    // A choice made in the previous run is handled by the startup task. Do not
    // race it with the regular automatic check.
    if !force && read_bool(&app, KEY_INSTALL_ON_NEXT_LAUNCH) {
        return Ok(None);
    }

    let mut prepared = state.prepared.lock().await;
    if let Some(update) = prepared.as_ref() {
        return Ok(Some(update.meta.clone()));
    }

    let now = Utc::now();
    if !force {
        let last = parse_ts(read_str(&app, KEY_LAST_CHECK));
        if !should_check(last, now, Duration::hours(THROTTLE_HOURS)) {
            return Ok(None);
        }
    }
    // Surface fetch / parse / download failures to the caller so a manual
    // check can distinguish them from "already latest". Automatic checks show
    // a retryable toolbar status without interrupting collection with an alert.
    let Some(mut update) = find_update(&app).await? else {
        log::info!("OPCUAMaster is up to date");
        write_str(&app, KEY_LAST_CHECK, &now.to_rfc3339());
        return Ok(None);
    };

    if !force
        && is_skipped(
            read_str(&app, KEY_SKIPPED_VERSION).as_deref(),
            &update.version,
        )
    {
        write_str(&app, KEY_LAST_CHECK, &now.to_rfc3339());
        return Ok(None);
    }

    let meta = update_meta(&update);
    let bytes = download_update(&app, &mut update).await?;
    *prepared = Some(PreparedUpdate {
        meta: meta.clone(),
        update,
        bytes,
    });
    write_str(&app, KEY_LAST_CHECK, &now.to_rfc3339());
    progress(&app, "ready", 0, None);
    Ok(Some(meta))
}

/// Installs the already downloaded package. No network request is made here.
#[tauri::command]
pub async fn install_update(app: AppHandle, state: State<'_, UpdateState>) -> Result<(), String> {
    ensure_installable()?;
    let prepared = state.prepared.lock().await;
    let update = prepared
        .as_ref()
        .ok_or_else(|| "update package is not ready".to_string())?;

    progress(&app, "installing", 0, None);
    update
        .update
        .install(&update.bytes)
        .map_err(|e| e.to_string())?;
    remove_value(&app, KEY_SKIPPED_VERSION);
    remove_value(&app, KEY_INSTALL_ON_NEXT_LAUNCH);
    drop(prepared);
    app.restart()
}

/// Ignores this exact version during future automatic checks. A manual check
/// can still surface it, and a newer version is never hidden.
#[tauri::command]
pub async fn skip_update(
    app: AppHandle,
    state: State<'_, UpdateState>,
    version: String,
) -> Result<(), String> {
    let mut prepared = state.prepared.lock().await;
    if !prepared
        .as_ref()
        .is_some_and(|update| update.meta.version == version)
    {
        return Err("update package is not ready".to_string());
    }
    *prepared = None;
    write_str(&app, KEY_SKIPPED_VERSION, &version);
    remove_value(&app, KEY_INSTALL_ON_NEXT_LAUNCH);
    Ok(())
}

/// Records a one-shot choice. The next process launch re-downloads the package
/// so Tauri verifies its release signature again before installing it.
#[tauri::command]
pub async fn schedule_update_on_next_launch(
    app: AppHandle,
    state: State<'_, UpdateState>,
    version: String,
) -> Result<(), String> {
    ensure_installable()?;
    let prepared = state.prepared.lock().await;
    if !prepared
        .as_ref()
        .is_some_and(|update| update.meta.version == version)
    {
        return Err("update package is not ready".to_string());
    }
    write_bool(&app, KEY_INSTALL_ON_NEXT_LAUNCH, true);
    remove_value(&app, KEY_SKIPPED_VERSION);
    Ok(())
}

/// Called from Tauri setup on every launch. It is a no-op unless the user chose
/// "install on next launch" in the previous run.
pub async fn install_pending_update(app: AppHandle) -> Result<(), String> {
    if !read_bool(&app, KEY_INSTALL_ON_NEXT_LAUNCH) {
        return Ok(());
    }
    ensure_installable()?;

    let state = app.state::<UpdateState>();
    let mut prepared = state.prepared.lock().await;
    let Some(mut update) = find_update(&app).await? else {
        remove_value(&app, KEY_INSTALL_ON_NEXT_LAUNCH);
        return Ok(());
    };

    let meta = update_meta(&update);
    let bytes = download_update(&app, &mut update).await?;
    *prepared = Some(PreparedUpdate {
        meta,
        update,
        bytes,
    });
    let ready = prepared
        .as_ref()
        .expect("prepared update was just inserted");
    progress(&app, "installing", 0, None);
    ready
        .update
        .install(&ready.bytes)
        .map_err(|e| e.to_string())?;

    remove_value(&app, KEY_SKIPPED_VERSION);
    remove_value(&app, KEY_INSTALL_ON_NEXT_LAUNCH);
    drop(prepared);
    app.restart()
}

fn ensure_installable() -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Err(
            "Development builds cannot install release updates; use the packaged application"
                .into(),
        );
    }
    Ok(())
}

#[tauri::command]
pub fn can_install_update() -> bool {
    !cfg!(debug_assertions)
}

pub fn should_check(
    last_check: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    throttle: Duration,
) -> bool {
    match last_check {
        None => true,
        Some(last) => now - last >= throttle,
    }
}

pub fn is_skipped(skipped_version: Option<&str>, remote_version: &str) -> bool {
    skipped_version == Some(remote_version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_check_respects_throttle_boundary() {
        let now = Utc::now();
        let throttle = Duration::hours(THROTTLE_HOURS);
        assert!(!should_check(
            Some(now - throttle + Duration::seconds(1)),
            now,
            throttle
        ));
        assert!(should_check(Some(now - throttle), now, throttle));
    }

    #[test]
    fn skipped_version_only_hides_the_exact_release() {
        assert!(is_skipped(Some("1.2.3"), "1.2.3"));
        assert!(!is_skipped(Some("1.2.3"), "1.2.4"));
        assert!(!is_skipped(None, "1.2.3"));
    }
}
