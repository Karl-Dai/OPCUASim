//! Event items captured from event subscriptions (master-side).

use std::collections::VecDeque;
use std::sync::{Arc, RwLock};

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
///
/// Uses `std::sync::RwLock` so the inner write path is callable from
/// synchronous OPC UA `EventCallback` closures (which are not async).
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

    /// Synchronous write — safe to call from an OPC UA event callback
    /// that runs outside a tokio task.
    pub fn add_sync(&self, item: EventItem) {
        if let Ok(mut items) = self.items.write() {
            if items.len() >= self.capacity {
                items.pop_front();
            }
            items.push_back(item);
        }
    }

    pub async fn add(&self, item: EventItem) {
        self.add_sync(item);
    }

    pub async fn items(&self) -> Vec<EventItem> {
        self.items
            .read()
            .map(|items| items.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn items_sync(&self) -> Vec<EventItem> {
        self.items
            .read()
            .map(|items| items.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub async fn clear(&self) {
        if let Ok(mut items) = self.items.write() {
            items.clear();
        }
    }

    pub fn clear_sync(&self) {
        if let Ok(mut items) = self.items.write() {
            items.clear();
        }
    }

    pub fn clone_shared(&self) -> Self {
        Self {
            items: self.items.clone(),
            capacity: self.capacity,
        }
    }

    pub fn len(&self) -> usize {
        self.items.read().map(|items| items.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
