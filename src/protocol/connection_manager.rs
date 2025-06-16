use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time::interval;
use tracing::{debug, warn};

pub trait ConnectionState: Send + Sync {
    fn id(&self) -> u64;
    fn last_activity(&self) -> Instant;
    fn update_activity(&mut self);
    fn is_idle(&self, timeout: Duration) -> bool;
}

#[derive(Debug)]
pub struct ConnectionStats {
    pub active_connections: usize,
    pub total_connections: u64,
    pub idle_connections: usize,
}

pub struct ConnectionManager<T: ConnectionState> {
    connections: Arc<DashMap<u64, Arc<Mutex<T>>>>,
    max_connections: usize,
    idle_timeout: Duration,
    next_id: AtomicU64,
    total_connections: AtomicU64,
}

impl<T: ConnectionState + 'static> ConnectionManager<T> {
    pub fn new(max_connections: usize, idle_timeout: Duration) -> Self {
        let manager = Self {
            connections: Arc::new(DashMap::new()),
            max_connections,
            idle_timeout,
            next_id: AtomicU64::new(1),
            total_connections: AtomicU64::new(0),
        };

        // Start cleanup task
        let connections_clone = manager.connections.clone();
        let idle_timeout = manager.idle_timeout;

        tokio::spawn(async move {
            let mut cleanup_interval = interval(Duration::from_secs(30));

            loop {
                cleanup_interval.tick().await;

                let mut to_remove = Vec::new();

                // Find idle connections
                for entry in connections_clone.iter() {
                    let id = *entry.key();
                    let conn = entry.value();

                    if let Ok(state) = conn.try_lock() {
                        if state.is_idle(idle_timeout) {
                            to_remove.push(id);
                        }
                    }
                }

                // Remove idle connections
                for id in to_remove {
                    connections_clone.remove(&id);
                    debug!("Removed idle connection {}", id);
                }

                debug!(
                    "Connection cleanup: {} active connections",
                    connections_clone.len()
                );
            }
        });

        manager
    }

    pub async fn add_connection(&self, state: T) -> Option<u64> {
        if self.connections.len() >= self.max_connections {
            warn!(
                "Connection limit reached ({}/{})",
                self.connections.len(),
                self.max_connections
            );
            return None;
        }

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.connections.insert(id, Arc::new(Mutex::new(state)));
        self.total_connections.fetch_add(1, Ordering::Relaxed);

        debug!(
            "Added connection {} ({}/{})",
            id,
            self.connections.len(),
            self.max_connections
        );

        Some(id)
    }

    pub async fn remove_connection(&self, id: u64) {
        if self.connections.remove(&id).is_some() {
            debug!(
                "Removed connection {} ({} remaining)",
                id,
                self.connections.len()
            );
        }
    }

    pub async fn get_connection(&self, id: u64) -> Option<Arc<Mutex<T>>> {
        self.connections.get(&id).map(|entry| entry.clone())
    }

    pub async fn update_activity(&self, id: u64) {
        if let Some(conn) = self.connections.get(&id) {
            let mut state = conn.lock().await;
            state.update_activity();
        }
    }

    pub async fn get_stats(&self) -> ConnectionStats {
        let mut idle_count = 0;

        for entry in self.connections.iter() {
            if let Ok(state) = entry.value().try_lock() {
                if state.is_idle(self.idle_timeout) {
                    idle_count += 1;
                }
            }
        }

        ConnectionStats {
            active_connections: self.connections.len(),
            total_connections: self.total_connections.load(Ordering::Relaxed),
            idle_connections: idle_count,
        }
    }

    pub fn active_connections(&self) -> usize {
        self.connections.len()
    }

    pub fn is_full(&self) -> bool {
        self.connections.len() >= self.max_connections
    }
}

// Simple connection state for stateless protocols
#[derive(Debug)]
pub struct StatelessConnection {
    id: u64,
    last_activity: Instant,
}

impl StatelessConnection {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            last_activity: Instant::now(),
        }
    }
}

impl ConnectionState for StatelessConnection {
    fn id(&self) -> u64 {
        self.id
    }

    fn last_activity(&self) -> Instant {
        self.last_activity
    }

    fn update_activity(&mut self) {
        self.last_activity = Instant::now();
    }

    fn is_idle(&self, timeout: Duration) -> bool {
        self.last_activity.elapsed() > timeout
    }
}
