use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, TryAcquireError};
use tracing::{debug, warn};

use crate::error::{DnsError, Result};

#[derive(Clone)]
pub struct PermitManager {
    semaphore: Arc<Semaphore>,
    max_concurrent: usize,
    protocol: String,
}

impl PermitManager {
    pub fn new(max_concurrent: usize, protocol: &str) -> Self {
        debug!(
            "Creating PermitManager for {} with {} max concurrent requests",
            protocol, max_concurrent
        );

        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            max_concurrent,
            protocol: protocol.to_string(),
        }
    }

    pub async fn acquire(&self) -> Result<OwnedSemaphorePermit> {
        match self.semaphore.clone().acquire_owned().await {
            Ok(permit) => {
                debug!(
                    "{}: Acquired permit ({}/{})",
                    self.protocol,
                    self.max_concurrent - self.semaphore.available_permits(),
                    self.max_concurrent
                );
                Ok(permit)
            }
            Err(_) => {
                warn!("{}: Failed to acquire permit", self.protocol);
                Err(DnsError::TooManyRequests)
            }
        }
    }

    pub fn try_acquire(&self) -> Result<OwnedSemaphorePermit> {
        match self.semaphore.clone().try_acquire_owned() {
            Ok(permit) => {
                debug!(
                    "{}: Acquired permit (try) ({}/{})",
                    self.protocol,
                    self.max_concurrent - self.semaphore.available_permits(),
                    self.max_concurrent
                );
                Ok(permit)
            }
            Err(TryAcquireError::NoPermits) => {
                warn!("{}: No permits available", self.protocol);
                Err(DnsError::TooManyRequests)
            }
            Err(TryAcquireError::Closed) => {
                warn!("{}: Semaphore closed", self.protocol);
                Err(DnsError::ServerShutdown)
            }
        }
    }

    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }

    pub fn in_use(&self) -> usize {
        self.max_concurrent - self.semaphore.available_permits()
    }
}
