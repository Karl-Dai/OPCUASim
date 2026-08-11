//! End-to-end: preset demo methods are registered and callable.

use std::sync::Arc;
use std::time::Duration;

use opcua_types::{AttributeId, ReadValueId, TimestampsToReturn, Variant};
use opcuasim_core::client::OpcUaConnection;
use opcuasim_core::config::{AuthConfig, ConnectionConfig};
use opcuasim_core::method::call_method;
use opcuasim_core::server::models::{DataType, ServerConfig, ServerNode, SimulationMode};
use opcuasim_core::server::server::OpcUaServer;

const PORT: u16 = 48431;

fn server_config() -> ServerConfig {
    ServerConfig {
        name: "MethodsE2E".into(),
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
    }
}

fn writable_node() -> ServerNode {
    ServerNode {
        node_id: "Demo.Setpoint".into(),
        display_name: "Setpoint".into(),
        parent_id: "i=85".into(),
        data_type: DataType::Double,
        writable: true,
        simulation: SimulationMode::Static { value: "0".into() },
        update_seq: 0,
        current_value: None,
        eu_range_low: 0.0,
        eu_range_high: 100.0,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn preset_methods_are_callable() {
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,opcua=warn"),
    )
    .is_test(true)
    .try_init();

    let server = Arc::new(OpcUaServer::new());
    server
        .start(&server_config(), &[], &[writable_node()])
        .await
        .expect("server start");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let conn = Arc::new(OpcUaConnection::new(ConnectionConfig {
        id: "m1".into(),
        name: "m1".into(),
        endpoint_url: format!("opc.tcp://127.0.0.1:{PORT}"),
        security_policy: "None".into(),
        security_mode: "None".into(),
        auth: AuthConfig::Anonymous,
        timeout_ms: 5_000,
    }));
    conn.connect().await.expect("connect");
    let session = conn.get_session().await.expect("session");

    // Methods are registered as components of ObjectsFolder (ns=0;i=85).
    let object_id = opcua_types::NodeId::new(0, 85u32);

    // Echo: String -> String
    let outcome = call_method(
        &session,
        &object_id,
        &"ns=2;s=Demo.Echo".parse().unwrap(),
        vec![Variant::String("hello".into())],
    )
    .await
    .expect("echo");
    assert!(
        outcome
            .outputs
            .first()
            .is_some_and(|v| matches!(v, Variant::String(s) if s == "hello")),
        "Echo output mismatch: {:?}",
        outcome.outputs
    );

    // Add: Double + Double -> Double
    let outcome = call_method(
        &session,
        &object_id,
        &"ns=2;s=Demo.Add".parse().unwrap(),
        vec![Variant::Double(2.0), Variant::Double(3.0)],
    )
    .await
    .expect("add");
    assert!(
        outcome
            .outputs
            .first()
            .is_some_and(|v| matches!(v, Variant::Double(val) if (*val - 5.0).abs() < 1e-9)),
        "Add output mismatch: {:?}",
        outcome.outputs
    );

    // RandomValue: Double (max) -> Double in [0, max)
    let outcome = call_method(
        &session,
        &object_id,
        &"ns=2;s=Demo.RandomValue".parse().unwrap(),
        vec![Variant::Double(0.0)],
    )
    .await
    .expect("random");
    assert!(
        outcome
            .outputs
            .first()
            .is_some_and(|v| matches!(v, Variant::Double(val) if *val >= 0.0 && *val < 100.0)),
        "RandomValue output out of [0, 100): {:?}",
        outcome.outputs
    );

    // SetNodeValue: String (node_id) + Double -> String (status)
    // Writing to a non-existent node returns an error status string (non-empty).
    let outcome = call_method(
        &session,
        &object_id,
        &"ns=2;s=Demo.SetNodeValue".parse().unwrap(),
        vec![
            Variant::String("ns=2;s=Demo.Missing".into()),
            Variant::Double(1.0),
        ],
    )
    .await
    .expect("setnodevalue");
    assert!(
        outcome
            .outputs
            .first()
            .is_some_and(|v| matches!(v, Variant::String(s) if !s.is_empty())),
        "SetNodeValue status should be non-empty: {:?}",
        outcome.outputs
    );

    // SetNodeValue happy path: write an existing writable node, read back.
    let out_ok = call_method(
        &session,
        &object_id,
        &"ns=2;s=Demo.SetNodeValue".parse().unwrap(),
        vec![
            Variant::String("ns=2;s=Demo.Setpoint".into()),
            Variant::Double(42.5),
        ],
    )
    .await
    .expect("setnodevalue ok");
    assert!(
        out_ok
            .outputs
            .first()
            .is_some_and(|v| matches!(v, Variant::String(s) if s == "Good")),
        "SetNodeValue happy-path status mismatch: {:?}",
        out_ok.outputs
    );

    // Read back via OPC UA Read service and assert the value is 42.5.
    let setpoint_id: opcua_types::NodeId = "ns=2;s=Demo.Setpoint".parse().unwrap();
    let ids = [ReadValueId::new(setpoint_id, AttributeId::Value)];
    let values = session
        .read(&ids, TimestampsToReturn::Both, 0.0)
        .await
        .expect("read setpoint");
    let read_value = values
        .first()
        .and_then(|dv| dv.value.as_ref())
        .expect("setpoint value missing");
    match read_value {
        Variant::Double(v) => assert!(
            (*v - 42.5).abs() < 1e-9,
            "setpoint read-back value mismatch: expected 42.5, got {}",
            v
        ),
        other => panic!("setpoint read-back unexpected variant: {:?}", other),
    }

    conn.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");
}
