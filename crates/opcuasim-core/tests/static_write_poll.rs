//! e2e: client writes to a writable Static node must surface in the
//! simulation engine's `current_values` via the static-node polling task.
//! Static nodes have no auto-generation, so without the 500ms poll the
//! frontend would never see external writes.

use std::sync::Arc;
use std::time::Duration;

use opcuasim_core::browse::write_node_value;
use opcuasim_core::client::OpcUaConnection;
use opcuasim_core::config::{AuthConfig, ConnectionConfig};
use opcuasim_core::server::models::{DataType, ServerConfig, ServerNode, SimulationMode};
use opcuasim_core::server::server::OpcUaServer;

const PORT: u16 = 48444;

fn server_config() -> ServerConfig {
    ServerConfig {
        name: "StaticWritePollE2E".into(),
        endpoint_url: format!("opc.tcp://127.0.0.1:{PORT}"),
        port: PORT,
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

fn writable_static_node() -> ServerNode {
    ServerNode {
        node_id: "Demo.Setpoint".into(),
        display_name: "Setpoint".into(),
        browse_name: None,
        parent_id: "i=85".into(),
        data_type: DataType::Double,
        writable: true,
        simulation: SimulationMode::Static { value: "0".into() },
        eu_range_low: 0.0,
        eu_range_high: 100.0,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn static_node_client_writes_surface_to_engine() {
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,opcua=warn"),
    )
    .is_test(true)
    .try_init();

    let server = Arc::new(OpcUaServer::new());
    server
        .start(&server_config(), &[], &[writable_static_node()])
        .await
        .expect("server start");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let conn = Arc::new(OpcUaConnection::new(ConnectionConfig {
        id: "swp1".into(),
        name: "swp1".into(),
        endpoint_url: format!("opc.tcp://127.0.0.1:{PORT}"),
        security_policy: "None".into(),
        security_mode: "None".into(),
        auth: AuthConfig::Anonymous,
        timeout_ms: 5_000,
    }));
    conn.connect().await.expect("connect");
    let session = conn.get_session().await.expect("session");

    write_node_value(&session, "ns=2;s=Demo.Setpoint", "42.5", "Double")
        .await
        .expect("write");

    // The static poll task runs every 500ms; wait for two ticks plus margin.
    tokio::time::sleep(Duration::from_millis(1300)).await;

    let engine = server.simulation_engine().await.expect("engine");
    let (values, seq) = engine.get_values_since(0).await;
    assert!(seq >= 1, "update_seq must advance, got {seq}");
    let setpoint = values
        .iter()
        .find(|(nid, _)| nid == "Demo.Setpoint")
        .map(|(_, v)| v.clone());
    assert_eq!(
        setpoint.as_deref(),
        Some("42.5"),
        "static write must surface in current_values, got {:?}",
        values
    );

    conn.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
