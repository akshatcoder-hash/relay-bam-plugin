use crate::PLUGIN_STATE;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn update_processing_metrics(processing_time_us: u64, success: bool) {
    if let Ok(mut state) = PLUGIN_STATE.lock() {
        if !state.config.enable_metrics {
            return;
        }

        // Update average processing time using exponential moving average
        let alpha = 0.1; // Smoothing factor
        let current_avg = state.average_processing_time_us as f64;
        let new_avg = alpha * processing_time_us as f64 + (1.0 - alpha) * current_avg;
        state.average_processing_time_us = new_avg as u64;

        // Track errors
        if !success {
            state.last_error = Some(format!(
                "Processing failed at {}",
                current_timestamp()
            ));
        }

        // Log metrics periodically
        if state.bundles_processed % 100 == 0 && state.bundles_processed > 0 {
            log::info!(
                "Metrics: {} bundles, avg time: {}μs, fees: {} lamports",
                state.bundles_processed,
                state.average_processing_time_us,
                state.total_fees_collected
            );
        }
    }
}

pub fn get_current_metrics() -> MetricsSnapshot {
    let state = PLUGIN_STATE.lock().unwrap();
    
    MetricsSnapshot {
        bundles_processed: state.bundles_processed,
        total_fees_collected: state.total_fees_collected,
        average_processing_time_us: state.average_processing_time_us,
        timestamp: current_timestamp(),
    }
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub bundles_processed: u64,
    pub total_fees_collected: u64,
    pub average_processing_time_us: u64,
    pub timestamp: u64,
}

// Performance tracking for specific operations
pub struct Timer {
    start: std::time::Instant,
    operation: String,
}

impl Timer {
    pub fn new(operation: &str) -> Self {
        Self {
            start: std::time::Instant::now(),
            operation: operation.to_string(),
        }
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed();
        if elapsed.as_millis() > 10 {
            log::warn!(
                "Operation '{}' took {}ms",
                self.operation,
                elapsed.as_millis()
            );
        }
    }
}