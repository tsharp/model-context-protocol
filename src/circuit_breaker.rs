//! Circuit breaker pattern for resilient server connections.
//!
//! Implements a simple circuit breaker that tracks failures and prevents
//! cascading failures by temporarily blocking requests to unhealthy servers.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use parking_lot::RwLock;

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation - requests are allowed
    Closed,
    /// Circuit is open - requests are blocked
    Open,
    /// Testing if service has recovered - limited requests allowed
    HalfOpen,
}

/// Configuration for circuit breaker behavior
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening the circuit
    pub failure_threshold: u32,
    /// Duration to keep the circuit open before trying half-open
    pub open_duration: Duration,
    /// Number of successful requests in half-open state to close the circuit
    pub half_open_successes: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            open_duration: Duration::from_secs(30),
            half_open_successes: 2,
        }
    }
}

/// Circuit breaker for managing connection health
#[derive(Debug)]
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: RwLock<CircuitState>,
    failure_count: AtomicU32,
    success_count: AtomicU32,
    last_failure_time: RwLock<Option<Instant>>,
    total_requests: AtomicU64,
    total_failures: AtomicU64,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with default configuration
    pub fn new() -> Self {
        Self::with_config(CircuitBreakerConfig::default())
    }

    /// Create a new circuit breaker with custom configuration
    pub fn with_config(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: RwLock::new(CircuitState::Closed),
            failure_count: AtomicU32::new(0),
            success_count: AtomicU32::new(0),
            last_failure_time: RwLock::new(None),
            total_requests: AtomicU64::new(0),
            total_failures: AtomicU64::new(0),
        }
    }

    /// Get the current circuit state
    pub fn state(&self) -> CircuitState {
        *self.state.read()
    }

    /// Check if a request should be allowed
    /// 
    /// Returns `true` if the request can proceed, `false` if it should be blocked.
    pub fn allow_request(&self) -> bool {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        
        let current_state = *self.state.read();
        
        match current_state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if we should transition to half-open
                if let Some(last_failure) = *self.last_failure_time.read() {
                    if last_failure.elapsed() >= self.config.open_duration {
                        let mut state = self.state.write();
                        if *state == CircuitState::Open {
                            *state = CircuitState::HalfOpen;
                            self.success_count.store(0, Ordering::Relaxed);
                            drop(state);
                            return true;
                        }
                    }
                }
                false
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Record a successful request
    pub fn record_success(&self) {
        let current_state = *self.state.read();
        
        match current_state {
            CircuitState::Closed => {
                // Reset failure count on success
                self.failure_count.store(0, Ordering::Relaxed);
            }
            CircuitState::HalfOpen => {
                let successes = self.success_count.fetch_add(1, Ordering::Relaxed) + 1;
                if successes >= self.config.half_open_successes {
                    let mut state = self.state.write();
                    *state = CircuitState::Closed;
                    self.failure_count.store(0, Ordering::Relaxed);
                    self.success_count.store(0, Ordering::Relaxed);
                }
            }
            CircuitState::Open => {
                // Shouldn't happen, but reset if it does
            }
        }
    }

    /// Record a failed request
    pub fn record_failure(&self) {
        self.total_failures.fetch_add(1, Ordering::Relaxed);
        *self.last_failure_time.write() = Some(Instant::now());
        
        let current_state = *self.state.read();
        
        match current_state {
            CircuitState::Closed => {
                let failures = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
                if failures >= self.config.failure_threshold {
                    let mut state = self.state.write();
                    *state = CircuitState::Open;
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in half-open immediately opens the circuit
                let mut state = self.state.write();
                *state = CircuitState::Open;
                self.success_count.store(0, Ordering::Relaxed);
            }
            CircuitState::Open => {
                // Already open, nothing to do
            }
        }
    }

    /// Manually reset the circuit breaker to closed state
    pub fn reset(&self) {
        let mut state = self.state.write();
        *state = CircuitState::Closed;
        self.failure_count.store(0, Ordering::Relaxed);
        self.success_count.store(0, Ordering::Relaxed);
    }

    /// Get statistics about the circuit breaker
    pub fn stats(&self) -> CircuitBreakerStats {
        CircuitBreakerStats {
            state: *self.state.read(),
            failure_count: self.failure_count.load(Ordering::Relaxed),
            total_requests: self.total_requests.load(Ordering::Relaxed),
            total_failures: self.total_failures.load(Ordering::Relaxed),
        }
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about circuit breaker state
#[derive(Debug, Clone)]
pub struct CircuitBreakerStats {
    pub state: CircuitState,
    pub failure_count: u32,
    pub total_requests: u64,
    pub total_failures: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_starts_closed() {
        let cb = CircuitBreaker::new();
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.allow_request());
    }

    #[test]
    fn test_circuit_opens_after_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let cb = CircuitBreaker::with_config(config);
        
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
        
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.allow_request());
    }

    #[test]
    fn test_success_resets_failure_count() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let cb = CircuitBreaker::with_config(config);
        
        cb.record_failure();
        cb.record_failure();
        cb.record_success();
        cb.record_failure();
        cb.record_failure();
        
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_manual_reset() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            ..Default::default()
        };
        let cb = CircuitBreaker::with_config(config);
        
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        
        cb.reset();
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.allow_request());
    }
}
