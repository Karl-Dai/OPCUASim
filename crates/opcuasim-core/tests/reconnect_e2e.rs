//! End-to-end: connection drops -> auto-reconnect -> subscription restored.

use std::sync::Arc;
use std::time::Duration;

use opcuasim_core::client::{ConnectionState, OpcUaConnection};
use opcuasim_core::config::ConnectionConfig;
use opcuasim_core::node::{AccessMode, MonitoredNode};
use opcuasim_core::server::models::{
    DataType, ServerConfig, ServerNode, SimulationMode,
};
use opcuasim_core::server::server::OpcUaServer;
use opcuasim_core::subscription::SubscriptionManager;

const PORT: u16 = 48420;

fn server_config(port: u16) -> ServerConfig {
    ServerConfig {
        name: "ReconnectE2E".into(),
        endpoint_url: format!("opc.tcp://127.0.0.1:{port}"),
        port,
        security_policies: vec!["None".into()],
        security_modes: vec!["None".into()],
        users: Vec::new(),
        anonymous_enabled: true,
        max_sessions: 10,
        max_subscriptions_per_session: 10,
    }
}

fn sine_node() -> ServerNode {
    ServerNode {
        node_id: "Demo.Sine".into(),
        display_name: "Sine".into(),
        parent_id: "i=85".into(),
        data_type: DataType::Double,
        writable: false,
        simulation: SimulationMode::Sine {
            amplitude: 10.0,
            offset: 0.0,
            period_ms: 4000,
            interval_ms: 100,
        },
        update_seq: 0,
        current_value: None,
        eu_range_low: 0.0,
        eu_range_high: 100.0,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reconnect_restores_subscription() {
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,opcua=warn"),
    )
    .is_test(true)
    .try_init();

    let server = Arc::new(OpcUaServer::new());
    server
        .start(&server_config(PORT), &[], &[sine_node()])
        .await
        .expect("server start");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // 1. Connect as master.
    let conn = Arc::new(OpcUaConnection::new(ConnectionConfig {
        id: "c1".into(),
        name: "c1".into(),
        endpoint_url: format!("opc.tcp://127.0.0.1:{PORT}"),
        security_policy: "None".into(),
        security_mode: "None".into(),
        auth: opcuasim_core::config::AuthConfig::Anonymous,
        timeout_ms: 5_000,
    }));
    conn.connect().await.expect("initial connect");
    assert_eq!(conn.get_state().await, ConnectionState::Connected);

    // 2. Subscribe to the sine node.
    let sub_mgr = SubscriptionManager::new();
    let session = conn.get_session().await.expect("session");
    let mut node = MonitoredNode::new(
        "ns=2;s=Demo.Sine".into(),
        "Sine".into(),
        String::new(),
        "Double".into(),
    );
    node.access_mode = AccessMode::Subscription { interval_ms: 200.0 };
    sub_mgr
        .add_nodes(vec![node], Some(&session))
        .await
        .expect("subscribe");

    // 3. Expect live updates.
    tokio::time::sleep(Duration::from_millis(800)).await;
    let seq_before = sub_mgr.get_update_seq().await;
    assert!(seq_before > 0, "expected data changes before server stop");

    // 4. Kill the server.
    server.stop().await.expect("server stop");
    tokio::time::sleep(Duration::from_millis(300)).await;

    // 5. Restart the server on the same port.
    let server2 = Arc::new(OpcUaServer::new());
    server2
        .start(&server_config(PORT), &[], &[sine_node()])
        .await
        .expect("server restart");
    tokio::time::sleep(Duration::from_millis(300)).await;

    // 6. Auto-reconnect loop: wait until Connected again (up to 10s).
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut reconnected = false;
    while tokio::time::Instant::now() < deadline {
        if conn.get_state().await == ConnectionState::Connected {
            reconnected = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert!(reconnected, "connection did not auto-reconnect");

    // 7. Recreate subscription manually (restore path) and expect updates again.
    let session2 = conn.get_session().await.expect("session after reconnect");
    let mut node2 = MonitoredNode::new(
        "ns=2;s=Demo.Sine".into(),
        "Sine".into(),
        String::new(),
        "Double".into(),
    );
    node2.access_mode = AccessMode::Subscription { interval_ms: 200.0 };
    sub_mgr
        .remove_nodes(&["ns=2;s=Demo.Sine".into()])
        .await
        .expect("remove old subscription");
    sub_mgr
        .add_nodes(vec![node2], Some(&session2))
        .await
        .expect("resubscribe");

    tokio::time::sleep(Duration::from_millis(800)).await;
    assert!(
        sub_mgr.get_update_seq().await > seq_before,
        "expected data changes after reconnect"
    );

    conn.disconnect().await.expect("disconnect");
    server2.stop().await.expect("server stop");
}
