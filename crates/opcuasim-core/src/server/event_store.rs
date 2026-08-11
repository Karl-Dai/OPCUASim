//! Per-source in-memory ring buffer of event field lists.

use std::collections::{HashMap, VecDeque};

use opcua_types::{DateTime, NodeId, Variant};
use tokio::sync::RwLock;

/// Per-source event buffer: node id → ring of (timestamp, field list).
pub type EventBuffers = RwLock<HashMap<NodeId, VecDeque<(DateTime, Vec<Variant>)>>>;

/// Per-source ring buffer of event field lists, oldest-first.
pub struct EventStore {
    buffers: EventBuffers,
    capacity: usize,
}

impl EventStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffers: RwLock::new(HashMap::new()),
            capacity,
        }
    }

    /// Record an event with a timestamp and field list; drops oldest when at
    /// capacity. No-op when capacity is 0.
    pub async fn record(&self, node_id: &NodeId, time: DateTime, fields: Vec<Variant>) {
        if self.capacity == 0 {
            return;
        }
        let mut buffers = self.buffers.write().await;
        let buf = buffers.entry(node_id.clone()).or_default();
        if buf.len() >= self.capacity {
            buf.pop_front();
        }
        buf.push_back((time, fields));
    }

    /// Query events with timestamp in [start, end] (inclusive), oldest-first,
    /// skipping the first `skip` in-range events. Returns (events, next_skip)
    /// where next_skip is Some(skip + returned) when more in-range events remain.
    pub async fn query(
        &self,
        node_id: &NodeId,
        start: DateTime,
        end: DateTime,
        max_values: u32,
        skip: usize,
    ) -> (Vec<(DateTime, Vec<Variant>)>, Option<usize>) {
        let buffers = self.buffers.read().await;
        let Some(buf) = buffers.get(node_id) else {
            return (Vec::new(), None);
        };
        let mut in_range: Vec<&(DateTime, Vec<Variant>)> = buf
            .iter()
            .filter(|(t, _)| *t >= start && *t <= end)
            .collect();
        let total = in_range.len();
        if skip >= total {
            return (Vec::new(), None);
        }
        let take = (total - skip).min(max_values as usize);
        let events: Vec<(DateTime, Vec<Variant>)> =
            in_range.drain(skip..skip + take).cloned().collect();
        let next_skip = if skip + take < total {
            Some(skip + take)
        } else {
            None
        };
        (events, next_skip)
    }

    /// Current event count for a node.
    pub async fn len(&self, node_id: &NodeId) -> usize {
        self.buffers
            .read()
            .await
            .get(node_id)
            .map(|b| b.len())
            .unwrap_or(0)
    }

    /// All events across every source node, oldest-first per source.
    pub async fn all_events(&self) -> Vec<(DateTime, Vec<Variant>)> {
        let buffers = self.buffers.read().await;
        let mut out: Vec<(DateTime, Vec<Variant>)> = Vec::new();
        for buf in buffers.values() {
            out.extend(buf.iter().cloned());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Convert a small seconds value (< 3600) into a deterministic DateTime
    /// on 2026-01-01. Used only for test construction.
    fn dt(secs: i64) -> DateTime {
        let m = (secs / 60) as u16;
        let s = (secs % 60) as u16;
        DateTime::ymd_hms(2026, 1, 1, 0, m, s)
    }

    #[tokio::test]
    async fn ring_buffer_drops_oldest() {
        let store = EventStore::new(3);
        let id = NodeId::new(2, "A");
        for i in 0..5i64 {
            store
                .record(&id, dt(1000 + i), vec![Variant::Int64(i)])
                .await;
        }
        assert_eq!(store.len(&id).await, 3);
        let (vals, _) = store
            .query(&id, DateTime::epoch(), DateTime::endtimes(), 100, 0)
            .await;
        assert_eq!(vals.len(), 3);
        // Oldest two dropped; newest three kept, oldest-first
        assert_eq!(vals[0].1.first().map(|v| format!("{v}")), Some("2".into()));
        assert_eq!(vals[2].1.first().map(|v| format!("{v}")), Some("4".into()));
    }

    #[tokio::test]
    async fn query_filters_by_time_range() {
        let store = EventStore::new(100);
        let id = NodeId::new(2, "A");
        for i in 0..10i64 {
            store
                .record(&id, dt(1000 + i * 100), vec![Variant::Int64(i)])
                .await;
        }
        let start = dt(1200);
        let end = dt(1500);
        let (vals, _) = store.query(&id, start, end, 100, 0).await;
        assert_eq!(vals.len(), 4); // ts 1200,1300,1400,1500 inclusive
    }

    #[tokio::test]
    async fn query_paginates_with_skip() {
        let store = EventStore::new(100);
        let id = NodeId::new(2, "A");
        for i in 0..5i64 {
            store
                .record(&id, dt(1000 + i), vec![Variant::Int64(i)])
                .await;
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
        let store = EventStore::new(0);
        let id = NodeId::new(2, "A");
        store.record(&id, dt(1000), vec![Variant::Int64(1)]).await;
        assert_eq!(store.len(&id).await, 0);
        let (vals, _) = store
            .query(&id, DateTime::epoch(), DateTime::endtimes(), 10, 0)
            .await;
        assert!(vals.is_empty());
    }
}
