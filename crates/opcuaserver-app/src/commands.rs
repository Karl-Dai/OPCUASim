use opcuasim_core::server::models::{
    DataType, ServerConfig, ServerFolder, ServerNode, ServerProjectFile, SimulationMode,
};
use serde::Deserialize;
use tauri::{AppHandle, Emitter, State};

use crate::state::{
    AddressSpace, AppState, FolderRow, NodeRow, ServerStatus, SimValue, SimValuesResponse,
};

/// Health-check command used by the frontend shell to prove the IPC bridge is
/// wired up end to end.
#[tauri::command]
pub fn ping() -> &'static str {
    "pong"
}

// ---------------------------------------------------------------------------
// Request payloads
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AddFolderRequest {
    pub node_id: String,
    pub display_name: String,
    pub parent_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AddNodeRequest {
    pub node_id: String,
    pub display_name: String,
    pub parent_id: String,
    pub data_type: DataType,
    pub writable: bool,
    pub simulation: SimulationMode,
    #[serde(default)]
    pub eu_range_low: f64,
    #[serde(default = "default_eu_range_high")]
    pub eu_range_high: f64,
}

fn default_eu_range_high() -> f64 {
    100.0
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateNodeRequest {
    pub node_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub data_type: Option<DataType>,
    #[serde(default)]
    pub writable: Option<bool>,
    #[serde(default)]
    pub simulation: Option<SimulationMode>,
    #[serde(default)]
    pub eu_range_low: Option<f64>,
    #[serde(default)]
    pub eu_range_high: Option<f64>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_address_space(state: &AppState) -> AddressSpace {
    let folders = state.folders.read().unwrap();
    let nodes = state.nodes.read().unwrap();
    AddressSpace {
        folders: folders
            .iter()
            .map(|f| FolderRow {
                node_id: f.node_id.clone(),
                display_name: f.display_name.clone(),
                parent_id: f.parent_id.clone(),
            })
            .collect(),
        nodes: nodes
            .iter()
            .map(|n| NodeRow {
                node_id: n.node_id.clone(),
                display_name: n.display_name.clone(),
                parent_id: n.parent_id.clone(),
                data_type: n.data_type.clone(),
                writable: n.writable,
                simulation: n.simulation.clone(),
                eu_range_low: n.eu_range_low,
                eu_range_high: n.eu_range_high,
            })
            .collect(),
    }
}

async fn compute_status(state: &AppState) -> ServerStatus {
    let st = state.server.state().await;
    let (node_count, folder_count, endpoint_url) = {
        let nodes = state.nodes.read().unwrap();
        let folders = state.folders.read().unwrap();
        let config = state.config.read().unwrap();
        (nodes.len(), folders.len(), config.endpoint_url.clone())
    };
    ServerStatus {
        state: format!("{st:?}"),
        node_count,
        folder_count,
        endpoint_url,
    }
}

async fn emit_server_state(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let status = compute_status(state).await;
    app.emit("server-state", &status).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Server lifecycle
// ---------------------------------------------------------------------------

pub(crate) async fn start_server_impl(state: &AppState) -> Result<(), String> {
    let (config, folders, nodes) = {
        (
            state.config.read().unwrap().clone(),
            state.folders.read().unwrap().clone(),
            state.nodes.read().unwrap().clone(),
        )
    };
    state
        .server
        .start(&config, &folders, &nodes)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_server(state: State<'_, AppState>, app_handle: AppHandle) -> Result<(), String> {
    start_server_impl(state.inner()).await?;
    emit_server_state(&app_handle, state.inner()).await
}

pub(crate) async fn stop_server_impl(state: &AppState) -> Result<(), String> {
    state.server.stop().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stop_server(state: State<'_, AppState>, app_handle: AppHandle) -> Result<(), String> {
    stop_server_impl(state.inner()).await?;
    emit_server_state(&app_handle, state.inner()).await
}

pub(crate) async fn refresh_status_impl(state: &AppState) -> Result<ServerStatus, String> {
    Ok(compute_status(state).await)
}

#[tauri::command]
pub async fn refresh_status(state: State<'_, AppState>) -> Result<ServerStatus, String> {
    refresh_status_impl(state.inner()).await
}

pub(crate) async fn refresh_address_space_impl(state: &AppState) -> Result<AddressSpace, String> {
    Ok(build_address_space(state))
}

#[tauri::command]
pub async fn refresh_address_space(state: State<'_, AppState>) -> Result<AddressSpace, String> {
    refresh_address_space_impl(state.inner()).await
}

#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<ServerConfig, String> {
    Ok(state.config.read().unwrap().clone())
}

// ---------------------------------------------------------------------------
// Address-space editing
// ---------------------------------------------------------------------------

pub(crate) async fn add_folder_impl(
    state: &AppState,
    request: AddFolderRequest,
) -> Result<AddressSpace, String> {
    let folder = ServerFolder {
        node_id: request.node_id,
        display_name: request.display_name,
        browse_name: None,
        parent_id: request.parent_id,
    };
    if let Some(nm) = state.server.node_manager().await {
        let ns = state.server.namespace_index().await;
        let custom_types = state.server.custom_types().await;
        let mut addr = nm.address_space().write();
        opcuasim_core::server::address_space::populate_address_space(
            &mut addr,
            ns,
            std::slice::from_ref(&folder),
            &[],
            &custom_types,
        );
    }
    state.folders.write().unwrap().push(folder);
    Ok(build_address_space(state))
}

#[tauri::command]
pub async fn add_folder(
    state: State<'_, AppState>,
    request: AddFolderRequest,
) -> Result<AddressSpace, String> {
    add_folder_impl(state.inner(), request).await
}

pub(crate) async fn add_node_impl(
    state: &AppState,
    request: AddNodeRequest,
) -> Result<AddressSpace, String> {
    let node = ServerNode {
        node_id: request.node_id,
        display_name: request.display_name,
        browse_name: None,
        parent_id: request.parent_id,
        data_type: request.data_type,
        writable: request.writable,
        simulation: request.simulation,
        eu_range_low: request.eu_range_low,
        eu_range_high: request.eu_range_high,
    };
    if let Some(nm) = state.server.node_manager().await {
        let ns = state.server.namespace_index().await;
        let custom_types = state.server.custom_types().await;
        let mut addr = nm.address_space().write();
        opcuasim_core::server::address_space::add_variable_node(
            &mut addr,
            ns,
            &node,
            &custom_types,
        );
    }
    state.nodes.write().unwrap().push(node);
    Ok(build_address_space(state))
}

#[tauri::command]
pub async fn add_node(
    state: State<'_, AppState>,
    request: AddNodeRequest,
) -> Result<AddressSpace, String> {
    add_node_impl(state.inner(), request).await
}

#[tauri::command]
pub async fn remove_node(
    state: State<'_, AppState>,
    node_id: String,
) -> Result<AddressSpace, String> {
    if let Some(nm) = state.server.node_manager().await {
        let ns = state.server.namespace_index().await;
        let mut addr = nm.address_space().write();
        opcuasim_core::server::address_space::remove_node(&mut addr, ns, &node_id);
    }
    state
        .folders
        .write()
        .unwrap()
        .retain(|f| f.node_id != node_id && f.parent_id != node_id);
    state
        .nodes
        .write()
        .unwrap()
        .retain(|n| n.node_id != node_id && n.parent_id != node_id);
    Ok(build_address_space(state.inner()))
}

#[tauri::command]
pub async fn update_node(
    state: State<'_, AppState>,
    request: UpdateNodeRequest,
) -> Result<AddressSpace, String> {
    {
        let mut nodes = state.nodes.write().unwrap();
        let Some(n) = nodes.iter_mut().find(|n| n.node_id == request.node_id) else {
            return Err(format!("节点 {} 未找到", request.node_id));
        };
        if let Some(display_name) = request.display_name {
            n.display_name = display_name;
        }
        if let Some(data_type) = request.data_type {
            n.data_type = data_type;
        }
        if let Some(writable) = request.writable {
            n.writable = writable;
        }
        if let Some(simulation) = request.simulation {
            n.simulation = simulation;
        }
        if let Some(low) = request.eu_range_low {
            n.eu_range_low = low;
        }
        if let Some(high) = request.eu_range_high {
            n.eu_range_high = high;
        }
    }
    Ok(build_address_space(state.inner()))
}

#[tauri::command]
pub async fn update_config(
    state: State<'_, AppState>,
    config: ServerConfig,
) -> Result<ServerConfig, String> {
    *state.config.write().unwrap() = config.clone();
    Ok(config)
}

// ---------------------------------------------------------------------------
// Project load / save
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn save_project(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let project = ServerProjectFile {
        project_type: "OpcUaServer".into(),
        version: "0.1.0".into(),
        server_config: state.config.read().unwrap().clone(),
        folders: state.folders.read().unwrap().clone(),
        nodes: state.nodes.read().unwrap().clone(),
    };
    let json = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn load_project(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    path: String,
) -> Result<(), String> {
    let json = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let project: ServerProjectFile = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    *state.config.write().unwrap() = project.server_config;
    *state.folders.write().unwrap() = project.folders;
    *state.nodes.write().unwrap() = project.nodes;
    emit_server_state(&app_handle, state.inner()).await
}

// ---------------------------------------------------------------------------
// Simulation value polling
// ---------------------------------------------------------------------------

pub(crate) async fn get_simulation_values_since_impl(
    state: &AppState,
    seq: u64,
) -> Result<SimValuesResponse, String> {
    let Some(engine) = state.server.simulation_engine().await else {
        return Ok(SimValuesResponse {
            seq,
            values: Vec::new(),
        });
    };
    let (values, current_seq) = engine.get_values_since(seq).await;
    Ok(SimValuesResponse {
        seq: current_seq,
        values: values
            .into_iter()
            .map(|(node_id, value)| SimValue { node_id, value })
            .collect(),
    })
}

#[tauri::command]
pub async fn get_simulation_values_since(
    state: State<'_, AppState>,
    seq: u64,
) -> Result<SimValuesResponse, String> {
    get_simulation_values_since_impl(state.inner(), seq).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use opcuasim_core::server::models::{
        DataType, ServerConfig, ServerFolder, ServerNode, SimulationMode,
    };
    use std::time::Duration;

    const TEST_PORT: u16 = 49502;

    fn ensure(cond: bool, msg: impl Into<String>) -> Result<(), String> {
        if cond {
            Ok(())
        } else {
            Err(msg.into())
        }
    }

    fn config(port: u16) -> ServerConfig {
        ServerConfig {
            name: "ServerAppE2E".into(),
            endpoint_url: format!("opc.tcp://127.0.0.1:{port}"),
            port,
            security_policies: vec!["None".into()],
            security_modes: vec!["None".into()],
            users: Vec::new(),
            anonymous_enabled: true,
            max_sessions: 10,
            max_subscriptions_per_session: 10,
            history_buffer_size: 10_000,
            event_history_size: 1_000,
            ..Default::default()
        }
    }

    fn node(id: &str, name: &str, simulation: SimulationMode, writable: bool) -> ServerNode {
        ServerNode {
            node_id: id.into(),
            display_name: name.into(),
            browse_name: None,
            parent_id: "i=85".into(),
            data_type: DataType::Double,
            writable,
            simulation,
            eu_range_low: 0.0,
            eu_range_high: 100.0,
        }
    }

    async fn run_server_scenario(state: &AppState) -> Result<(), String> {
        start_server_impl(state).await?;

        let mut running = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while tokio::time::Instant::now() < deadline {
            let status = refresh_status_impl(state).await?;
            if status.state == "Running" {
                ensure(status.node_count == 2, "node_count should be 2")?;
                ensure(status.folder_count == 1, "folder_count should be 1")?;
                ensure(
                    status.endpoint_url == format!("opc.tcp://127.0.0.1:{TEST_PORT}"),
                    "endpoint_url mismatch",
                )?;
                running = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        ensure(running, "server never reached Running")?;

        let added_folder = add_folder_impl(
            state,
            AddFolderRequest {
                node_id: "ExtraFolder".into(),
                display_name: "ExtraFolder".into(),
                parent_id: "i=85".into(),
            },
        )
        .await?;
        ensure(
            added_folder
                .folders
                .iter()
                .any(|f| f.node_id == "ExtraFolder"),
            "new folder should appear",
        )?;

        let added = add_node_impl(
            state,
            AddNodeRequest {
                node_id: "ExtraNode".into(),
                display_name: "ExtraNode".into(),
                parent_id: "ExtraFolder".into(),
                data_type: DataType::Double,
                writable: false,
                simulation: SimulationMode::Static {
                    value: "1.0".into(),
                },
                eu_range_low: 0.0,
                eu_range_high: 100.0,
            },
        )
        .await?;
        ensure(
            added.nodes.iter().any(|n| n.node_id == "ExtraNode"),
            "new node should appear",
        )?;

        let space = refresh_address_space_impl(state).await?;
        ensure(
            space.nodes.iter().any(|n| n.node_id == "ExtraNode"),
            "refresh_address_space should include the new node",
        )?;

        // Simulation values stream from the sine node.
        let mut values_seen = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while tokio::time::Instant::now() < deadline {
            let resp = get_simulation_values_since_impl(state, 0).await?;
            if resp.values.iter().any(|v| v.node_id == "Demo.Sine") {
                values_seen = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        ensure(values_seen, "simulation values never streamed")?;

        stop_server_impl(state).await?;

        let mut stopped = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while tokio::time::Instant::now() < deadline {
            let status = refresh_status_impl(state).await?;
            if status.state == "Stopped" {
                stopped = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        ensure(stopped, "server should reach Stopped after stop")?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_server_lifecycle_and_address_space() {
        let _ = env_logger::Builder::from_env(
            env_logger::Env::default().default_filter_or("info,opcua=warn"),
        )
        .is_test(true)
        .try_init();

        let state = AppState::new();
        *state.config.write().unwrap() = config(TEST_PORT);
        *state.folders.write().unwrap() = vec![ServerFolder {
            node_id: "MainFolder".into(),
            display_name: "MainFolder".into(),
            browse_name: None,
            parent_id: "i=85".into(),
        }];
        *state.nodes.write().unwrap() = vec![
            node(
                "Demo.Static",
                "Static",
                SimulationMode::Static {
                    value: "1.0".into(),
                },
                false,
            ),
            node(
                "Demo.Sine",
                "Sine",
                SimulationMode::Sine {
                    amplitude: 10.0,
                    offset: 0.0,
                    period_ms: 2000,
                    interval_ms: 100,
                },
                false,
            ),
        ];

        let outcome = run_server_scenario(&state).await;
        // Safety net: never leak the bound port on error paths.
        let _ = stop_server_impl(&state).await;

        if let Err(e) = outcome {
            panic!("server app e2e scenario failed: {e}");
        }
    }
}
