//! Historical data access wrapper around Session::history_read.
//! Loops continuation points up to max_values.

use std::sync::Arc;

use opcua_client::{HistoryReadAction, Session};
use opcua_types::{
    AggregateConfiguration, ContinuationPoint, DataValue, DateTime, EventFilter, HistoryData,
    HistoryEvent, HistoryReadResult, HistoryReadValueId, NodeId, NumericRange, QualifiedName,
    ReadEventDetails, ReadProcessedDetails, ReadRawModifiedDetails, TimestampsToReturn,
};

use crate::error::OpcUaSimError;

#[derive(Debug, Clone)]
pub struct HistoryDataPoint {
    pub source_timestamp: String,
    pub server_timestamp: String,
    pub value: String,
    pub numeric: Option<f64>,
    pub status: String,
}

pub async fn history_read_raw(
    session: &Arc<Session>,
    node_id: &NodeId,
    start: DateTime,
    end: DateTime,
    max_values: u32,
    return_bounds: bool,
) -> Result<Vec<HistoryDataPoint>, OpcUaSimError> {
    let mut out: Vec<HistoryDataPoint> = Vec::new();
    let mut continuation_point = ContinuationPoint::null();

    loop {
        let action = HistoryReadAction::ReadRawModifiedDetails(ReadRawModifiedDetails {
            is_read_modified: false,
            start_time: start,
            end_time: end,
            num_values_per_node: max_values.saturating_sub(out.len() as u32),
            return_bounds,
        });
        let nodes_to_read = vec![HistoryReadValueId {
            node_id: node_id.clone(),
            index_range: NumericRange::None,
            data_encoding: QualifiedName::null(),
            continuation_point: continuation_point.clone(),
        }];

        let results: Vec<HistoryReadResult> = session
            .history_read(action, TimestampsToReturn::Both, false, &nodes_to_read)
            .await
            .map_err(|e| OpcUaSimError::ConnectionFailed(format!("history_read failed: {e}")))?;

        let result = results
            .into_iter()
            .next()
            .ok_or_else(|| OpcUaSimError::ConnectionFailed("history_read empty result".into()))?;

        if !result.status_code.is_good() {
            return Err(OpcUaSimError::ConnectionFailed(format!(
                "history_read status: {}",
                result.status_code
            )));
        }

        let history_data: Option<Box<HistoryData>> =
            result.history_data.into_inner_as::<HistoryData>();
        let dvs: Vec<DataValue> = history_data
            .and_then(|hd| hd.data_values)
            .unwrap_or_default();

        let reached_max = {
            let mut reached = false;
            for dv in dvs {
                out.push(map_data_value(dv));
                if out.len() as u32 >= max_values {
                    reached = true;
                    break;
                }
            }
            reached
        };

        // All data consumed and server has no more pages: done.
        if result.continuation_point.is_null() {
            break;
        }

        // Reached the caller's max_values with a pending continuation point:
        // release it server-side per OPC UA Part 4 5.10.3.
        if reached_max {
            let release_nodes = vec![HistoryReadValueId {
                node_id: node_id.clone(),
                index_range: NumericRange::None,
                data_encoding: QualifiedName::null(),
                continuation_point: result.continuation_point.clone(),
            }];
            let release_action =
                HistoryReadAction::ReadRawModifiedDetails(ReadRawModifiedDetails::default());
            if let Err(e) = session
                .history_read(
                    release_action,
                    TimestampsToReturn::Neither,
                    true,
                    &release_nodes,
                )
                .await
            {
                log::warn!("Failed to release history continuation point: {e}");
            }
            break;
        }

        continuation_point = result.continuation_point;
    }

    Ok(out)
}

/// Read processed (aggregated) history: buckets computed by the server with
/// the given `processing_interval_ms` and aggregation function.
///
/// Internally follows continuation points with the same paging / release
/// pattern as [`history_read_raw`].
pub async fn history_read_processed(
    session: &Arc<Session>,
    node_id: &NodeId,
    start: DateTime,
    end: DateTime,
    processing_interval_ms: u64,
    agg_type: NodeId,
    max_values: u32,
) -> Result<Vec<HistoryDataPoint>, OpcUaSimError> {
    let mut out: Vec<HistoryDataPoint> = Vec::new();
    let mut continuation_point = ContinuationPoint::null();

    loop {
        let action = HistoryReadAction::ReadProcessedDetails(ReadProcessedDetails {
            start_time: start,
            end_time: end,
            processing_interval: processing_interval_ms as f64 / 1000.0,
            aggregate_type: Some(vec![agg_type.clone()]),
            aggregate_configuration: AggregateConfiguration {
                use_server_capabilities_defaults: true,
                treat_uncertain_as_bad: false,
                percent_data_bad: 0,
                percent_data_good: 100,
                use_sloped_extrapolation: false,
            },
        });
        let nodes_to_read = vec![HistoryReadValueId {
            node_id: node_id.clone(),
            index_range: NumericRange::None,
            data_encoding: QualifiedName::null(),
            continuation_point: continuation_point.clone(),
        }];

        let results: Vec<HistoryReadResult> = session
            .history_read(action, TimestampsToReturn::Both, false, &nodes_to_read)
            .await
            .map_err(|e| {
                OpcUaSimError::ConnectionFailed(format!("history_read_processed failed: {e}"))
            })?;

        let result = results.into_iter().next().ok_or_else(|| {
            OpcUaSimError::ConnectionFailed("history_read_processed empty result".into())
        })?;

        if !result.status_code.is_good() {
            return Err(OpcUaSimError::ConnectionFailed(format!(
                "history_read_processed status: {}",
                result.status_code
            )));
        }

        let history_data: Option<Box<HistoryData>> =
            result.history_data.into_inner_as::<HistoryData>();
        let dvs: Vec<DataValue> = history_data
            .and_then(|hd| hd.data_values)
            .unwrap_or_default();

        let reached_max = {
            let mut reached = false;
            for dv in dvs {
                out.push(map_data_value(dv));
                if out.len() as u32 >= max_values {
                    reached = true;
                    break;
                }
            }
            reached
        };

        // All data consumed and server has no more pages: done.
        if result.continuation_point.is_null() {
            break;
        }

        // Reached the caller's max_values with a pending continuation point:
        // release it server-side per OPC UA Part 4 5.10.3.
        if reached_max {
            let release_nodes = vec![HistoryReadValueId {
                node_id: node_id.clone(),
                index_range: NumericRange::None,
                data_encoding: QualifiedName::null(),
                continuation_point: result.continuation_point.clone(),
            }];
            let release_action =
                HistoryReadAction::ReadProcessedDetails(ReadProcessedDetails::default());
            if let Err(e) = session
                .history_read(
                    release_action,
                    TimestampsToReturn::Neither,
                    true,
                    &release_nodes,
                )
                .await
            {
                log::warn!("Failed to release history_processed continuation point: {e}");
            }
            break;
        }

        continuation_point = result.continuation_point;
    }

    Ok(out)
}

fn map_data_value(dv: DataValue) -> HistoryDataPoint {
    let value_str = dv
        .value
        .as_ref()
        .map(|v| format!("{v}"))
        .unwrap_or_default();
    let numeric = dv.value.as_ref().and_then(variant_to_f64);
    let status = dv
        .status
        .map(|s| format!("{s}"))
        .unwrap_or_else(|| "Good".to_string());
    let source_timestamp = dv
        .source_timestamp
        .map(|t| t.to_string())
        .unwrap_or_default();
    let server_timestamp = dv
        .server_timestamp
        .map(|t| t.to_string())
        .unwrap_or_default();
    HistoryDataPoint {
        source_timestamp,
        server_timestamp,
        value: value_str,
        numeric,
        status,
    }
}

fn variant_to_f64(v: &opcua_types::Variant) -> Option<f64> {
    use opcua_types::Variant;
    match v {
        Variant::Boolean(b) => Some(if *b { 1.0 } else { 0.0 }),
        Variant::SByte(x) => Some(*x as f64),
        Variant::Byte(x) => Some(*x as f64),
        Variant::Int16(x) => Some(*x as f64),
        Variant::UInt16(x) => Some(*x as f64),
        Variant::Int32(x) => Some(*x as f64),
        Variant::UInt32(x) => Some(*x as f64),
        Variant::Int64(x) => Some(*x as f64),
        Variant::UInt64(x) => Some(*x as f64),
        Variant::Float(x) => Some(*x as f64),
        Variant::Double(x) => Some(*x),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct EventHistoryPoint {
    pub time: String,
    pub fields: Vec<String>,
}

pub async fn history_read_events(
    session: &Arc<Session>,
    node_id: &NodeId,
    start: DateTime,
    end: DateTime,
    max_events: u32,
    filter: EventFilter,
) -> Result<Vec<EventHistoryPoint>, OpcUaSimError> {
    let mut out: Vec<EventHistoryPoint> = Vec::new();
    let mut continuation_point = ContinuationPoint::null();

    loop {
        let action = HistoryReadAction::ReadEventDetails(ReadEventDetails {
            num_values_per_node: max_events.saturating_sub(out.len() as u32),
            start_time: start,
            end_time: end,
            filter: filter.clone(),
        });
        let nodes_to_read = vec![HistoryReadValueId {
            node_id: node_id.clone(),
            index_range: NumericRange::None,
            data_encoding: QualifiedName::null(),
            continuation_point: continuation_point.clone(),
        }];

        let results: Vec<HistoryReadResult> = session
            .history_read(action, TimestampsToReturn::Both, false, &nodes_to_read)
            .await
            .map_err(|e| {
                OpcUaSimError::ConnectionFailed(format!("history_read_events failed: {e}"))
            })?;

        let result = results.into_iter().next().ok_or_else(|| {
            OpcUaSimError::ConnectionFailed("history_read_events empty result".into())
        })?;

        if !result.status_code.is_good() {
            return Err(OpcUaSimError::ConnectionFailed(format!(
                "history_read_events status: {}",
                result.status_code
            )));
        }

        let history_event: Option<Box<HistoryEvent>> =
            result.history_data.into_inner_as::<HistoryEvent>();
        let field_lists = history_event.and_then(|he| he.events).unwrap_or_default();

        let reached_max = {
            let mut reached = false;
            for field_list in field_lists {
                let fields = field_list.event_fields.unwrap_or_default();
                let time_str = fields.get(4).map(|v| format!("{v}")).unwrap_or_default();
                let field_strs: Vec<String> = fields.iter().map(|v| format!("{v}")).collect();
                out.push(EventHistoryPoint {
                    time: time_str,
                    fields: field_strs,
                });
                if out.len() as u32 >= max_events {
                    reached = true;
                    break;
                }
            }
            reached
        };

        if result.continuation_point.is_null() {
            break;
        }

        if reached_max {
            let release_nodes = vec![HistoryReadValueId {
                node_id: node_id.clone(),
                index_range: NumericRange::None,
                data_encoding: QualifiedName::null(),
                continuation_point: result.continuation_point.clone(),
            }];
            let release_action = HistoryReadAction::ReadEventDetails(ReadEventDetails::default());
            if let Err(e) = session
                .history_read(
                    release_action,
                    TimestampsToReturn::Neither,
                    true,
                    &release_nodes,
                )
                .await
            {
                log::warn!("Failed to release history_events continuation point: {e}");
            }
            break;
        }

        continuation_point = result.continuation_point;
    }

    Ok(out)
}
