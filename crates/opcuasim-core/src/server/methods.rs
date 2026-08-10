//! Preset demo methods registered at server startup (A2).

use std::sync::Arc;

use opcua_nodes::MethodBuilder;
use opcua_server::node_manager::memory::InMemoryNodeManager;
use opcua_server::SubscriptionCache;
use opcua_types::{
    Argument, DataTypeId, DataValue, DateTime, LocalizedText, NodeId, ObjectId, StatusCode,
    UAString, Variant,
};

use super::history_node_manager::HistoryNodeManagerImpl;
use crate::error::OpcUaSimError;
use crate::server::server::OpcUaServer;

/// Register the preset demo methods. Returns their NodeIds.
pub async fn register_demo_methods(
    server: &OpcUaServer,
    subscriptions: Arc<SubscriptionCache>,
) -> Result<Vec<NodeId>, OpcUaSimError> {
    let nm = server
        .node_manager()
        .await
        .ok_or_else(|| OpcUaSimError::ServerError("Server not started".into()))?;
    let ns = server.namespace_index().await;

    let mut ids = Vec::new();

    // Echo: String -> String
    ids.push(register_method(
        &nm,
        ns,
        "Demo.Echo",
        "Echo",
        &[arg("input", DataTypeId::String)],
        &[arg("output", DataTypeId::String)],
        |inputs: &[Variant]| match inputs.first() {
            Some(Variant::String(s)) => Ok(vec![Variant::String(s.clone())]),
            _ => Err(StatusCode::BadInvalidArgument),
        },
    ));

    // Add: Double + Double -> Double
    ids.push(register_method(
        &nm,
        ns,
        "Demo.Add",
        "Add",
        &[arg("a", DataTypeId::Double), arg("b", DataTypeId::Double)],
        &[arg("sum", DataTypeId::Double)],
        |inputs: &[Variant]| {
            let a = match inputs.first() {
                Some(Variant::Double(v)) => *v,
                _ => return Err(StatusCode::BadInvalidArgument),
            };
            let b = match inputs.get(1) {
                Some(Variant::Double(v)) => *v,
                _ => return Err(StatusCode::BadInvalidArgument),
            };
            Ok(vec![Variant::Double(a + b)])
        },
    ));

    // RandomValue: Double (max, 0 = default 100) -> Double
    ids.push(register_method(
        &nm,
        ns,
        "Demo.RandomValue",
        "RandomValue",
        &[arg("max", DataTypeId::Double)],
        &[arg("value", DataTypeId::Double)],
        |inputs: &[Variant]| {
            let max = match inputs.first() {
                Some(Variant::Double(v)) if *v > 0.0 => *v,
                _ => 100.0,
            };
            Ok(vec![Variant::Double(rand::random::<f64>() * max)])
        },
    ));

    // SetNodeValue: String (node id) + Double -> String (status)
    let nm_for_set = Arc::clone(&nm);
    let subs_for_set = subscriptions;
    ids.push(register_method(
        &nm,
        ns,
        "Demo.SetNodeValue",
        "SetNodeValue",
        &[
            arg("node_id", DataTypeId::String),
            arg("value", DataTypeId::Double),
        ],
        &[arg("status", DataTypeId::String)],
        move |inputs: &[Variant]| {
            let node_id_str = match inputs.first() {
                Some(Variant::String(s)) => s.to_string(),
                _ => return Err(StatusCode::BadInvalidArgument),
            };
            let value = match inputs.get(1) {
                Some(Variant::Double(v)) => *v,
                _ => return Err(StatusCode::BadInvalidArgument),
            };
            let nid = match node_id_str.parse::<NodeId>() {
                Ok(n) => n,
                Err(_) => {
                    return Ok(vec![Variant::String(UAString::from(format!(
                        "BadNodeIdUnknown: {node_id_str}"
                    )))]);
                }
            };
            let now = DateTime::now();
            let mut dv = DataValue::new_now(Variant::Double(value));
            dv.source_timestamp = Some(now);
            dv.server_timestamp = Some(now);
            match nm_for_set.set_value(&*subs_for_set, &nid, None, dv) {
                Ok(()) => Ok(vec![Variant::String(UAString::from("Good"))]),
                Err(e) => Ok(vec![Variant::String(UAString::from(format!("{}", e)))]),
            }
        },
    ));

    Ok(ids)
}

fn arg(name: &str, data_type: DataTypeId) -> Argument {
    Argument {
        name: UAString::from(name),
        data_type: data_type.into(),
        value_rank: -1,
        array_dimensions: None,
        description: LocalizedText::from(""),
    }
}

fn register_method(
    nm: &Arc<InMemoryNodeManager<HistoryNodeManagerImpl>>,
    ns: u16,
    node_id_str: &str,
    display_name: &str,
    in_args: &[Argument],
    out_args: &[Argument],
    cb: impl Fn(&[Variant]) -> Result<Vec<Variant>, StatusCode> + Send + Sync + 'static,
) -> NodeId {
    let method_id = NodeId::new(ns, node_id_str);
    let in_args_id = NodeId::new(ns, format!("{node_id_str}.InputArguments"));
    let out_args_id = NodeId::new(ns, format!("{node_id_str}.OutputArguments"));
    let parent_id: NodeId = ObjectId::ObjectsFolder.into();

    {
        let mut addr = nm.address_space().write();
        let _ = MethodBuilder::new(&method_id, display_name, display_name)
            .component_of(parent_id)
            .executable(true)
            .user_executable(true)
            .input_args(&mut *addr, &in_args_id, in_args)
            .output_args(&mut *addr, &out_args_id, out_args)
            .insert(&mut *addr);
    }

    nm.inner().add_method_callback(method_id.clone(), cb);
    method_id
}
