use crate::types::*;
use crate::validation;
use crate::fees;
use crate::PLUGIN_STATE;

pub unsafe fn process_bundle(bundle: *mut TransactionBundle) -> i32 {
    // Dereference bundle safely
    let bundle_ref = match bundle.as_ref() {
        Some(b) => b,
        None => return ERROR_NULL_POINTER,
    };

    // Validate bundle structure
    let validation_result = validation::validate_bundle(bundle_ref);
    if validation_result != SUCCESS {
        log::error!("Bundle validation failed with code: {}", validation_result);
        return validation_result;
    }

    // Check bundle size limits
    if let Ok(state) = PLUGIN_STATE.lock() {
        if bundle_ref.transaction_count > state.config.max_bundle_size {
            log::error!(
                "Bundle exceeds max size: {} > {}",
                bundle_ref.transaction_count,
                state.config.max_bundle_size
            );
            return ERROR_INVALID_BUNDLE;
        }
    }

    // Calculate and validate fees
    let required_fee = fees::calculate_bundle_fee(bundle);
    if bundle_ref.metadata.plugin_fees < required_fee {
        log::error!(
            "Insufficient plugin fee: {} < {}",
            bundle_ref.metadata.plugin_fees,
            required_fee
        );
        return ERROR_INSUFFICIENT_FEE;
    }

    // Apply optimizations
    let optimization_result = apply_bundle_optimizations(bundle_ref);
    if optimization_result != SUCCESS {
        return optimization_result;
    }

    // Update state metrics
    if let Ok(mut state) = PLUGIN_STATE.lock() {
        state.bundles_processed += 1;
        state.total_fees_collected += bundle_ref.metadata.plugin_fees;
    }

    log::debug!(
        "Successfully processed bundle with {} transactions",
        bundle_ref.transaction_count
    );

    SUCCESS
}

unsafe fn apply_bundle_optimizations(bundle: &TransactionBundle) -> i32 {
    // IMPORTANT: Do NOT modify BAM Node's memory directly!
    // Instead, analyze and suggest optimizations without mutating
    
    if bundle.transactions.is_null() || bundle.transaction_count == 0 {
        return SUCCESS;
    }

    let transactions = std::slice::from_raw_parts(
        bundle.transactions,
        bundle.transaction_count as usize
    );

    // Calculate optimal ordering without modifying original data
    let mut indices: Vec<usize> = (0..transactions.len()).collect();
    indices.sort_by(|&a, &b| {
        transactions[b].priority_fee.cmp(&transactions[a].priority_fee)
    });
    
    // Log the suggested reordering for BAM Node to use
    log::debug!("Suggested transaction order by priority: {:?}", indices);
    
    // Calculate optimization metrics without mutation
    let total_priority_fees: u64 = transactions.iter()
        .map(|tx| tx.priority_fee)
        .sum();
    
    let total_compute_units: u64 = transactions.iter()
        .map(|tx| tx.compute_limit as u64)
        .sum();
    
    log::debug!(
        "Bundle optimization analysis: {} txs, {} total priority fees, {} total CU",
        transactions.len(),
        total_priority_fees,
        total_compute_units
    );
    
    // Detect potential optimization opportunities
    analyze_optimization_opportunities(transactions);

    SUCCESS
}

unsafe fn analyze_optimization_opportunities(transactions: &[Transaction]) {
    // Check for duplicate priority fees (could be batched)
    let mut fee_counts = std::collections::HashMap::new();
    for tx in transactions {
        *fee_counts.entry(tx.priority_fee).or_insert(0) += 1;
    }
    
    let duplicates: Vec<_> = fee_counts.iter()
        .filter(|(_, &count)| count > 1)
        .collect();
    
    if !duplicates.is_empty() {
        log::debug!("Found {} priority fee groups that could be optimized", duplicates.len());
    }
    
    // Check for overly high compute limits (could be reduced)
    let high_compute_txs = transactions.iter()
        .filter(|tx| tx.compute_limit > 1_000_000)
        .count();
    
    if high_compute_txs > 0 {
        log::debug!("Found {} transactions with high compute limits", high_compute_txs);
    }
}

pub fn get_bundle_stats(bundle: &TransactionBundle) -> BundleStats {
    let mut stats = BundleStats {
        total_compute_units: 0,
        total_priority_fees: 0,
        unique_programs: 0,
        max_accounts_per_tx: 0,
    };

    if bundle.transactions.is_null() {
        return stats;
    }

    unsafe {
        let transactions = std::slice::from_raw_parts(
            bundle.transactions,
            bundle.transaction_count as usize
        );

        for tx in transactions {
            stats.total_compute_units += tx.compute_limit as u64;
            stats.total_priority_fees += tx.priority_fee;
            
            if !tx.message.account_keys.is_null() {
                stats.max_accounts_per_tx = stats.max_accounts_per_tx
                    .max(tx.message.account_keys_count as u32);
            }
        }
    }

    stats
}

#[derive(Debug, Clone)]
pub struct BundleStats {
    pub total_compute_units: u64,
    pub total_priority_fees: u64,
    pub unique_programs: u32,
    pub max_accounts_per_tx: u32,
}