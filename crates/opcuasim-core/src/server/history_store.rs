//! Per-node in-memory ring buffer of historical samples.

use std::collections::{HashMap, VecDeque};

use opcua_types::{DataValue, DateTime, NodeId};
use tokio::sync::RwLock;

/// Per-node ring buffer of historical samples, oldest-first.
pub struct HistoryStore {
    buffers: RwLock<HashMap<NodeId, VecDeque<DataValue>>>,
    capacity: usize,
}

impl HistoryStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffers: RwLock::new(HashMap::new()),
            capacity,
        }
    }

    /// Record a sample; drops oldest when at capacity. No-op when capacity is 0.
    pub async fn record(&self, node_id: &NodeId, dv: DataValue) {
        if self.capacity == 0 {
            return;
        }
        let mut buffers = self.buffers.write().await;
        let buf = buffers.entry(node_id.clone()).or_default();
        if buf.len() >= self.capacity {
            buf.pop_front();
        }
        buf.push_back(dv);
    }

    /// Query samples with timestamp in [start, end], oldest-first, skipping the
    /// first `skip` in-range samples. Returns (samples, next_skip) where
    /// next_skip is Some(skip + returned) when more in-range samples remain.
    pub async fn query(
        &self,
        node_id: &NodeId,
        start: DateTime,
        end: DateTime,
        max_values: u32,
        skip: usize,
    ) -> (Vec<DataValue>, Option<usize>) {
        let buffers = self.buffers.read().await;
        let Some(buf) = buffers.get(node_id) else {
            return (Vec::new(), None);
        };
        let mut in_range: Vec<&DataValue> = buf
            .iter()
            .filter(|dv| sample_time(dv).map(|t| t >= start && t <= end).unwrap_or(false))
            .collect();
        let total = in_range.len();
        if skip >= total {
            return (Vec::new(), None);
        }
        let take = (total - skip).min(max_values as usize);
        let samples: Vec<DataValue> = in_range.drain(skip..skip + take).cloned().collect();
        let next_skip = if skip + take < total {
            Some(skip + take)
        } else {
            None
        };
        (samples, next_skip)
    }

    /// Current sample count for a node.
    pub async fn len(&self, node_id: &NodeId) -> usize {
        self.buffers
            .read()
            .await
            .get(node_id)
            .map(|b| b.len())
            .unwrap_or(0)
    }
}

/// Extract the sample timestamp: source_timestamp, falling back to
/// server_timestamp, else None (sample excluded from range queries).
fn sample_time(dv: &DataValue) -> Option<DateTime> {
    dv.source_timestamp.or(dv.server_timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opcua_types::Variant;

    /// Convert a small seconds value (< 3600) into a deterministic DateTime
    /// on 2026-01-01. Used only for test construction.
    fn dt(secs: i64) -> DateTime {
        let m = (secs / 60) as u16;
        let s = (secs % 60) as u16;
        DateTime::ymd_hms(2026, 1, 1, 0, m, s)
    }

    fn dv(ts_secs: i64, value: i64) -> DataValue {
        DataValue::new_at(Variant::Int64(value), dt(ts_secs))
    }

    #[tokio::test]
    async fn ring_buffer_drops_oldest() {
        let store = HistoryStore::new(3);
        let id = NodeId::new(2, "A");
        for i in 0..5i64 {
            store.record(&id, dv(1000 + i, i)).await;
        }
        assert_eq!(store.len(&id).await, 3);
        let (vals, _) = store
            .query(&id, DateTime::epoch(), DateTime::endtimes(), 100, 0)
            .await;
        assert_eq!(vals.len(), 3);
        // Oldest two dropped; newest three kept, oldest-first
        assert_eq!(
            vals[0].value.as_ref().map(|v| format!("{v}")),
            Some("2".into())
        );
        assert_eq!(
            vals[2].value.as_ref().map(|v| format!("{v}")),
            Some("4".into())
        );
    }

    #[tokio::test]
    async fn query_filters_by_time_range() {
        let store = HistoryStore::new(100);
        let id = NodeId::new(2, "A");
        for i in 0..10i64 {
            store.record(&id, dv(1000 + i * 100, i)).await;
        }
        let start = dt(1200);
        let end = dt(1500);
        let (vals, _) = store.query(&id, start, end, 100, 0).await;
        assert_eq!(vals.len(), 4); // ts 1200,1300,1400,1500
    }

    #[tokio::test]
    async fn query_paginates_with_skip() {
        let store = HistoryStore::new(100);
        let id = NodeId::new(2, "A");
        for i in 0..5i64 {
            store.record(&id, dv(1000 + i, i)).await;
        }
        let (page1, next1) = store
            .query(&id, DateTime::epoch(), DateTime::endtimes(), 2, 0)
            .await;
        assert_eq!(page1.len(), 2);
        assert_eq!(next1, Some(2));
        let (page2, next2) = store
            .query(&id, DateTime::epoch(), DateTime::endtimes(), 2, 2)
            .await;
        assert_eq!(page2.len(), 2);
        assert_eq!(next2, Some(4));
        let (page3, next3) = store
            .query(&id, DateTime::epoch(), DateTime::endtimes(), 2, 4)
            .await;
        assert_eq!(page3.len(), 1);
        assert_eq!(next3, None);
    }

    #[tokio::test]
    async fn zero_capacity_disables() {
        let store = HistoryStore::new(0);
        let id = NodeId::new(2, "A");
        store.record(&id, dv(1000, 1)).await;
        assert_eq!(store.len(&id).await, 0);
        let (vals, _) = store
            .query(&id, DateTime::epoch(), DateTime::endtimes(), 10, 0)
            .await;
        assert!(vals.is_empty());
    }
}
