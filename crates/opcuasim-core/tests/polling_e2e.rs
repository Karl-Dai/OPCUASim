//! End-to-end: polling mode reads values from the server at interval.

use std::sync::Arc;
use std::time::Duration;

use opcuasim_core::client::OpcUaConnection;
use opcuasim_core::config::ConnectionConfig;
use opcuasim_core::node::{AccessMode, MonitoredNode};
use opcuasim_core::polling::PollingManager;
use opcuasim_core::server::models::{
    DataType, LinearMode, ServerConfig, ServerNode, SimulationMode,
};
use opcuasim_core::server::server::OpcUaServer;

const PORT: u16 = 48421;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn polling_reads_live_values() {
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,opcua=warn"),
    )
    .is_test(true)
    .try_init();

    let server = Arc::new(OpcUaServer::new());
    server
        .start(
            &ServerConfig {
                name: "PollingE2E".into(),
                endpoint_url: format!("opc.tcp://127.0.0.1:{PORT}"),
                port: PORT,
                security_policies: vec!["None".into()],
                security_modes: vec!["None".into()],
                users: Vec::new(),
                anonymous_enabled: true,
                max_sessions: 10,
                max_subscriptions_per_session: 10,
            },
            &[],
            &[ServerNode {
                node_id: "Demo.Ramp".into(),
                display_name: "Ramp".into(),
                parent_id: "i=85".into(),
                data_type: DataType::Double,
                writable: false,
                simulation: SimulationMode::Linear {
                    start: 0.0,
                    step: 1.0,
                    min: 0.0,
                    max: 100.0,
                    mode: LinearMode::Repeat,
                    interval_ms: 100,
                },
                update_seq: 0,
                current_value: None,
                eu_range_low: 0.0,
                eu_range_high: 100.0,
            }],
        )
        .await
        .expect("server start");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let conn = Arc::new(OpcUaConnection::new(ConnectionConfig {
        id: "p1".into(),
        name: "p1".into(),
        endpoint_url: format!("opc.tcp://127.0.0.1:{PORT}"),
        security_policy: "None".into(),
        security_mode: "None".into(),
        auth: opcuasim_core::config::AuthConfig::Anonymous,
        timeout_ms: 5_000,
    }));
    conn.connect().await.expect("connect");

    let poll_mgr = PollingManager::new(conn.get_session_holder());
    let mut node = MonitoredNode::new(
        "ns=2;s=Demo.Ramp".into(),
        "Ramp".into(),
        String::new(),
        "Double".into(),
    );
    node.access_mode = AccessMode::Polling { interval_ms: 100 };
    poll_mgr
        .add_polling_node(node, 100)
        .await
        .expect("add polling");

    // Wait ~1s: a 100ms ramp should have been read multiple times with distinct values.
    tokio::time::sleep(Duration::from_millis(1000)).await;

    let nodes = poll_mgr.get_polling_nodes().await;
    let node = nodes
        .iter()
        .find(|n| n.node_id == "ns=2;s=Demo.Ramp")
        .expect("polled node present");
    assert!(
        node.update_seq >= 3,
        "expected >=3 polling reads, got {}",
        node.update_seq
    );
    assert!(node.value.is_some(), "expected a value from polling read");
    assert!(
        node.timestamp.is_some(),
        "expected a timestamp from polling read"
    );

    conn.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
