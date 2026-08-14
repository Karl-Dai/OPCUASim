use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StructField {
    pub name: String,
    #[serde(rename = "type")]
    pub data_type: DataType,
}

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
    /// One-dimensional array of scalar elements (Task 7 will supply values).
    Array {
        #[serde(rename = "elementType")]
        element_type: Box<DataType>,
    },
    /// Two-dimensional array (matrix). `dims = [rows, cols]`.
    Array2D {
        #[serde(rename = "elementType")]
        element_type: Box<DataType>,
        dims: [u32; 2],
    },
    /// User-defined enumeration: `(value, display_name)` pairs.
    Enum {
        name: String,
        fields: Vec<(i64, String)>,
    },
    /// User-defined structure encoded as `ExtensionObject` (DynamicStructure +
    /// a registered binary encoding ID).
    Structure {
        name: String,
        fields: Vec<StructField>,
    },
}

impl DataType {
    /// Return the OPC UA DataTypeId numeric value (namespace 0) for the
    /// underlying scalar carried by this type.
    ///
    /// For complex types we follow the OPC UA type hierarchy:
    ///  - `Enum` values are transmitted as `Int32`
    ///  - `Structure` is transmitted as `ExtensionObject` whose encoding ID
    ///    maps back to the registered data type node (handled by
    ///    [`Self::type_node_id`]).
    ///  - `Array`/`Array2D` elements carry the scalar's DataTypeId.
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
            DataType::Array { element_type } | DataType::Array2D { element_type, .. } => {
                element_type.type_id()
            }
            DataType::Enum { .. } => 6,
            DataType::Structure { .. } => 22,
        }
    }

    /// Resolve this type to the full `NodeId` used as the `DataType` attribute
    /// of a `Variable` node. Scalar types map to namespace-0 `DataTypeId`
    /// values; complex types return the custom `NodeId` registered by
    /// [`super::address_space::register_custom_types`].
    pub fn type_node_id(
        &self,
        custom: &std::collections::HashMap<String, opcua_types::NodeId>,
    ) -> opcua_types::NodeId {
        match self {
            DataType::Enum { name, .. } | DataType::Structure { name, .. } => {
                if let Some(id) = custom.get(name) {
                    id.clone()
                } else {
                    opcua_types::NodeId::new(0, self.type_id())
                }
            }
            _ => opcua_types::NodeId::new(0, self.type_id()),
        }
    }

    /// Return the registration key used as the HashMap key inside
    /// [`super::address_space::register_custom_types`]. `None` for scalar
    /// and array types (those do not need custom DataType nodes).
    pub fn register_name(&self) -> Option<&str> {
        match self {
            DataType::Enum { name, .. } | DataType::Structure { name, .. } => Some(name.as_str()),
            _ => None,
        }
    }

    /// Whether this type requires a registered custom DataType node
    /// (`Enum` or `Structure`).
    pub fn is_custom(&self) -> bool {
        matches!(self, DataType::Enum { .. } | DataType::Structure { .. })
    }

    /// Whether this type's element type is itself a custom type (array of
    /// structures/enums). Those are not supported in this task.
    pub fn has_custom_element(&self) -> bool {
        match self {
            DataType::Array { element_type } | DataType::Array2D { element_type, .. } => {
                element_type.is_custom()
            }
            _ => false,
        }
    }

    /// Whether the type is a numeric type that can carry an EU Range.
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            DataType::Int16
                | DataType::Int32
                | DataType::Int64
                | DataType::UInt16
                | DataType::UInt32
                | DataType::UInt64
                | DataType::Float
                | DataType::Double
        )
    }
}

impl std::fmt::Display for DataType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataType::Enum { name, .. } => write!(f, "Enum({})", name),
            DataType::Structure { name, .. } => write!(f, "Structure({})", name),
            DataType::Array { element_type } => write!(f, "Array({:?})", element_type),
            DataType::Array2D {
                element_type, dims, ..
            } => write!(f, "Array2D({:?}{}x{})", element_type, dims[0], dims[1]),
            _ => write!(f, "{:?}", self),
        }
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
    Static {
        value: String,
    },
    Random {
        min: f64,
        max: f64,
        interval_ms: u64,
    },
    Sine {
        amplitude: f64,
        offset: f64,
        period_ms: u64,
        interval_ms: u64,
    },
    Linear {
        start: f64,
        step: f64,
        min: f64,
        max: f64,
        mode: LinearMode,
        interval_ms: u64,
    },
    Script {
        expression: String,
        interval_ms: u64,
    },
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
        SimulationMode::Static {
            value: "0".to_string(),
        }
    }
}

/// A variable node in the server address space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerNode {
    pub node_id: String,
    pub display_name: String,
    /// Optional OPC UA browse name. Defaults to the display name with spaces
    /// removed when not set.
    #[serde(default)]
    pub browse_name: Option<String>,
    pub parent_id: String,
    pub data_type: DataType,
    pub writable: bool,
    pub simulation: SimulationMode,
    /// EU Range property (low). Default 0.0; required for Percent deadband.
    #[serde(default)]
    pub eu_range_low: f64,
    /// EU Range property (high). Default 100.0; required for Percent deadband.
    #[serde(default = "default_eu_range_high")]
    pub eu_range_high: f64,
}

fn default_eu_range_high() -> f64 {
    100.0
}

/// A folder node in the server address space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerFolder {
    pub node_id: String,
    pub display_name: String,
    /// Optional OPC UA browse name. Defaults to the display name with spaces
    /// removed when not set.
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
    /// Bind host. Defaults to `127.0.0.1` (secure by default); use `0.0.0.0`
    /// to expose the server on the local network.
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
    /// Optional PEM/DER application certificate. When set together with
    /// `private_key_path`, the server uses it instead of an auto-generated
    /// self-signed keypair.
    #[serde(default)]
    pub certificate_path: Option<String>,
    /// Optional private key matching `certificate_path`.
    #[serde(default)]
    pub private_key_path: Option<String>,
    /// Automatically trust client certificates that are not in the trusted
    /// store. `true` keeps first-run local connections frictionless; disable
    /// for stricter production-style certificate validation.
    #[serde(default = "default_trust_client_certs")]
    pub trust_client_certs: bool,
    /// Per-node history ring buffer capacity. 0 disables history recording.
    #[serde(default = "default_history_buffer_size")]
    pub history_buffer_size: usize,
    /// Event history ring buffer capacity (per-source). 0 disables event recording.
    #[serde(default = "default_event_history_size")]
    pub event_history_size: usize,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_application_uri() -> String {
    "urn:opcuasim:server".to_string()
}

fn default_trust_client_certs() -> bool {
    true
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            name: "OPCUAServer Simulator".to_string(),
            application_uri: default_application_uri(),
            host: default_host(),
            endpoint_url: format!("opc.tcp://{}:4840", default_host()),
            port: 4840,
            security_policies: vec!["Basic256Sha256".to_string()],
            security_modes: vec!["SignAndEncrypt".to_string()],
            users: Vec::new(),
            anonymous_enabled: true,
            max_sessions: 100,
            max_subscriptions_per_session: 50,
            certificate_path: None,
            private_key_path: None,
            trust_client_certs: default_trust_client_certs(),
            history_buffer_size: default_history_buffer_size(),
            event_history_size: default_event_history_size(),
        }
    }
}

fn default_history_buffer_size() -> usize {
    10_000
}

fn default_event_history_size() -> usize {
    1_000
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
