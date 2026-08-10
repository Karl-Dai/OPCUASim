//! Event items captured from event subscriptions (master-side).

use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A single event notification, fields flattened for UI display.
#[derive(Debug, Clone)]
pub struct EventItem {
    pub time: String,
    pub severity: u16,
    pub source: String,
    pub message: String,
    pub event_type: String,
}

/// Ring buffer of received events (master-side display log).
pub struct EventLog {
    items: Arc<RwLock<VecDeque<EventItem>>>,
    capacity: usize,
}

impl EventLog {
    pub fn new(capacity: usize) -> Self {
        Self {
            items: Arc::new(RwLock::new(VecDeque::new())),
            capacity,
        }
    }

    pub async fn add(&self, item: EventItem) {
        let mut items = self.items.write().await;
        if items.len() >= self.capacity {
            items.pop_front();
        }
        items.push_back(item);
    }

    pub async fn items(&self) -> Vec<EventItem> {
        self.items.read().await.iter().cloned().collect()
    }

    pub async fn clear(&self) {
        self.items.write().await.clear();
    }
}
