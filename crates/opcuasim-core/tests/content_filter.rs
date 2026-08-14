//! Integration test: where_clause ContentFilter evaluation on real event history.

use std::sync::Arc;
use std::time::Duration;

use opcua_types::{
    AttributeId, ContentFilter, ContentFilterElement, EventFilter, ExtensionObject, FilterOperator,
    LiteralOperand, NodeId, NumericRange, ObjectTypeId, QualifiedName, SimpleAttributeOperand,
    Variant,
};
use opcuasim_core::client::OpcUaConnection;
use opcuasim_core::config::{AuthConfig, ConnectionConfig};
use opcuasim_core::history::history_read_events;
use opcuasim_core::server::models::{DataType, ServerConfig, ServerNode, SimulationMode};
use opcuasim_core::server::server::OpcUaServer;

/// Distinct ports per test: the two e2e tests run in parallel (cargo test
/// default thread count), so a shared port would race on bind.
const PORT: u16 = 48441;
const PORT_CAST: u16 = 48442;

fn server_config(port: u16) -> ServerConfig {
    ServerConfig {
        name: "ContentFilterE2E".into(),
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
        ..Default::default()
    }
}

fn sine_node() -> ServerNode {
    ServerNode {
        node_id: "Demo.TestSine".into(),
        browse_name: None,
        display_name: "TestSine".into(),
        parent_id: "i=85".into(),
        data_type: DataType::Double,
        writable: false,
        simulation: SimulationMode::Sine {
            amplitude: 10.0,
            offset: 0.0,
            period_ms: 4000,
            interval_ms: 200,
        },

        eu_range_low: 0.0,
        eu_range_high: 100.0,
    }
}

fn make_event_filter(where_clause: ContentFilter) -> EventFilter {
    let base: NodeId = ObjectTypeId::BaseEventType.into();
    let select_clauses = [
        "Time",
        "Severity",
        "SourceNode",
        "SourceName",
        "Message",
        "EventId",
        "EventType",
    ]
    .iter()
    .map(|field| SimpleAttributeOperand {
        type_definition_id: base.clone(),
        browse_path: Some(vec![QualifiedName::from(*field)]),
        attribute_id: AttributeId::Value as u32,
        index_range: NumericRange::None,
    })
    .collect();
    EventFilter {
        select_clauses: Some(select_clauses),
        where_clause,
    }
}

fn sao_ext(field_name: &str) -> ExtensionObject {
    ExtensionObject::from_message(SimpleAttributeOperand {
        type_definition_id: NodeId::null(),
        browse_path: Some(vec![QualifiedName::from(field_name)]),
        attribute_id: AttributeId::Value as u32,
        index_range: NumericRange::None,
    })
}

fn lit_ext(v: Variant) -> ExtensionObject {
    ExtensionObject::from_message(LiteralOperand { value: v })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn content_filter_where_clause_e2e() {
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

    let conn = Arc::new(OpcUaConnection::new(ConnectionConfig {
        id: "cf1".into(),
        name: "cf1".into(),
        endpoint_url: format!("opc.tcp://127.0.0.1:{PORT}"),
        security_policy: "None".into(),
        security_mode: "None".into(),
        auth: AuthConfig::Anonymous,
        timeout_ms: 10_000,
    }));
    conn.connect().await.expect("connect");
    let session = conn.get_session().await.expect("session");

    let demo_events_id: NodeId = "ns=2;s=DemoEvents".parse().unwrap();

    // Wait for heartbeat events to accumulate in history.
    tokio::time::sleep(Duration::from_secs(5)).await;

    let start_past = opcua_types::DateTime::ymd_hms(2020, 1, 1, 0, 0, 0);
    let end = opcua_types::DateTime::now();

    // Test 1: No where_clause → all events returned.
    let all_filter = make_event_filter(ContentFilter::default());
    let all = history_read_events(&session, &demo_events_id, start_past, end, 1000, all_filter)
        .await
        .expect("history read all");
    assert!(!all.is_empty(), "expected at least one event");
    println!("[OK] unfiltered: {} events", all.len());

    // Test 2: Severity Equals 500 → should match heartbeat events.
    let sev500_filter = make_event_filter(ContentFilter {
        elements: Some(vec![ContentFilterElement {
            filter_operator: FilterOperator::Equals,
            filter_operands: Some(vec![sao_ext("Severity"), lit_ext(Variant::UInt16(500))]),
        }]),
    });
    let sev500 = history_read_events(
        &session,
        &demo_events_id,
        start_past,
        end,
        1000,
        sev500_filter,
    )
    .await
    .expect("history read severity=500");
    println!("[OK] Severity=500: {} events", sev500.len());
    // At least some events should have severity 500 (heartbeat).
    assert!(
        !sev500.is_empty(),
        "expected at least one severity-500 event"
    );

    // Test 3: Severity > 100 (AND) Message Like "Heartbeat%" → compound filter.
    let compound_filter = make_event_filter(ContentFilter {
        elements: Some(vec![
            ContentFilterElement {
                filter_operator: FilterOperator::GreaterThan,
                filter_operands: Some(vec![sao_ext("Severity"), lit_ext(Variant::UInt16(100))]),
            },
            ContentFilterElement {
                filter_operator: FilterOperator::Like,
                filter_operands: Some(vec![
                    sao_ext("Message"),
                    lit_ext(Variant::String("Heartbeat%".into())),
                ]),
            },
        ]),
    });
    let compound = history_read_events(
        &session,
        &demo_events_id,
        start_past,
        end,
        1000,
        compound_filter,
    )
    .await
    .expect("history read compound");
    println!(
        "[OK] Severity>100 AND Message LIKE Heartbeat%: {} events",
        compound.len()
    );
    // All returned events should have "Heartbeat" in message.
    for ev in &compound {
        let has_heartbeat = ev.fields.iter().any(|f| f.contains("Heartbeat"));
        assert!(
            has_heartbeat,
            "compound filter returned non-heartbeat event: {:?}",
            ev.fields
        );
    }

    // Test 4: IsNull on a non-existent field → no events match.
    let isnull_filter = make_event_filter(ContentFilter {
        elements: Some(vec![ContentFilterElement {
            filter_operator: FilterOperator::IsNull,
            filter_operands: Some(vec![sao_ext("EventType")]),
        }]),
    });
    let isnull = history_read_events(
        &session,
        &demo_events_id,
        start_past,
        end,
        1000,
        isnull_filter,
    )
    .await
    .expect("history read IsNull");
    println!("[OK] IsNull(EventType): {} events", isnull.len());
    // EventType is always non-null, so should be empty.
    assert!(
        isnull.is_empty(),
        "expected no events with IsNull EventType, got {}",
        isnull.len()
    );

    // Cleanup
    conn.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");

    println!("\n=== content_filter_where_clause_e2e PASSED ===");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn content_filter_cast_operator_returns_error() {
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,opcua=warn"),
    )
    .is_test(true)
    .try_init();

    let server = Arc::new(OpcUaServer::new());
    server
        .start(&server_config(PORT_CAST), &[], &[sine_node()])
        .await
        .expect("server start");
    tokio::time::sleep(Duration::from_millis(500)).await;

    let conn = Arc::new(OpcUaConnection::new(ConnectionConfig {
        id: "cf_cast".into(),
        name: "cf_cast".into(),
        endpoint_url: format!("opc.tcp://127.0.0.1:{PORT_CAST}"),
        security_policy: "None".into(),
        security_mode: "None".into(),
        auth: AuthConfig::Anonymous,
        timeout_ms: 10_000,
    }));
    conn.connect().await.expect("connect");
    let session = conn.get_session().await.expect("session");

    let demo_events_id: NodeId = "ns=2;s=DemoEvents".parse().unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;

    let start_past = opcua_types::DateTime::ymd_hms(2020, 1, 1, 0, 0, 0);
    let end = opcua_types::DateTime::now();

    // Filter with Cast operator (unsupported) — must return error, not empty list.
    let cast_filter = make_event_filter(ContentFilter {
        elements: Some(vec![ContentFilterElement {
            filter_operator: FilterOperator::Cast,
            filter_operands: Some(vec![sao_ext("Severity")]),
        }]),
    });
    let result = history_read_events(
        &session,
        &demo_events_id,
        start_past,
        end,
        1000,
        cast_filter,
    )
    .await;

    assert!(
        result.is_err(),
        "Cast filter should return error, not empty list"
    );

    conn.disconnect().await.expect("disconnect");
    server.stop().await.expect("server stop");

    println!("\n=== content_filter_cast_operator_returns_error PASSED ===");
}
