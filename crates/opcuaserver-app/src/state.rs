use std::sync::{Arc, RwLock};

use opcuasim_core::server::models::{
    DataType, ServerConfig, ServerFolder, ServerNode, SimulationMode,
};
use opcuasim_core::server::server::OpcUaServer;
use serde::{Deserialize, Serialize};

/// Application state for the OPC UA server simulator.
///
/// Mirrors the legacy egui `BackendState`: a single `OpcUaServer` plus the
/// editable project model (config / folders / nodes). The model is kept in
/// `std::sync::RwLock`s exactly as the legacy backend did, because the
/// mutations never need to hold a guard across an `.await` point — the data is
/// cloned out first and the async work runs afterwards.
pub struct AppState {
    pub server: Arc<OpcUaServer>,
    pub config: RwLock<ServerConfig>,
    pub folders: RwLock<Vec<ServerFolder>>,
    pub nodes: RwLock<Vec<ServerNode>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            server: Arc::new(OpcUaServer::new()),
            config: RwLock::new(ServerConfig::default()),
            folders: RwLock::new(Vec::new()),
            nodes: RwLock::new(Vec::new()),
        }
    }
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }
}

// ---------------------------------------------------------------------------
// DTOs for API responses
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ServerStatus {
    pub state: String,
    pub node_count: usize,
    pub folder_count: usize,
    pub endpoint_url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AddressSpace {
    pub folders: Vec<FolderRow>,
    pub nodes: Vec<NodeRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FolderRow {
    pub node_id: String,
    pub display_name: String,
    pub parent_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NodeRow {
    pub node_id: String,
    pub display_name: String,
    pub parent_id: String,
    pub data_type: DataType,
    pub writable: bool,
    pub simulation: SimulationMode,
    pub current_value: Option<String>,
    pub eu_range_low: f64,
    pub eu_range_high: f64,
}

/// A single changed simulated value (node_id -> latest display string).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SimValue {
    pub node_id: String,
    pub value: String,
}

/// Response for incremental simulation-value polling.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SimValuesResponse {
    pub seq: u64,
    pub values: Vec<SimValue>,
}
