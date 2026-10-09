use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use opcuasim_core::client::OpcUaConnection;
use opcuasim_core::config::{
    ConnectionConfig, ConnectionProjectEntry, MonitoredNodeConfig, ProjectFile,
};
use opcuasim_core::node::{AccessMode, MonitoredNode};
use opcuasim_core::polling::PollingManager;
use opcuasim_core::subscription::SubscriptionManager;
use serde::Serialize;
use uuid::Uuid;

use crate::state::{AppState, ConnectionEntry};

#[derive(Default)]
pub struct Persistence {
    path: Option<PathBuf>,
    error: Option<String>,
    // Never overwrite a failed restore unless its original bytes were backed up.
    blocked: bool,
}

#[derive(Serialize)]
pub struct PersistenceStatus {
    pub enabled: bool,
    pub error: Option<String>,
}

pub fn snapshot(state: &AppState) -> Result<ProjectFile, String> {
    let conns = state.connections.read().map_err(|e| e.to_string())?;
    let groups = state.groups.read().map_err(|e| e.to_string())?;
    let mut project = ProjectFile::new_master();
    project.connections = conns
        .values()
        .map(|entry| {
            let c = &entry.connection.config;
            ConnectionProjectEntry {
                id: c.id.clone(),
                name: entry.name.clone(),
                endpoint_url: c.endpoint_url.clone(),
                security_policy: c.security_policy.clone(),
                security_mode: c.security_mode.clone(),
                auth: c.auth.clone(),
                timeout_ms: c.timeout_ms,
                monitored_nodes: entry
                    .pending_subscriptions
                    .iter()
                    .chain(&entry.pending_polling)
                    .map(|node| MonitoredNodeConfig {
                        node_id: node.node_id.clone(),
                        display_name: node.display_name.clone(),
                        access_mode: node.access_mode.clone(),
                        group_id: node.group_id.clone(),
                        data_type: node.data_type.clone(),
                        browse_path: node.browse_path.clone(),
                        filter: node.filter,
                    })
                    .collect(),
            }
        })
        .collect();
    project.connections.sort_by(|a, b| a.id.cmp(&b.id));
    project.groups = groups.clone();
    Ok(project)
}

pub fn read(path: &Path) -> Result<ProjectFile, String> {
    let json = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let project = ProjectFile::from_json(&json).map_err(|e| e.to_string())?;
    if project.project_type != "OpcUaMaster" {
        return Err("Not an OPC UA master project".into());
    }
    for conn in &project.connections {
        for node in &conn.monitored_nodes {
            let valid_interval = match node.access_mode {
                AccessMode::Subscription { interval_ms } => {
                    interval_ms.is_finite() && interval_ms > 0.0
                }
                AccessMode::Polling { interval_ms } => interval_ms > 0,
            };
            if !valid_interval {
                return Err(format!("Invalid monitoring interval for {}", node.node_id));
            }
        }
    }
    Ok(project)
}

/// Called only at startup or after the old runtimes have been stopped.
pub fn apply(state: &AppState, project: ProjectFile) -> Result<(), String> {
    let mut restored = HashMap::new();
    for ce in project.connections {
        let id = if ce.id.is_empty() || restored.contains_key(&ce.id) {
            Uuid::new_v4().to_string()
        } else {
            ce.id
        };
        let config = ConnectionConfig {
            id: id.clone(),
            name: ce.name.clone(),
            endpoint_url: ce.endpoint_url,
            security_policy: ce.security_policy,
            security_mode: ce.security_mode,
            auth: ce.auth,
            timeout_ms: ce.timeout_ms,
        };
        let connection = Arc::new(OpcUaConnection::new(config));
        let polling_mgr = Arc::new(PollingManager::new(connection.get_session_holder()));
        let nodes = ce.monitored_nodes.into_iter().map(|saved| {
            let mut node = MonitoredNode::new(
                saved.node_id,
                saved.display_name,
                saved.browse_path,
                if saved.data_type.is_empty() {
                    "Unknown".into()
                } else {
                    saved.data_type
                },
            );
            node.access_mode = saved.access_mode;
            node.group_id = saved.group_id;
            node.filter = saved.filter;
            node
        });
        let (pending_subscriptions, pending_polling) =
            nodes.partition(|n| matches!(n.access_mode, AccessMode::Subscription { .. }));
        restored.insert(
            id,
            ConnectionEntry {
                name: ce.name,
                connection,
                subscription_mgr: SubscriptionManager::new(),
                polling_mgr,
                pending_subscriptions,
                pending_polling,
            },
        );
    }
    let mut conns = state.connections.write().map_err(|e| e.to_string())?;
    let mut groups = state.groups.write().map_err(|e| e.to_string())?;
    *conns = restored;
    *groups = project.groups;
    Ok(())
}

/// Write beside the destination and rename only after all bytes reach disk.
pub fn write_atomic(path: &Path, project: &ProjectFile) -> Result<(), String> {
    let json = project.to_json().map_err(|e| e.to_string())?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let tmp = parent.join(format!(".opcuamaster-{}.tmp", Uuid::new_v4()));
    let result = (|| -> std::io::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map_err(|e| e.to_string())
}

impl AppState {
    pub fn initialize_persistence(&self, path: PathBuf) -> Result<(), String> {
        let mut persistence = self.persistence.lock().map_err(|e| e.to_string())?;
        persistence.path = Some(path.clone());
        let result = match fs::metadata(&path) {
            Ok(_) => read(&path).and_then(|project| apply(self, project)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        };
        if let Err(error) = &result {
            let backup =
                path.with_file_name(format!("last-session.corrupt-{}.opcuaproj", Uuid::new_v4()));
            persistence.blocked = fs::copy(&path, &backup).is_err();
            persistence.error = Some(format!("Automatic restore failed: {error}"));
        }
        result
    }

    pub fn autosave(&self) -> Result<(), String> {
        // Serializes writes and takes each snapshot under the same lock, so an
        // older concurrent mutation cannot overwrite a newer snapshot.
        let mut persistence = self.persistence.lock().map_err(|e| e.to_string())?;
        let Some(path) = persistence.path.as_ref() else {
            return Ok(());
        };
        if persistence.blocked {
            return Err(
                "Automatic save disabled because the failed restore could not be backed up".into(),
            );
        }
        let result = snapshot(self).and_then(|project| write_atomic(path, &project));
        persistence.error = result.as_ref().err().cloned();
        result.map_err(|e| format!("Automatic save failed: {e}"))
    }

    pub fn persistence_status(&self) -> Result<PersistenceStatus, String> {
        let p = self.persistence.lock().map_err(|e| e.to_string())?;
        Ok(PersistenceStatus {
            enabled: p.path.is_some() && !p.blocked,
            error: p.error.clone(),
        })
    }

    pub fn report_persistence_error(&self, error: String) {
        if let Ok(mut p) = self.persistence.lock() {
            p.error = Some(error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("opcua-project-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn legacy_project_restores_without_new_fields() {
        let dir = test_dir();
        let path = dir.join("legacy.opcuaproj");
        fs::write(&path, r#"{"type":"OpcUaMaster","version":"0.1.0","connections":[{"name":"Legacy","endpoint_url":"opc.tcp://localhost:4840","security_policy":"None","security_mode":"None","auth":"Anonymous","timeout_ms":5000,"monitored_nodes":[{"node_id":"ns=2;s=Temperature","display_name":"Temperature","access_mode":{"Subscription":{"interval_ms":1000}},"group_id":null}]}],"groups":[]}"#).unwrap();
        let state = AppState::new();
        state.initialize_persistence(path).unwrap();
        let conns = state.connections.read().unwrap();
        let entry = conns.values().next().unwrap();
        assert!(!entry.connection.config.id.is_empty());
        assert_eq!(entry.pending_subscriptions.len(), 1);
        assert_eq!(entry.pending_subscriptions[0].data_type, "Unknown");
        assert!(entry.pending_subscriptions[0].filter.is_none());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn corrupted_restore_is_reported_and_original_is_backed_up() {
        let dir = test_dir();
        let path = dir.join("last-session.opcuaproj");
        fs::write(&path, "{broken").unwrap();
        let state = AppState::new();
        assert!(state.initialize_persistence(path.clone()).is_err());
        assert!(state.persistence_status().unwrap().error.is_some());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{broken");
        let backup = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("last-session.corrupt-")
            })
            .unwrap();
        assert_eq!(fs::read_to_string(backup).unwrap(), "{broken");
        state.autosave().unwrap();
        assert!(read(&path).is_ok());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failed_atomic_replace_leaves_existing_destination_and_reports_error() {
        let dir = test_dir();
        let path = dir.join("destination");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("keep"), "original").unwrap();
        let state = AppState::new();
        assert!(state.initialize_persistence(path.clone()).is_err());
        assert!(state.autosave().is_err());
        assert_eq!(fs::read_to_string(path.join("keep")).unwrap(), "original");
        assert!(fs::read_dir(&dir).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")));
        assert!(write_atomic(&path, &ProjectFile::new_master()).is_err());
        assert_eq!(fs::read_to_string(path.join("keep")).unwrap(), "original");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn successful_atomic_writes_are_private_and_replace_previous_snapshot() {
        let dir = test_dir();
        let path = dir.join("last-session.opcuaproj");
        write_atomic(&path, &ProjectFile::new_master()).unwrap();
        write_atomic(&path, &ProjectFile::new_master()).unwrap();
        assert!(read(&path).is_ok());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }
}
