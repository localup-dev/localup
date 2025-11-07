//! Reconnection logic with exponential backoff

use std::time::Duration;
use thiserror::Error;
use tokio::time::sleep;
use tracing::debug;

/// Reconnection configuration
#[derive(Debug, Clone)]
pub struct ReconnectConfig {
    /// Initial backoff duration
    pub initial_backoff: Duration,
    /// Maximum backoff duration
    pub max_backoff: Duration,
    /// Backoff multiplier
    pub multiplier: f64,
    /// Maximum number of reconnection attempts (None = unlimited)
    pub max_attempts: Option<usize>,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            initial_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(60),
            multiplier: 2.0,
            max_attempts: None,
        }
    }
}

/// Reconnection errors
#[derive(Debug, Error)]
pub enum ReconnectError {
    #[error("Max reconnection attempts reached")]
    MaxAttemptsReached,

    #[error("Reconnection cancelled")]
    Cancelled,
}

/// Reconnection manager with exponential backoff
pub struct ReconnectManager {
    config: ReconnectConfig,
    current_backoff: Duration,
    attempt: usize,
}

impl ReconnectManager {
    pub fn new(config: ReconnectConfig) -> Self {
        Self {
            current_backoff: config.initial_backoff,
            config,
            attempt: 0,
        }
    }

    /// Wait before next reconnection attempt
    pub async fn wait(&mut self) -> Result<(), ReconnectError> {
        self.attempt += 1;

        if let Some(max_attempts) = self.config.max_attempts {
            if self.attempt > max_attempts {
                return Err(ReconnectError::MaxAttemptsReached);
            }
        }

        debug!(
            "Waiting {}s before reconnection attempt {}",
            self.current_backoff.as_secs(),
            self.attempt
        );

        sleep(self.current_backoff).await;

        // Increase backoff
        let next_backoff =
            Duration::from_secs_f64(self.current_backoff.as_secs_f64() * self.config.multiplier);

        self.current_backoff = next_backoff.min(self.config.max_backoff);

        Ok(())
    }

    /// Reset backoff (call after successful connection)
    pub fn reset(&mut self) {
        debug!("Resetting reconnection backoff");
        self.current_backoff = self.config.initial_backoff;
        self.attempt = 0;
    }

    /// Get current attempt number
    pub fn attempt(&self) -> usize {
        self.attempt
    }

    /// Get current backoff duration
    pub fn current_backoff(&self) -> Duration {
        self.current_backoff
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_reconnect_backoff() {
        let config = ReconnectConfig {
            initial_backoff: Duration::from_millis(10),
            max_backoff: Duration::from_millis(100),
            multiplier: 2.0,
            max_attempts: None,
        };

        let mut manager = ReconnectManager::new(config);

        assert_eq!(manager.attempt(), 0);
        assert_eq!(manager.current_backoff(), Duration::from_millis(10));

        manager.wait().await.unwrap();
        assert_eq!(manager.attempt(), 1);
        assert_eq!(manager.current_backoff(), Duration::from_millis(20));

        manager.wait().await.unwrap();
        assert_eq!(manager.attempt(), 2);
        assert_eq!(manager.current_backoff(), Duration::from_millis(40));

        manager.wait().await.unwrap();
        assert_eq!(manager.attempt(), 3);
        assert_eq!(manager.current_backoff(), Duration::from_millis(80));

        manager.wait().await.unwrap();
        assert_eq!(manager.attempt(), 4);
        // Should cap at max_backoff
        assert_eq!(manager.current_backoff(), Duration::from_millis(100));
    }

    #[tokio::test]
    async fn test_reconnect_reset() {
        let config = ReconnectConfig {
            initial_backoff: Duration::from_millis(10),
            max_backoff: Duration::from_millis(100),
            multiplier: 2.0,
            max_attempts: None,
        };

        let mut manager = ReconnectManager::new(config);

        manager.wait().await.unwrap();
        manager.wait().await.unwrap();

        assert_eq!(manager.attempt(), 2);

        manager.reset();

        assert_eq!(manager.attempt(), 0);
        assert_eq!(manager.current_backoff(), Duration::from_millis(10));
    }

    #[tokio::test]
    async fn test_max_attempts() {
        let config = ReconnectConfig {
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(10),
            multiplier: 2.0,
            max_attempts: Some(3),
        };

        let mut manager = ReconnectManager::new(config);

        assert!(manager.wait().await.is_ok());
        assert!(manager.wait().await.is_ok());
        assert!(manager.wait().await.is_ok());

        let result = manager.wait().await;
        assert!(result.is_err());
        assert!(matches!(result, Err(ReconnectError::MaxAttemptsReached)));
    }
}
