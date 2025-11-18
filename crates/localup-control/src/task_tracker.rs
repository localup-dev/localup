//! Task tracking for tunnel-related background tasks
//!
//! Tracks JoinHandle abort handles for tasks like TCP proxy servers,
//! allowing cleanup when tunnels disconnect.

use std::collections::HashMap;
use std::sync::Mutex;
use tokio::task::JoinHandle;

/// Tracks background tasks associated with tunnels
pub struct TaskTracker {
    /// Map of localup_id -> JoinHandle abort handle
    tasks: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
}

impl TaskTracker {
    /// Create a new task tracker
    pub fn new() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
        }
    }

    /// Register a task for a tunnel
    pub fn register(&self, localup_id: String, handle: JoinHandle<()>) {
        if let Ok(mut tasks) = self.tasks.lock() {
            // If there was a previous task, abort it first
            if let Some(old_handle) = tasks.remove(&localup_id) {
                old_handle.abort();
            }
            tasks.insert(localup_id, handle);
        }
    }

    /// Unregister and abort a task for a tunnel
    pub fn unregister(&self, localup_id: &str) {
        if let Ok(mut tasks) = self.tasks.lock() {
            if let Some(handle) = tasks.remove(localup_id) {
                handle.abort();
            }
        }
    }
}

impl Default for TaskTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_and_unregister() {
        let tracker = TaskTracker::new();

        // Create a simple task
        let handle =
            tokio::spawn(async { tokio::time::sleep(std::time::Duration::from_secs(10)).await });

        tracker.register("test-tunnel".to_string(), handle);

        // Unregister the task
        tracker.unregister("test-tunnel");

        // Verify the task is gone
        assert_eq!(tracker.tasks.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_replacing_task() {
        let tracker = TaskTracker::new();

        // Register first task
        let handle1 =
            tokio::spawn(async { tokio::time::sleep(std::time::Duration::from_secs(10)).await });
        tracker.register("test-tunnel".to_string(), handle1);

        assert_eq!(tracker.tasks.lock().unwrap().len(), 1);

        // Register second task for same tunnel (should replace first)
        let handle2 =
            tokio::spawn(async { tokio::time::sleep(std::time::Duration::from_secs(10)).await });
        tracker.register("test-tunnel".to_string(), handle2);

        // Should still be only one task
        assert_eq!(tracker.tasks.lock().unwrap().len(), 1);
    }
}
