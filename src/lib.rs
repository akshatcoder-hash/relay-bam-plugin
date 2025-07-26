use libc::c_char;
use once_cell::sync::Lazy;
use std::sync::Mutex;

mod types;
mod processing;
mod validation;
mod fees;
mod metrics;

// Re-export public types and functions
pub use crate::types::*;

// Global plugin state
static PLUGIN_STATE: Lazy<Mutex<PluginState>> = Lazy::new(|| {
    Mutex::new(PluginState::default())
});

// Plugin name as C string
static PLUGIN_NAME: &[u8] = b"RelayPlugin\0";

// Main plugin interface export
#[no_mangle]
pub static PLUGIN_INTERFACE: PluginInterface = PluginInterface {
    version: 1,
    capabilities: CAPABILITY_BUNDLE_PROCESSING | CAPABILITY_FEE_COLLECTION,
    name: PLUGIN_NAME.as_ptr() as *const c_char,
    init: plugin_init,
    shutdown: plugin_shutdown,
    process_bundle: process_bundle_forwarding,
    get_fee_estimate: estimate_forwarding_fee,
    get_state: get_plugin_state,
    set_state: set_plugin_state,
};

// Initialize plugin with configuration
#[no_mangle]
pub extern "C" fn plugin_init(config_data: *const u8, config_len: usize) -> i32 {
    // Validate input
    if config_data.is_null() && config_len > 0 {
        return ERROR_NULL_POINTER;
    }

    // Load configuration if provided
    if config_len > 0 {
        let config_slice = unsafe { std::slice::from_raw_parts(config_data, config_len) };
        
        match serde_json::from_slice::<PluginConfig>(config_slice) {
            Ok(config) => {
                if let Ok(mut state) = PLUGIN_STATE.lock() {
                    state.config = config;
                    log::info!("Plugin initialized with custom config");
                } else {
                    return ERROR_INVALID_STATE;
                }
            }
            Err(e) => {
                log::error!("Failed to parse config: {}", e);
                return ERROR_INVALID_STATE;
            }
        }
    }

    log::info!("Relay BAM Plugin v{} initialized", env!("CARGO_PKG_VERSION"));
    SUCCESS
}

// Shutdown plugin cleanly
#[no_mangle]
pub extern "C" fn plugin_shutdown() -> i32 {
    log::info!("Relay BAM Plugin shutting down");
    
    // Log final metrics
    if let Ok(state) = PLUGIN_STATE.lock() {
        log::info!(
            "Final stats: {} bundles processed, {} lamports collected",
            state.bundles_processed,
            state.total_fees_collected
        );
    }
    
    SUCCESS
}

// Process transaction bundle
#[no_mangle]
pub extern "C" fn process_bundle_forwarding(bundle: *mut TransactionBundle) -> i32 {
    // Start timing
    let start_time = std::time::Instant::now();
    
    // Validate bundle pointer
    if bundle.is_null() {
        log::error!("Received null bundle pointer");
        return ERROR_NULL_POINTER;
    }

    // Perform bundle processing
    let result = unsafe { processing::process_bundle(bundle) };
    
    // Update metrics
    let processing_time = start_time.elapsed().as_micros() as u64;
    metrics::update_processing_metrics(processing_time, result == SUCCESS);
    
    result
}

// Estimate fee for bundle
#[no_mangle]
pub extern "C" fn estimate_forwarding_fee(bundle: *const TransactionBundle) -> u64 {
    if bundle.is_null() {
        return 0;
    }

    unsafe { fees::calculate_bundle_fee(bundle) }
}

// Get current plugin state
#[no_mangle]
pub extern "C" fn get_plugin_state(state_buffer: *mut u8, buffer_len: usize) -> i32 {
    if state_buffer.is_null() {
        return ERROR_NULL_POINTER;
    }

    let state = match PLUGIN_STATE.lock() {
        Ok(s) => s.clone(),
        Err(_) => return ERROR_INVALID_STATE,
    };

    let serialized = match serde_json::to_vec(&state) {
        Ok(data) => data,
        Err(_) => return ERROR_INVALID_STATE,
    };

    if serialized.len() > buffer_len {
        return ERROR_INVALID_STATE;
    }

    unsafe {
        std::ptr::copy_nonoverlapping(serialized.as_ptr(), state_buffer, serialized.len());
    }

    serialized.len() as i32
}

// Set plugin state/config
#[no_mangle]
pub extern "C" fn set_plugin_state(state_data: *const u8, data_len: usize) -> i32 {
    if state_data.is_null() {
        return ERROR_NULL_POINTER;
    }

    let state_slice = unsafe { std::slice::from_raw_parts(state_data, data_len) };
    
    match serde_json::from_slice::<PluginState>(state_slice) {
        Ok(new_state) => {
            match PLUGIN_STATE.lock() {
                Ok(mut state) => {
                    *state = new_state;
                    SUCCESS
                }
                Err(_) => ERROR_INVALID_STATE,
            }
        }
        Err(_) => ERROR_INVALID_STATE,
    }
}

// Export additional utility functions
#[no_mangle]
pub extern "C" fn relay_plugin_version() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn relay_plugin_capabilities() -> u32 {
    CAPABILITY_BUNDLE_PROCESSING | CAPABILITY_FEE_COLLECTION
}

// Module tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_init_and_shutdown() {
        let result = plugin_init(std::ptr::null(), 0);
        assert_eq!(result, SUCCESS);
        
        let result = plugin_shutdown();
        assert_eq!(result, SUCCESS);
    }

    #[test]
    fn test_null_bundle_handling() {
        let result = process_bundle_forwarding(std::ptr::null_mut());
        assert_eq!(result, ERROR_NULL_POINTER);
        
        let fee = estimate_forwarding_fee(std::ptr::null());
        assert_eq!(fee, 0);
    }

    #[test]
    fn test_plugin_version() {
        assert_eq!(relay_plugin_version(), 1);
        assert_eq!(
            relay_plugin_capabilities(),
            CAPABILITY_BUNDLE_PROCESSING | CAPABILITY_FEE_COLLECTION
        );
    }
}