//! End-to-end: server records simulated + externally written values into
//! history, readable via the client history_read_raw loop (with paging).

use std::sync::Arc;
use std::time::Duration;

use opcuasim_core::browse::write_node_value;
use opcuasim_core::client::OpcUaConnection;
use opcuasim_core::config::{AuthConfig, ConnectionConfig};
use opcuasim_core::history::history_read_raw;
use opcuasim_core::server::models::{DataType, ServerConfig, ServerNode, SimulationMode};
use opcuasim_core::server::server::OpcUaServer;

const PORT: u16 = 48430;

fn server_config() -> ServerConfig {
    ServerConfig {
        name: "HistoryE2E".into(),
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

fn sine_node() -> ServerNode {
    ServerNode {
        node_id: "Demo.Sine".into(),
        browse_name: None,
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

        eu_range_low: 0.0,
        eu_range_high: 100.0,
    }
}

fn writable_node() -> ServerNode {
    ServerNode {
        node_id: "Demo.Setpoint".into(),
        browse_name: None,
        display_name: "Setpoint".into(),
        parent_id: "i=85".into(),
        data_type: DataType::Double,
        writable: true,
        simulation: SimulationMode::Static { value: "0".into() },

        eu_range_low: 0.0,
        eu_range_high: 100.0,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn history_records_simulation_and_external_writes() {
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,opcua=warn"),
    )
    .is_test(true)
    .try_init();

    let server = Arc::new(OpcUaServer::new());
    server
        .start(&server_config(), &[], &[sine_node(), writable_node()])
        .await
        .expect("server start");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let conn = Arc::new(OpcUaConnection::new(ConnectionConfig {
        id: "h1".into(),
        name: "h1".into(),
        endpoint_url: format!("opc.tcp://127.0.0.1:{PORT}"),
        security_policy: "None".into(),
        security_mode: "None".into(),
        auth: AuthConfig::Anonymous,
        timeout_ms: 5_000,
    }));
    conn.connect().await.expect("connect");
    let session = conn.get_session().await.expect("session");

    // 1. Simulated node: wait ~1s, then read history — expect several samples.
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let sine_id: opcua_types::NodeId = "ns=2;s=Demo.Sine".parse().unwrap();
    // Use a fixed past date as start to avoid DateTime subtraction API uncertainty.
    let start = opcua_types::DateTime::ymd_hms(2026, 1, 1, 0, 0, 0);
    let now = opcua_types::DateTime::now();
    let points = history_read_raw(&session, &sine_id, start, now, 1000, false)
        .await
        .expect("history read");
    assert!(
        points.len() >= 3,
        "expected >=3 simulated history samples, got {}",
        points.len()
    );
    // timestamps monotonic
    let ts: Vec<&String> = points.iter().map(|p| &p.source_timestamp).collect();
    assert!(
        ts.windows(2).all(|w| w[0] <= w[1]),
        "timestamps must be monotonic"
    );

    // 2. External write: write Setpoint, then history must contain it.
    write_node_value(&session, "ns=2;s=Demo.Setpoint", "42.5", "Double")
        .await
        .expect("write");
    tokio::time::sleep(Duration::from_millis(500)).await;
    let sp_id: opcua_types::NodeId = "ns=2;s=Demo.Setpoint".parse().unwrap();
    let points2 = history_read_raw(
        &session,
        &sp_id,
        start,
        opcua_types::DateTime::now(),
        100,
        false,
    )
    .await
    .expect("history read setpoint");
    assert!(
        points2.iter().any(|p| p.value.contains("42.5")),
        "external write 42.5 must appear in history, got {:?}",
        points2.iter().map(|p| &p.value).collect::<Vec<_>>()
    );

    // 3. Paging: max_values=2 must cap the result (history_read_raw
    //    internally follows continuation points, then releases CP).
    let points3 = history_read_raw(
        &session,
        &sine_id,
        start,
        opcua_types::DateTime::now(),
        2,
        false,
    )
    .await
    .expect("paged history read");
    assert_eq!(points3.len(), 2, "max_values=2 must cap the result");

    conn.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
