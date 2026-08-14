use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Structured detail payload for frontend localization.
/// When present alongside `LogEntry.detail`, the frontend renders
/// `t("log.{kind}", payload)` instead of the raw `detail` string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailEvent {
    pub kind: String,
    pub payload: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Direction {
    Request,
    Response,
}

impl std::fmt::Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Direction::Request => write!(f, "Request"),
            Direction::Response => write!(f, "Response"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub seq: u64,
    pub timestamp: DateTime<Utc>,
    pub connection_id: String,
    pub direction: Direction,
    pub service: String,
    pub detail: String,
    pub status: Option<String>,
    /// Structured payload for frontend i18n. When present, the frontend
    /// renders `t("log.{kind}", payload)` so the detail text follows the
    /// current UI locale. Legacy logs without this field still deserialize.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub detail_event: Option<DetailEvent>,
}

impl LogEntry {
    pub fn new(
        seq: u64,
        connection_id: String,
        direction: Direction,
        service: String,
        detail: String,
        status: Option<String>,
    ) -> Self {
        Self {
            seq,
            timestamp: Utc::now(),
            connection_id,
            direction,
            service,
            detail,
            status,
            detail_event: None,
        }
    }

    /// Attach a structured detail event for frontend localization.
    pub fn with_detail_event(mut self, kind: impl Into<String>, payload: JsonValue) -> Self {
        self.detail_event = Some(DetailEvent {
            kind: kind.into(),
            payload,
        });
        self
    }

    pub fn to_csv_row(&self) -> String {
        format!(
            "{},{},{},{},{},{}",
            self.timestamp.format("%Y-%m-%d %H:%M:%S%.3f"),
            self.direction,
            self.service,
            self.detail.replace(',', ";"),
            self.status.as_deref().unwrap_or(""),
            self.connection_id,
        )
    }

    pub fn csv_header() -> &'static str {
        "Timestamp,Direction,Service,Detail,Status,ConnectionId"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_entry_round_trip_with_detail_event() {
        use serde_json::json;

        let entry = LogEntry::new(
            1,
            "conn-1".to_string(),
            Direction::Request,
            "Read".to_string(),
            "Read node ns=2;i=42".to_string(),
            Some("Good".to_string()),
        )
        .with_detail_event("read", json!({ "node_id": "ns=2;i=42" }));

        let serialized = serde_json::to_string(&entry).unwrap();
        assert!(serialized.contains("\"detail_event\""));
        assert!(serialized.contains("\"kind\":\"read\""));

        let deserialized: LogEntry = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.seq, 1);
        assert_eq!(deserialized.connection_id, "conn-1");
        assert_eq!(deserialized.service, "Read");

        let event = deserialized
            .detail_event
            .as_ref()
            .expect("detail_event present");
        assert_eq!(event.kind, "read");
        assert_eq!(event.payload["node_id"], "ns=2;i=42");
    }

    #[test]
    fn test_log_entry_round_trip_without_detail_event() {
        let entry = LogEntry::new(
            2,
            "conn-2".to_string(),
            Direction::Response,
            "Write".to_string(),
            "Write node ns=2;i=43".to_string(),
            None,
        );

        let serialized = serde_json::to_string(&entry).unwrap();
        assert!(!serialized.contains("detail_event"));

        let deserialized: LogEntry = serde_json::from_str(&serialized).unwrap();
        assert!(deserialized.detail_event.is_none());
    }
}
