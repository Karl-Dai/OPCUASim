use std::sync::Arc;

use log::info;
use opcua_client::Session;
use opcua_types::{
    AttributeId, Array, BrowseDescription, BrowseDirection, ByteString, DataValue, NodeClass,
    NodeId, NumericRange, ReadValueId, ReferenceTypeId, StatusCode, TimestampsToReturn, UAString,
    Variant, VariantScalarTypeId, WriteValue,
};

use crate::error::OpcUaSimError;
use crate::node::{BrowseResultItem, NodeAttributes};

/// Defensive cap on total references collected across Browse + BrowseNext pagination.
/// OPC UA Part 4 5.8.3: servers may silently truncate references beyond their per-request
/// limit (commonly 1000) via a ContinuationPoint; we must loop BrowseNext to exhaust them,
/// but still bound memory in pathological cases.
const MAX_TOTAL_REFERENCES: usize = 100_000;

/// Browse children of a node. Pass None for node_id to browse from root (Objects folder).
pub async fn browse_node(
    session: &Arc<Session>,
    node_id: Option<&str>,
) -> Result<Vec<BrowseResultItem>, OpcUaSimError> {
    let target_node = match node_id {
        Some(id) => id
            .parse::<NodeId>()
            .map_err(|e| OpcUaSimError::BrowseError(format!("Invalid node id '{}': {}", id, e)))?,
        None => NodeId::objects_folder_id(),
    };

    info!("Browsing node: {:?}", node_id);

    let browse_desc = vec![BrowseDescription {
        node_id: target_node,
        browse_direction: BrowseDirection::Forward,
        reference_type_id: ReferenceTypeId::References.into(), // Most permissive — all reference types
        include_subtypes: true,
        node_class_mask: 0,
        result_mask: 0x3F, // All fields
    }];

    let mut items = Vec::new();
    let mut continuation_point = ByteString::null();
    let mut first_round = true;

    // Follow ContinuationPoints until exhausted (OPC UA Part 4 5.8.3).
    // First iteration uses Browse; subsequent iterations use BrowseNext.
    loop {
        let (references, next_cp) = if first_round {
            first_round = false;
            let raw = session
                .browse(&browse_desc, 0, None)
                .await
                .map_err(|e| OpcUaSimError::BrowseError(format!("Browse failed: {}", e)))?;
            let Some(result) = raw.into_iter().next() else {
                break;
            };
            let next_cp = result.continuation_point;
            (result.references, next_cp)
        } else {
            let raw = session
                .browse_next(false, &[continuation_point])
                .await
                .map_err(|e| OpcUaSimError::BrowseError(format!("BrowseNext failed: {}", e)))?;
            let Some(result) = raw.into_iter().next() else {
                break;
            };
            let next_cp = result.continuation_point;
            (result.references, next_cp)
        };

        for r in references.into_iter().flatten() {
            let node_class_str = match r.node_class {
                NodeClass::Object => "Object",
                NodeClass::Variable => "Variable",
                NodeClass::Method => "Method",
                NodeClass::ObjectType => "ObjectType",
                NodeClass::VariableType => "VariableType",
                NodeClass::ReferenceType => "ReferenceType",
                NodeClass::DataType => "DataType",
                NodeClass::View => "View",
                _ => "Unspecified",
            };

            // Default to true — let actual browse determine if children exist
            let has_children = true;

            items.push(BrowseResultItem {
                node_id: r.node_id.node_id.to_string(),
                display_name: r.display_name.text.value().clone().unwrap_or_default(),
                node_class: node_class_str.to_string(),
                data_type: None, // Would require additional read to determine
                has_children,
            });

            if items.len() >= MAX_TOTAL_REFERENCES {
                log::warn!(
                    "Browse of {:?} exceeded {} references; truncating",
                    node_id,
                    MAX_TOTAL_REFERENCES
                );
                if !next_cp.is_null() {
                    // Release the server-held continuation point per Part 4 5.8.3.
                    if let Err(e) = session.browse_next(true, &[next_cp]).await {
                        log::warn!("Failed to release browse continuation point: {e}");
                    }
                }
                return Ok(items);
            }
        }

        if next_cp.is_null() {
            break;
        }
        continuation_point = next_cp;
    }

    info!("Browse returned {} items for {:?}", items.len(), node_id);
    Ok(items)
}

/// Recursively browse a node and collect all Variable descendants.
pub async fn collect_variables(
    session: &Arc<Session>,
    node_id: &str,
    max_depth: u32,
) -> Result<Vec<BrowseResultItem>, OpcUaSimError> {
    let mut variables = Vec::new();
    let mut stack: Vec<(String, u32)> = vec![(node_id.to_string(), 0)];

    while let Some((current_id, depth)) = stack.pop() {
        if depth > max_depth {
            continue;
        }

        match browse_node(session, Some(&current_id)).await {
            Ok(children) => {
                for child in children {
                    if child.node_class == "Variable" {
                        variables.push(child);
                    } else {
                        stack.push((child.node_id.clone(), depth + 1));
                    }
                }
            }
            Err(e) => {
                info!(
                    "Skipping node {} during variable collection: {}",
                    current_id, e
                );
                continue;
            }
        }
    }

    info!("Collected {} variables under {}", variables.len(), node_id);
    Ok(variables)
}

/// Read detailed attributes of a specific node.
pub async fn read_node_attributes(
    session: &Arc<Session>,
    node_id: &str,
) -> Result<NodeAttributes, OpcUaSimError> {
    let target_node = node_id
        .parse::<NodeId>()
        .map_err(|e| OpcUaSimError::ReadError(format!("Invalid node id '{}': {}", node_id, e)))?;

    // Read multiple attributes: DisplayName, Description, DataType, Value, AccessLevel
    let nodes_to_read = vec![
        ReadValueId::new(target_node.clone(), AttributeId::DisplayName),
        ReadValueId::new(target_node.clone(), AttributeId::Description),
        ReadValueId::new(target_node.clone(), AttributeId::DataType),
        ReadValueId::new(target_node.clone(), AttributeId::Value),
        ReadValueId::new(target_node.clone(), AttributeId::AccessLevel),
    ];

    let values = session
        .read(&nodes_to_read, TimestampsToReturn::Both, 0.0)
        .await
        .map_err(|e| OpcUaSimError::ReadError(format!("Read failed: {}", e)))?;

    let display_name = values
        .first()
        .and_then(|dv| dv.value.as_ref())
        .map(|v| format!("{}", v))
        .unwrap_or_else(|| node_id.to_string());

    let description = values
        .get(1)
        .and_then(|dv| dv.value.as_ref())
        .map(|v| format!("{}", v))
        .unwrap_or_default();

    let data_type = values
        .get(2)
        .and_then(|dv| dv.value.as_ref())
        .map(|v| format!("{}", v))
        .unwrap_or_else(|| "Unknown".to_string());

    let value_dv = values.get(3);
    let value = value_dv
        .and_then(|dv| dv.value.as_ref())
        .map(|v| crate::server::address_space::variant_to_display_string(v));

    let quality = value_dv
        .and_then(|dv| dv.status.as_ref())
        .map(|s| format!("{}", s));

    let timestamp = value_dv
        .and_then(|dv| dv.source_timestamp.as_ref())
        .map(|t| t.to_string());

    let access_level = values
        .get(4)
        .and_then(|dv| dv.value.as_ref())
        .map(|v| format!("{}", v))
        .unwrap_or_else(|| "0".to_string());

    Ok(NodeAttributes {
        node_id: node_id.to_string(),
        display_name,
        description,
        data_type,
        access_level,
        value,
        quality,
        timestamp,
    })
}

/// Convert a user-entered string to the appropriate OPC UA Variant based on the data type name.
fn string_to_variant(value: &str, data_type: &str) -> Result<Variant, OpcUaSimError> {
    let err = |msg: &dyn std::fmt::Display| {
        OpcUaSimError::WriteError(format!(
            "Cannot convert '{}' to {}: {}",
            value, data_type, msg
        ))
    };

    if value.contains(';') || value.contains(',') {
        return parse_array_heuristic(value, data_type).map_err(|e| {
            OpcUaSimError::WriteError(format!(
                "Cannot convert '{}' to {}: {}",
                value, data_type, e
            ))
        });
    }

    match data_type {
        "Boolean" => match value.eq_ignore_ascii_case("true") || value == "1" {
            true => Ok(Variant::Boolean(true)),
            false if value.eq_ignore_ascii_case("false") || value == "0" => {
                Ok(Variant::Boolean(false))
            }
            _ => Err(err(&"expected true/false/1/0")),
        },
        "SByte" => value.parse::<i8>().map(Variant::SByte).map_err(|e| err(&e)),
        "Byte" => value.parse::<u8>().map(Variant::Byte).map_err(|e| err(&e)),
        "Int16" => value
            .parse::<i16>()
            .map(Variant::Int16)
            .map_err(|e| err(&e)),
        "UInt16" => value
            .parse::<u16>()
            .map(Variant::UInt16)
            .map_err(|e| err(&e)),
        "Int32" => value
            .parse::<i32>()
            .map(Variant::Int32)
            .map_err(|e| err(&e)),
        "UInt32" => value
            .parse::<u32>()
            .map(Variant::UInt32)
            .map_err(|e| err(&e)),
        "Int64" => value
            .parse::<i64>()
            .map(Variant::Int64)
            .map_err(|e| err(&e)),
        "UInt64" => value
            .parse::<u64>()
            .map(Variant::UInt64)
            .map_err(|e| err(&e)),
        "Float" => value
            .parse::<f32>()
            .map(Variant::Float)
            .map_err(|e| err(&e)),
        "Double" => value
            .parse::<f64>()
            .map(Variant::Double)
            .map_err(|e| err(&e)),
        "String" => Ok(Variant::String(UAString::from(value))),
        _ => Err(OpcUaSimError::WriteError(format!(
            "Unsupported data type for write: {}",
            data_type
        ))),
    }
}

fn element_scalar_id(data_type: &str) -> VariantScalarTypeId {
    match data_type {
        "Boolean" => VariantScalarTypeId::Boolean,
        "SByte" => VariantScalarTypeId::SByte,
        "Byte" => VariantScalarTypeId::Byte,
        "Int16" => VariantScalarTypeId::Int16,
        "UInt16" => VariantScalarTypeId::UInt16,
        "Int32" => VariantScalarTypeId::Int32,
        "UInt32" => VariantScalarTypeId::UInt32,
        "Int64" => VariantScalarTypeId::Int64,
        "UInt64" => VariantScalarTypeId::UInt64,
        "Float" => VariantScalarTypeId::Float,
        "Double" | _ => VariantScalarTypeId::Double,
    }
}

fn parse_scalar_element(s: &str, scalar: VariantScalarTypeId) -> Option<Variant> {
    let s = s.trim();
    match scalar {
        VariantScalarTypeId::Boolean => match s {
            "true" | "1" => Some(Variant::Boolean(true)),
            "false" | "0" => Some(Variant::Boolean(false)),
            _ => None,
        },
        VariantScalarTypeId::SByte => s.parse::<i8>().ok().map(Variant::SByte),
        VariantScalarTypeId::Byte => s.parse::<u8>().ok().map(Variant::Byte),
        VariantScalarTypeId::Int16 => s.parse::<i16>().ok().map(Variant::Int16),
        VariantScalarTypeId::UInt16 => s.parse::<u16>().ok().map(Variant::UInt16),
        VariantScalarTypeId::Int32 => s.parse::<i32>().ok().map(Variant::Int32),
        VariantScalarTypeId::UInt32 => s.parse::<u32>().ok().map(Variant::UInt32),
        VariantScalarTypeId::Int64 => s.parse::<i64>().ok().map(Variant::Int64),
        VariantScalarTypeId::UInt64 => s.parse::<u64>().ok().map(Variant::UInt64),
        VariantScalarTypeId::Float => s.parse::<f32>().ok().map(Variant::Float),
        VariantScalarTypeId::Double => s.parse::<f64>().ok().map(Variant::Double),
        VariantScalarTypeId::String => Some(Variant::String(UAString::from(s))),
        _ => s.parse::<f64>().ok().map(Variant::Double),
    }
}

fn parse_array_heuristic(value: &str, data_type: &str) -> Result<Variant, String> {
    let scalar = element_scalar_id(data_type);
    if value.contains(';') {
        let rows: Vec<&str> = value.split(';').collect();
        let mut values: Vec<Variant> = Vec::new();
        let mut cols = 0usize;
        for (ri, row) in rows.iter().enumerate() {
            let elements: Vec<&str> = row.split(',').collect();
            if ri == 0 {
                cols = elements.len();
            }
            for elem in elements {
                let v = parse_scalar_element(elem, scalar)
                    .ok_or_else(|| format!("cannot parse '{}' as {:?}", elem.trim(), scalar))?;
                values.push(v);
            }
        }
        let dims = vec![rows.len() as u32, cols as u32];
        Array::new_multi(scalar, values, dims)
            .map(|arr| Variant::Array(Box::new(arr)))
            .map_err(|e| format!("Array::new_multi failed: {e:?}"))
    } else {
        let elements: Vec<&str> = value.split(',').collect();
        let mut values: Vec<Variant> = Vec::with_capacity(elements.len());
        for elem in elements {
            let v = parse_scalar_element(elem, scalar)
                .ok_or_else(|| format!("cannot parse '{}' as {:?}", elem.trim(), scalar))?;
            values.push(v);
        }
        Array::new(scalar, values)
            .map(|arr| Variant::Array(Box::new(arr)))
            .map_err(|e| format!("Array::new failed: {e:?}"))
    }
}

/// Write a value to a node's Value attribute.
pub async fn write_node_value(
    session: &Arc<Session>,
    node_id: &str,
    value: &str,
    data_type: &str,
) -> Result<(), OpcUaSimError> {
    let target_node = node_id
        .parse::<NodeId>()
        .map_err(|e| OpcUaSimError::WriteError(format!("Invalid node id '{}': {}", node_id, e)))?;

    let variant = string_to_variant(value, data_type)?;
    info!(
        "Writing {} = {:?} (data_type={})",
        node_id, variant, data_type
    );

    let write_value = WriteValue {
        node_id: target_node.clone(),
        attribute_id: AttributeId::Value as u32,
        index_range: NumericRange::None,
        value: DataValue::value_only(variant),
    };

    let results = session
        .write(&[write_value])
        .await
        .map_err(|e| OpcUaSimError::WriteError(format!("Write request failed: {}", e)))?;

    let status = results
        .first()
        .copied()
        .unwrap_or(StatusCode::BadUnexpectedError);
    if !status.is_good() {
        // Read AccessLevel + UserAccessLevel for diagnostics
        let diag_reads = vec![
            ReadValueId::new(target_node.clone(), AttributeId::AccessLevel),
            ReadValueId::new(target_node, AttributeId::UserAccessLevel),
        ];
        let diag = session
            .read(&diag_reads, TimestampsToReturn::Neither, 0.0)
            .await
            .ok();
        let (al, ual) = diag
            .map(|v| {
                let fmt = |dv: Option<&opcua_types::DataValue>| -> String {
                    dv.and_then(|d| d.value.as_ref())
                        .map(|v| format!("{v}"))
                        .unwrap_or_else(|| {
                            dv.and_then(|d| d.status.as_ref())
                                .map(|s| format!("{s}"))
                                .unwrap_or("?".into())
                        })
                };
                (fmt(v.first()), fmt(v.get(1)))
            })
            .unwrap_or(("?".into(), "?".into()));
        return Err(OpcUaSimError::WriteError(format!(
            "{} (AccessLevel={}, UserAccessLevel={})",
            status, al, ual
        )));
    }

    info!("Write succeeded: {} = {} ({})", node_id, value, data_type);
    Ok(())
}
