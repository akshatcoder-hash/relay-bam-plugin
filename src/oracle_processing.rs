use crate::types::*;
use crate::oracle::*;
use crate::validation;
use crate::fees;
use crate::PLUGIN_STATE;

#[cfg(feature = "oracle")]
use crate::pyth_client;

use std::time::SystemTime;
use once_cell::sync::Lazy;
use tokio::runtime::Runtime;

static ORACLE_RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create oracle runtime")
});

pub unsafe fn process_oracle_bundle(bundle: *mut TransactionBundle) -> i32 {
    let bundle_ref = match bundle.as_ref() {
        Some(b) => b,
        None => return ERROR_NULL_POINTER,
    };

    // First, run standard V1 validation
    let validation_result = validation::validate_bundle(bundle_ref);
    if validation_result != SUCCESS {
        log::error!("Oracle bundle validation failed with code: {}", validation_result);
        return validation_result;
    }

    // Check oracle feature is enabled
    #[cfg(not(feature = "oracle"))]
    {
        log::warn!("Oracle feature not enabled, falling back to basic processing");
        return crate::processing::process_bundle(bundle);
    }

    #[cfg(feature = "oracle")]
    {
        ORACLE_RUNTIME.block_on(process_oracle_enabled_bundle(bundle_ref))
    }
}

#[cfg(feature = "oracle")]
async fn process_oracle_enabled_bundle(bundle: &TransactionBundle) -> i32 {
    let start_time = SystemTime::now();

    // Step 1: Extract price injection points
    let injection_points = extract_price_injection_points(bundle);
    
    if injection_points.is_empty() {
        log::debug!("No oracle price injection points found, using standard processing");
        return unsafe { crate::processing::process_bundle(bundle as *const _ as *mut _) };
    }

    log::debug!("Found {} oracle price injection points", injection_points.len());

    // Step 2: Ensure fresh oracle data
    let fetch_result = pyth_client::fetch_oracle_prices().await;
    if fetch_result != SUCCESS {
        log::error!("Failed to fetch oracle prices: {}", fetch_result);
        return fetch_result;
    }

    // Step 3: Validate we have all required prices
    for point in &injection_points {
        match pyth_client::get_oracle_price(&point.required_price_id).await {
            Ok(price_data) => {
                let confidence_score = calculate_price_confidence_score(
                    &price_data,
                    SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64,
                );

                if confidence_score < 30 {
                    log::error!(
                        "Price confidence too low ({}%) for injection at tx:{}, inst:{}",
                        confidence_score,
                        point.transaction_index,
                        point.instruction_index
                    );
                    return ERROR_ORACLE_STALE_PRICE;
                }
            }
            Err(error_code) => {
                log::error!(
                    "Missing required price for injection at tx:{}, inst:{} - error: {}",
                    point.transaction_index,
                    point.instruction_index,
                    error_code
                );
                return error_code;
            }
        }
    }

    // Step 4: Calculate oracle-enhanced fees
    let base_fee = unsafe { fees::calculate_bundle_fee(bundle as *const _) };
    let oracle_fee = calculate_oracle_processing_fee(&injection_points);
    let total_required_fee = base_fee + oracle_fee;

    if bundle.metadata.plugin_fees < total_required_fee {
        log::error!(
            "Insufficient fee for oracle processing: {} < {} (base: {}, oracle: {})",
            bundle.metadata.plugin_fees,
            total_required_fee,
            base_fee,
            oracle_fee
        );
        return ERROR_INSUFFICIENT_FEE;
    }

    // Step 5: Perform just-in-time price injection
    // Note: In a real implementation, price injection would modify bundle data
    // For now, we simulate the injection process
    let injection_result = pyth_client::inject_oracle_prices(
        std::ptr::null_mut(), // Placeholder - real implementation would pass mutable bundle
        &injection_points,
    ).await;

    if injection_result != SUCCESS {
        log::error!("Oracle price injection failed: {}", injection_result);
        return injection_result;
    }

    // Step 6: Apply oracle-aware optimizations
    let optimization_result = unsafe { apply_oracle_optimizations(bundle, &injection_points) };
    if optimization_result != SUCCESS {
        return optimization_result;
    }

    // Step 7: Update metrics
    let processing_time = start_time.elapsed().unwrap_or_default().as_micros() as u64;
    
    if let Ok(mut state) = PLUGIN_STATE.lock() {
        state.bundles_processed += 1;
        state.total_fees_collected += bundle.metadata.plugin_fees;
        
        // Update oracle-specific metrics (if state supports them)
        update_oracle_metrics(&mut state, &injection_points, processing_time);
    }

    log::info!(
        "Oracle bundle processed successfully: {} transactions, {} price injections, {}μs",
        bundle.transaction_count,
        injection_points.len(),
        processing_time
    );

    SUCCESS
}

fn calculate_oracle_processing_fee(injection_points: &[PriceInjectionPoint]) -> u64 {
    // Base oracle fee per injection point
    const BASE_ORACLE_FEE: u64 = 10_000; // 0.01 SOL per price injection
    const COMPLEXITY_MULTIPLIER: u64 = 2_000; // Additional fee for complex operations

    let base_total = injection_points.len() as u64 * BASE_ORACLE_FEE;
    
    // Add complexity fee for bundles with many injection points
    let complexity_fee = if injection_points.len() > 5 {
        (injection_points.len() as u64 - 5) * COMPLEXITY_MULTIPLIER
    } else {
        0
    };

    base_total + complexity_fee
}

unsafe fn apply_oracle_optimizations(
    bundle: &TransactionBundle,
    injection_points: &[PriceInjectionPoint],
) -> i32 {
    if bundle.transactions.is_null() || bundle.transaction_count == 0 {
        return SUCCESS;
    }

    let transactions = std::slice::from_raw_parts(
        bundle.transactions,
        bundle.transaction_count as usize,
    );

    // Group transactions by oracle dependency
    let mut oracle_dependent_txs = Vec::new();
    let mut independent_txs = Vec::new();

    for (idx, _tx) in transactions.iter().enumerate() {
        let has_oracle_dependency = injection_points.iter()
            .any(|point| point.transaction_index == idx);

        if has_oracle_dependency {
            oracle_dependent_txs.push(idx);
        } else {
            independent_txs.push(idx);
        }
    }

    // Log optimization suggestions
    if !oracle_dependent_txs.is_empty() && !independent_txs.is_empty() {
        log::debug!(
            "Oracle optimization: {} oracle-dependent txs, {} independent txs",
            oracle_dependent_txs.len(),
            independent_txs.len()
        );
        
        // Suggest optimal ordering: independent first, then oracle-dependent
        log::debug!("Suggested execution order: independent {:?}, then oracle-dependent {:?}",
            independent_txs, oracle_dependent_txs);
    }

    // Check for price feed conflicts
    detect_price_feed_conflicts(injection_points);

    SUCCESS
}

fn detect_price_feed_conflicts(injection_points: &[PriceInjectionPoint]) {
    let mut price_usage = std::collections::HashMap::new();
    
    for point in injection_points {
        let entry = price_usage.entry(point.required_price_id).or_insert(Vec::new());
        entry.push((point.transaction_index, point.instruction_index));
    }

    for (price_id, usages) in price_usage {
        if usages.len() > 1 {
            log::debug!(
                "Price feed {:?} used in {} locations: {:?}",
                hex::encode(price_id),
                usages.len(),
                usages
            );
        }
    }
}

fn update_oracle_metrics(
    state: &mut PluginState,
    injection_points: &[PriceInjectionPoint],
    processing_time_us: u64,
) {
    // These would be added to PluginState in a real implementation
    log::debug!(
        "Oracle metrics: {} injections, {}μs processing time, state.bundles_processed={}",
        injection_points.len(),
        processing_time_us,
        state.bundles_processed
    );
}

// Oracle-aware transaction validation
pub unsafe fn validate_oracle_transactions(bundle: &TransactionBundle) -> i32 {
    if bundle.transactions.is_null() || bundle.transaction_count == 0 {
        return SUCCESS;
    }

    let transactions = std::slice::from_raw_parts(
        bundle.transactions,
        bundle.transaction_count as usize,
    );

    for (tx_idx, transaction) in transactions.iter().enumerate() {
        // Validate oracle-specific constraints
        if let Err(error_code) = validate_oracle_transaction(transaction, tx_idx) {
            return error_code;
        }
    }

    SUCCESS
}

unsafe fn validate_oracle_transaction(transaction: &Transaction, tx_index: usize) -> Result<(), i32> {
    // Check for excessive oracle dependencies
    let oracle_instruction_count = count_oracle_instructions(transaction);
    
    if oracle_instruction_count > 10 {
        log::error!(
            "Transaction {} has too many oracle instructions: {}",
            tx_index,
            oracle_instruction_count
        );
        return Err(ERROR_INVALID_BUNDLE);
    }

    // Validate compute budget for oracle operations
    let estimated_oracle_compute = oracle_instruction_count * 10_000; // 10k CU per oracle op
    if transaction.compute_limit < estimated_oracle_compute {
        log::warn!(
            "Transaction {} may have insufficient compute for oracle operations: {} < {}",
            tx_index,
            transaction.compute_limit,
            estimated_oracle_compute
        );
    }

    Ok(())
}

unsafe fn count_oracle_instructions(transaction: &Transaction) -> u32 {
    if transaction.message.instructions.is_null() {
        return 0;
    }

    let instructions = std::slice::from_raw_parts(
        transaction.message.instructions,
        transaction.message.instructions_count as usize,
    );

    instructions.iter()
        .filter(|inst| crate::oracle::is_price_update_instruction(inst))
        .count() as u32
}

// Export oracle-specific FFI functions
#[no_mangle]
pub extern "C" fn process_oracle_bundle_ffi(bundle: *mut TransactionBundle) -> i32 {
    unsafe { process_oracle_bundle(bundle) }
}

#[no_mangle]
pub extern "C" fn get_oracle_fee_estimate(bundle: *const TransactionBundle) -> u64 {
    if bundle.is_null() {
        return 0;
    }

    unsafe {
        let bundle_ref = bundle.as_ref().unwrap();
        let injection_points = extract_price_injection_points(bundle_ref);
        let base_fee = fees::calculate_bundle_fee(bundle);
        let oracle_fee = calculate_oracle_processing_fee(&injection_points);
        base_fee + oracle_fee
    }
}

#[no_mangle]
pub extern "C" fn get_oracle_injection_count(bundle: *const TransactionBundle) -> u32 {
    if bundle.is_null() {
        return 0;
    }

    unsafe {
        let bundle_ref = bundle.as_ref().unwrap();
        let injection_points = extract_price_injection_points(bundle_ref);
        injection_points.len() as u32
    }
}