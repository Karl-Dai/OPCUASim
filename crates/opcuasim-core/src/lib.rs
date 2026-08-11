pub mod browse;
pub mod cert_manager;
pub mod client;
pub mod config; // ConnectionConfig, AuthConfig, ProjectFile (includes security config)
pub mod discovery;
pub mod error;
pub mod events;
pub mod history;
pub mod log_collector;
pub mod log_entry;
pub mod method;
pub mod node; // MonitoredNode, NodeGroup, BrowseResultItem, NodeAttributes
pub mod output;
pub mod polling;
pub mod reconnect;
pub mod server;
pub mod subscription; // OPC UA server simulation module
pub mod values;

/// Re-export the OPC UA Session type for downstream crates.
pub use opcua_client::Session as OpcUaSession;
