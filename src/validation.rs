use crate::types::*;

pub unsafe fn validate_bundle(bundle: &TransactionBundle) -> i32 {
    // Validate basic bundle structure
    if bundle.transaction_count == 0 {
        log::error!("Bundle contains no transactions");
        return ERROR_INVALID_BUNDLE;
    }

    if bundle.transactions.is_null() {
        log::error!("Bundle transactions pointer is null");
        return ERROR_NULL_POINTER;
    }

    // Validate metadata
    if let Err(e) = validate_metadata(&bundle.metadata) {
        log::error!("Invalid bundle metadata: {}", e);
        return ERROR_INVALID_BUNDLE;
    }

    // Validate attestation if present
    if !bundle.attestation.is_null() {
        if let Err(e) = validate_attestation(bundle.attestation) {
            log::error!("Invalid attestation: {}", e);
            return ERROR_INVALID_BUNDLE;
        }
    }

    // Validate each transaction
    let transactions = std::slice::from_raw_parts(
        bundle.transactions,
        bundle.transaction_count as usize
    );

    for (idx, tx) in transactions.iter().enumerate() {
        if let Err(e) = validate_transaction(tx) {
            log::error!("Invalid transaction at index {}: {}", idx, e);
            return ERROR_INVALID_BUNDLE;
        }
    }

    SUCCESS
}

fn validate_metadata(metadata: &BundleMetadata) -> Result<(), &'static str> {
    // Check slot is reasonable (not 0, not too far in future)
    if metadata.slot == 0 {
        return Err("Invalid slot: 0");
    }

    // Check timestamp is reasonable
    let current_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    let time_diff = (current_time as i64 - metadata.timestamp as i64).abs();
    if time_diff > 300 { // More than 5 minutes difference
        return Err("Timestamp too far from current time");
    }

    // Validate leader pubkey is not all zeros
    if metadata.leader_pubkey.iter().all(|&b| b == 0) {
        return Err("Invalid leader pubkey");
    }

    Ok(())
}

unsafe fn validate_attestation(attestation: *mut Attestation) -> Result<(), &'static str> {
    let attestation_ref = attestation.as_ref()
        .ok_or("Null attestation pointer")?;

    // Check version
    if attestation_ref.version == 0 || attestation_ref.version > 10 {
        return Err("Invalid attestation version");
    }

    // Validate node ID
    if attestation_ref.node_id.iter().all(|&b| b == 0) {
        return Err("Invalid node ID");
    }

    // Validate bundle hash
    if attestation_ref.bundle_hash.iter().all(|&b| b == 0) {
        return Err("Invalid bundle hash");
    }

    // Check TEE report if present
    if !attestation_ref.tee_report.is_null() && attestation_ref.tee_report_len == 0 {
        return Err("Invalid TEE report length");
    }

    Ok(())
}

unsafe fn validate_transaction(tx: &Transaction) -> Result<(), &'static str> {
    // Validate signature count and pointer
    if tx.signature_count == 0 {
        return Err("No signatures");
    }

    if tx.signature_count > 8 {
        return Err("Too many signatures");
    }

    if tx.signatures.is_null() {
        return Err("Null signatures pointer");
    }

    // Validate message
    validate_message(&tx.message)?;

    // Validate compute limits
    if tx.compute_limit == 0 {
        return Err("Zero compute limit");
    }

    if tx.compute_limit > 1_400_000 {
        return Err("Compute limit exceeds maximum");
    }

    Ok(())
}

fn validate_message(msg: &TransactionMessage) -> Result<(), &'static str> {
    // Validate header
    if msg.header.num_required_signatures == 0 {
        return Err("No required signatures");
    }

    if msg.header.num_required_signatures > msg.account_keys_count {
        return Err("More required signatures than accounts");
    }

    // Validate account keys
    if msg.account_keys_count == 0 {
        return Err("No account keys");
    }

    if msg.account_keys.is_null() {
        return Err("Null account keys pointer");
    }

    // Validate instructions
    if msg.instructions_count == 0 {
        return Err("No instructions");
    }

    if msg.instructions.is_null() {
        return Err("Null instructions pointer");
    }

    // Validate blockhash
    if msg.recent_blockhash.iter().all(|&b| b == 0) {
        return Err("Invalid blockhash");
    }

    Ok(())
}