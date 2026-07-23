use serde::{Deserialize, Serialize};

/// OPC UA data types supported by the simulation server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DataType {
    Boolean,
    Int16,
    Int32,
    Int64,
    UInt16,
    UInt32,
    UInt64,
    Float,
    Double,
    String,
    DateTime,
    ByteString,
}

impl DataType {
    /// Return the OPC UA DataTypeId numeric value (namespace 0).
    pub fn type_id(&self) -> u32 {
        match self {
            DataType::Boolean => 1,
            DataType::Int16 => 4,
            DataType::Int32 => 6,
            DataType::Int64 => 8,
            DataType::UInt16 => 5,
            DataType::UInt32 => 7,
            DataType::UInt64 => 9,
            DataType::Float => 10,
            DataType::Double => 11,
            DataType::String => 12,
            DataType::DateTime => 13,
            DataType::ByteString => 15,
        }
    }
}

impl std::fmt::Display for DataType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Linear mode: what happens when the value reaches max.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LinearMode {
    Repeat,
    Bounce,
}

/// Simulation mode for a server variable node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SimulationMode {
    Static { value: String },
    Random { min: f64, max: f64, interval_ms: u64 },
    Sine { amplitude: f64, offset: f64, period_ms: u64, interval_ms: u64 },
    Linear { start: f64, step: f64, min: f64, max: f64, mode: LinearMode, interval_ms: u64 },
    Script { expression: String, interval_ms: u64 },
}

impl SimulationMode {
    /// Get the update interval in ms (None for Static mode).
    pub fn interval_ms(&self) -> Option<u64> {
        match self {
            SimulationMode::Static { .. } => None,
            SimulationMode::Random { interval_ms, .. }
            | SimulationMode::Sine { interval_ms, .. }
            | SimulationMode::Linear { interval_ms, .. }
            | SimulationMode::Script { interval_ms, .. } => Some(*interval_ms),
        }
    }
}

impl Default for SimulationMode {
    fn default() -> Self {
        SimulationMode::Static { value: "0".to_string() }
    }
}

/// A variable node in the server address space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerNode {
    pub node_id: String,
    pub display_name: String,
    #[serde(default)]
    pub browse_name: Option<String>,
    pub parent_id: String,
    pub data_type: DataType,
    pub writable: bool,
    pub simulation: SimulationMode,
}

/// A folder node in the server address space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerFolder {
    pub node_id: String,
    pub display_name: String,
    #[serde(default)]
    pub browse_name: Option<String>,
    pub parent_id: String,
}

/// User role for access control.
///
/// NOTE: Roles are currently metadata only and not enforced as `UserAccessLevel`
/// on nodes. Full RBAC requires a custom async-opcua `AuthManager` (follow-up TODO).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UserRole {
    ReadOnly,
    ReadWrite,
    Admin,
}

/// A user account for server authentication.
///
/// NOTE: `password` is stored in plaintext in `.opcuaproj` files. Do not
/// distribute project files containing real credentials. Switching to argon2
/// hashing requires a custom async-opcua `AuthManager` (follow-up TODO).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAccount {
    pub username: String,
    pub password: String,
    pub role: UserRole,
}

/// Server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub name: String,
    #[serde(default = "default_application_uri")]
    pub application_uri: String,
    #[serde(default = "default_host")]
    pub host: String,
    pub endpoint_url: String,
    pub port: u16,
    pub security_policies: Vec<String>,
    pub security_modes: Vec<String>,
    pub users: Vec<UserAccount>,
    pub anonymous_enabled: bool,
    pub max_sessions: u32,
    pub max_subscriptions_per_session: u32,
    #[serde(default)]
    pub certificate_path: Option<String>,
    #[serde(default)]
    pub private_key_path: Option<String>,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_application_uri() -> String {
    "urn:opcuasim:server".to_string()
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            name: "OPCUAServer Simulator".to_string(),
            application_uri: "urn:opcuasim:server".to_string(),
            host: "127.0.0.1".to_string(),
            endpoint_url: "opc.tcp://127.0.0.1:4840".to_string(),
            port: 4840,
            security_policies: vec!["Basic256Sha256".to_string()],
            security_modes: vec!["SignAndEncrypt".to_string()],
            users: Vec::new(),
            anonymous_enabled: true,
            max_sessions: 100,
            max_subscriptions_per_session: 50,
            certificate_path: None,
            private_key_path: None,
        }
    }
}

/// Server lifecycle state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ServerState {
    Stopped,
    Starting,
    Running,
    Stopping,
}

/// Project file for saving/loading server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerProjectFile {
    pub project_type: String,
    pub version: String,
    pub server_config: ServerConfig,
    pub folders: Vec<ServerFolder>,
    pub nodes: Vec<ServerNode>,
}

impl Default for ServerProjectFile {
    fn default() -> Self {
        Self {
            project_type: "OpcUaServer".to_string(),
            version: "0.1.0".to_string(),
            server_config: ServerConfig::default(),
            folders: Vec::new(),
            nodes: Vec::new(),
        }
    }
}
