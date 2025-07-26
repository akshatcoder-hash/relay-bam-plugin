use crate::types::*;
use crate::PLUGIN_STATE;

pub unsafe fn calculate_bundle_fee(bundle: *const TransactionBundle) -> u64 {
    if bundle.is_null() {
        return 0;
    }

    let bundle_ref = match bundle.as_ref() {
        Some(b) => b,
        None => return 0,
    };

    // Get configuration
    let (min_fee, fee_percentage) = match PLUGIN_STATE.lock() {
        Ok(state) => (state.config.min_fee_lamports, state.config.fee_percentage),
        Err(_) => (5000, 0.001), // Fallback to defaults
    };

    // Calculate base fee from transaction count
    let base_fee = min_fee * bundle_ref.transaction_count as u64;

    // Calculate percentage-based fee from total priority fees
    let total_priority_fees = calculate_total_priority_fees(bundle_ref);
    let percentage_fee = (total_priority_fees as f64 * fee_percentage as f64) as u64;

    // Calculate compute-based fee
    let compute_fee = calculate_compute_fee(bundle_ref);

    // Return the maximum of all fee calculations
    base_fee.max(percentage_fee).max(compute_fee)
}

unsafe fn calculate_total_priority_fees(bundle: &TransactionBundle) -> u64 {
    if bundle.transactions.is_null() {
        return 0;
    }

    let transactions = std::slice::from_raw_parts(
        bundle.transactions,
        bundle.transaction_count as usize
    );

    transactions.iter()
        .map(|tx| tx.priority_fee)
        .sum()
}

unsafe fn calculate_compute_fee(bundle: &TransactionBundle) -> u64 {
    if bundle.transactions.is_null() {
        return 0;
    }

    let transactions = std::slice::from_raw_parts(
        bundle.transactions,
        bundle.transaction_count as usize
    );

    let total_compute: u64 = transactions.iter()
        .map(|tx| tx.compute_limit as u64)
        .sum();

    // 1 lamport per 1000 compute units
    total_compute / 1000
}

pub fn estimate_bundle_value(bundle: &TransactionBundle) -> BundleValue {
    let mut value = BundleValue {
        total_priority_fees: 0,
        total_tips: 0,
        estimated_mev: 0,
        plugin_fee: 0,
    };

    unsafe {
        value.total_priority_fees = calculate_total_priority_fees(bundle);
        value.total_tips = bundle.metadata.tip_amount;
        value.plugin_fee = calculate_bundle_fee(bundle);
        
        // MEV estimation would require more complex analysis
        // For now, use a simple heuristic based on priority fees
        value.estimated_mev = value.total_priority_fees / 10;
    }

    value
}

#[derive(Debug, Clone)]
pub struct BundleValue {
    pub total_priority_fees: u64,
    pub total_tips: u64,
    pub estimated_mev: u64,
    pub plugin_fee: u64,
}

impl BundleValue {
    pub fn total(&self) -> u64 {
        self.total_priority_fees + self.total_tips + self.estimated_mev
    }
}