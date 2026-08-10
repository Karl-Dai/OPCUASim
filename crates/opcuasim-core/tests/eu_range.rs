//! End-to-end: server variables expose an EURange property; percent deadband
//! subscription against the simulator succeeds (not BadDeadbandFilterInvalid).

use std::sync::Arc;
use std::time::Duration;

use opcua_client::Session;
use opcua_types::{AttributeId, NodeId, ReadValueId, TimestampsToReturn, Variant};

use opcuasim_core::client::OpcUaConnection;
use opcuasim_core::config::ConnectionConfig;
use opcuasim_core::node::{
    AccessMode, DataChangeFilterCfg, DataChangeTriggerKind, DeadbandKind, MonitoredNode,
};
use opcuasim_core::server::models::{DataType, ServerConfig, ServerNode, SimulationMode};
use opcuasim_core::server::server::OpcUaServer;
use opcuasim_core::subscription::SubscriptionManager;

const PORT: u16 = 48422;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn eu_range_property_and_percent_deadband() {
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,opcua=warn"),
    )
    .is_test(true)
    .try_init();

    let server = Arc::new(OpcUaServer::new());
    server
        .start(
            &ServerConfig {
                name: "EuRangeE2E".into(),
                endpoint_url: format!("opc.tcp://127.0.0.1:{PORT}"),
                port: PORT,
                security_policies: vec!["None".into()],
                security_modes: vec!["None".into()],
                users: Vec::new(),
                anonymous_enabled: true,
                max_sessions: 10,
                max_subscriptions_per_session: 10,
                history_buffer_size: 10_000,
            },
            &[],
            &[ServerNode {
                node_id: "Demo.Sine".into(),
                display_name: "Sine".into(),
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
                eu_range_low: -50.0,
                eu_range_high: 50.0,
            }],
        )
        .await
        .expect("server start");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let conn = Arc::new(OpcUaConnection::new(ConnectionConfig {
        id: "e1".into(),
        name: "e1".into(),
        endpoint_url: format!("opc.tcp://127.0.0.1:{PORT}"),
        security_policy: "None".into(),
        security_mode: "None".into(),
        auth: opcuasim_core::config::AuthConfig::Anonymous,
        timeout_ms: 5_000,
    }));
    conn.connect().await.expect("connect");
    let session: Arc<Session> = conn.get_session().await.expect("session");

    // 1. Read the EURange property: ns=2;s=Demo.Sine_EURange, Double array.
    let prop_id: NodeId = "ns=2;s=Demo.Sine_EURange".parse().expect("prop id");
    let values = session
        .read(
            &[ReadValueId::new(prop_id, AttributeId::Value)],
            TimestampsToReturn::Neither,
            0.0,
        )
        .await
        .expect("read EURange");
    let dv = values.first().expect("dv");
    assert!(
        dv.status.as_ref().map(|s| s.is_good()).unwrap_or(false),
        "EURange read should be good: {:?}",
        dv.status
    );
    let v = dv.value.as_ref().expect("EURange value");
    let arr = match v {
        Variant::Array(a) => a,
        other => panic!("expected Array, got {other:?}"),
    };
    assert_eq!(
        arr.values.len(),
        2,
        "EURange should have exactly 2 elements"
    );
    let low = match &arr.values[0] {
        Variant::Double(d) => *d,
        other => panic!("expected Double[0], got {other:?}"),
    };
    let high = match &arr.values[1] {
        Variant::Double(d) => *d,
        other => panic!("expected Double[1], got {other:?}"),
    };
    assert_eq!(low, -50.0);
    assert_eq!(high, 50.0);

    // 2. Percent deadband subscription must succeed.
    let sub_mgr = SubscriptionManager::new();
    let mut node = MonitoredNode::new(
        "ns=2;s=Demo.Sine".into(),
        "Sine".into(),
        String::new(),
        "Double".into(),
    );
    node.access_mode = AccessMode::Subscription { interval_ms: 200.0 };
    node.filter = Some(DataChangeFilterCfg {
        trigger: DataChangeTriggerKind::StatusValue,
        deadband_kind: DeadbandKind::Percent,
        deadband_value: 5.0,
    });
    let result = sub_mgr.add_nodes(vec![node], Some(&session)).await;
    assert!(
        result.is_ok(),
        "percent deadband subscription should succeed: {result:?}"
    );

    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(sub_mgr.get_update_seq().await > 0, "expected data changes");

    conn.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
