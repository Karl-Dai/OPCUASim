//! End-to-end: server aggregate history (ReadProcessedDetails) — client
//! reads aggregated buckets via history_read_processed with CP paging.

use std::sync::Arc;
use std::time::Duration;

use opcuasim_core::client::OpcUaConnection;
use opcuasim_core::config::{AuthConfig, ConnectionConfig};
use opcuasim_core::history::history_read_processed;
use opcuasim_core::server::models::{DataType, ServerConfig, ServerNode, SimulationMode};
use opcuasim_core::server::server::OpcUaServer;

fn server_config(port: u16) -> ServerConfig {
    ServerConfig {
        name: "AggregateE2E".into(),
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
    }
}

fn sine_node() -> ServerNode {
    ServerNode {
        node_id: "Demo.SineAgg".into(),
        display_name: "SineAgg".into(),
        parent_id: "i=85".into(),
        data_type: DataType::Double,
        writable: false,
        simulation: SimulationMode::Sine {
            amplitude: 10.0,
            offset: 0.0,
            period_ms: 4000,
            interval_ms: 200,
        },
        update_seq: 0,
        current_value: None,
        eu_range_low: 0.0,
        eu_range_high: 100.0,
    }
}

async fn connect_and_wait(
    port: u16,
    client_id: &str,
    wait_ms: u64,
) -> (Arc<OpcUaConnection>, Arc<opcua_client::Session>) {
    let conn = Arc::new(OpcUaConnection::new(ConnectionConfig {
        id: client_id.into(),
        name: client_id.into(),
        endpoint_url: format!("opc.tcp://127.0.0.1:{port}"),
        security_policy: "None".into(),
        security_mode: "None".into(),
        auth: AuthConfig::Anonymous,
        timeout_ms: 5_000,
    }));
    conn.connect().await.expect("connect");
    let session = conn.get_session().await.expect("session");
    tokio::time::sleep(Duration::from_millis(wait_ms)).await;
    (conn, session)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aggregate_average_returns_buckets_in_range() {
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,opcua=warn"),
    )
    .is_test(true)
    .try_init();

    const PORT: u16 = 48431;

    let server = Arc::new(OpcUaServer::new());
    server
        .start(&server_config(PORT), &[], &[sine_node()])
        .await
        .expect("server start");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let (_conn, session) = connect_and_wait(PORT, "agg1", 3000).await;

    let sine_id: opcua_types::NodeId = "ns=2;s=Demo.SineAgg".parse().unwrap();
    let avg_type: opcua_types::NodeId = opcua_types::ObjectId::AggregateFunction_Average.into();

    let now = opcua_types::DateTime::now();
    let start = now - chrono::Duration::seconds(2);

    let points = history_read_processed(
        &session, &sine_id, start, now, 2000, // processing_interval_ms → 2s bucket
        avg_type, 100,
    )
    .await
    .expect("history_read_processed average");

    assert!(
        !points.is_empty(),
        "expected >=1 aggregate bucket, got {}",
        points.len()
    );

    for p in &points {
        assert!(
            p.numeric.is_some(),
            "aggregate bucket must have numeric value, got {:?}",
            p.value
        );
        let v = p.numeric.unwrap();
        assert!(
            (-11.0..=11.0).contains(&v),
            "aggregate value {v} outside sine range ±10%"
        );
    }

    let ts: Vec<&String> = points.iter().map(|p| &p.source_timestamp).collect();
    assert!(
        ts.windows(2).all(|w| w[0] <= w[1]),
        "timestamps must be monotonic"
    );

    server.stop().await.expect("server stop");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aggregate_max_gte_min() {
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,opcua=warn"),
    )
    .is_test(true)
    .try_init();

    const PORT: u16 = 48432;

    let server = Arc::new(OpcUaServer::new());
    server
        .start(&server_config(PORT), &[], &[sine_node()])
        .await
        .expect("server start");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let (_conn, session) = connect_and_wait(PORT, "agg2", 3000).await;

    let sine_id: opcua_types::NodeId = "ns=2;s=Demo.SineAgg".parse().unwrap();
    let max_type: opcua_types::NodeId = opcua_types::ObjectId::AggregateFunction_Maximum.into();
    let min_type: opcua_types::NodeId = opcua_types::ObjectId::AggregateFunction_Minimum.into();

    let now = opcua_types::DateTime::now();
    let start = now - chrono::Duration::seconds(2);

    let max_points = history_read_processed(&session, &sine_id, start, now, 2000, max_type, 100)
        .await
        .expect("history_read_processed maximum");

    let min_points = history_read_processed(&session, &sine_id, start, now, 2000, min_type, 100)
        .await
        .expect("history_read_processed minimum");

    assert!(!max_points.is_empty(), "expected maximum buckets");
    assert!(!min_points.is_empty(), "expected minimum buckets");

    let max_val = max_points[0].numeric.expect("max numeric");
    let min_val = min_points[0].numeric.expect("min numeric");
    assert!(
        max_val >= min_val,
        "max ({max_val}) must be >= min ({min_val})"
    );

    server.stop().await.expect("server stop");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aggregate_invalid_type_returns_error() {
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,opcua=warn"),
    )
    .is_test(true)
    .try_init();

    const PORT: u16 = 48433;

    let server = Arc::new(OpcUaServer::new());
    server
        .start(&server_config(PORT), &[], &[sine_node()])
        .await
        .expect("server start");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let (_conn, session) = connect_and_wait(PORT, "agg3", 2000).await;

    let sine_id: opcua_types::NodeId = "ns=2;s=Demo.SineAgg".parse().unwrap();
    let bad_type = opcua_types::NodeId::new(99, "nope");

    let now = opcua_types::DateTime::now();
    let start = now - chrono::Duration::seconds(2);

    let result = history_read_processed(&session, &sine_id, start, now, 2000, bad_type, 100).await;

    assert!(
        result.is_err(),
        "invalid aggregate type should return error, got {:?}",
        result.ok()
    );

    server.stop().await.expect("server stop");
}
