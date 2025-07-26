use crate::types::*;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[repr(C)]
#[derive(Debug, Clone)]
pub struct PriceData {
    pub price: i64,
    pub conf: u64,
    pub expo: i32,
    pub publish_time: i64,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct OracleUpdate {
    pub price_id: [u8; 32],
    pub price_data: PriceData,
    pub verification_level: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythPriceAccount {
    pub magic: u32,
    pub version: u32,
    pub atype: u32,
    pub size: u32,
    pub price_type: u32,
    pub exponent: i32,
    pub num_component_prices: u32,
    pub num_quoters: u32,
    pub last_slot: u64,
    pub valid_slot: u64,
    pub ema_price: PythPrice,
    pub ema_confidence: PythPrice,
    pub timestamp: i64,
    pub min_publishers: u8,
    pub drv2: u8,
    pub drv3: u16,
    pub drv4: u32,
    pub product_account: [u8; 32],
    pub next_price_account: [u8; 32],
    pub prev_slot: u64,
    pub prev_price: i64,
    pub prev_confidence: u64,
    pub prev_timestamp: i64,
    pub agg: PythPriceInfo,
    pub comp: Vec<PythPriceInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythPrice {
    pub val: i64,
    pub numer: u64,
    pub denom: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythPriceInfo {
    pub price: i64,
    pub conf: u64,
    pub status: u32,
    pub corp_act: u32,
    pub pub_slot: u64,
}

#[derive(Debug, Clone)]
pub struct OracleCache {
    pub prices: lru::LruCache<[u8; 32], PriceData>,
    pub last_update: SystemTime,
    pub update_count: u64,
}

impl Default for OracleCache {
    fn default() -> Self {
        Self {
            prices: lru::LruCache::new(std::num::NonZeroUsize::new(1000).unwrap()),
            last_update: UNIX_EPOCH,
            update_count: 0,
        }
    }
}

impl OracleCache {
    pub fn get_price(&mut self, price_id: &[u8; 32]) -> Option<&PriceData> {
        self.prices.get(price_id)
    }

    pub fn update_price(&mut self, price_id: [u8; 32], price_data: PriceData) {
        self.prices.put(price_id, price_data);
        self.last_update = SystemTime::now();
        self.update_count += 1;
    }

    pub fn is_stale(&self, max_age_seconds: u64) -> bool {
        match self.last_update.duration_since(UNIX_EPOCH) {
            Ok(duration) => {
                let current_time = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                current_time - duration.as_secs() > max_age_seconds
            }
            Err(_) => true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleConfig {
    pub pyth_cluster_url: String,
    pub price_account_keys: Vec<String>,
    pub max_price_age_seconds: u64,
    pub update_interval_ms: u64,
    pub verification_level: u8,
    pub enable_just_in_time_updates: bool,
}

impl Default for OracleConfig {
    fn default() -> Self {
        Self {
            pyth_cluster_url: "https://api.mainnet-beta.solana.com".to_string(),
            price_account_keys: vec![
                "GVXRSBjFk6e6J3NbVPXohDJetcTjaeeuykUpbQF8UoMU".to_string(), // BTC/USD
                "H6ARHf6YXhGYeQfUzQNGk6rDNnLBQKrenN712K4AQJEG".to_string(), // ETH/USD
                "Gnt27xtC473ZT2Mw5u8wZ68Z3gULkSTb5DuxJy7eJotD".to_string(), // SOL/USD
            ],
            max_price_age_seconds: 30,
            update_interval_ms: 1000,
            verification_level: 2,
            enable_just_in_time_updates: true,
        }
    }
}


pub const PRICE_STATUS_UNKNOWN: u32 = 0;
pub const PRICE_STATUS_TRADING: u32 = 1;
pub const PRICE_STATUS_HALTED: u32 = 2;
pub const PRICE_STATUS_AUCTION: u32 = 3;

pub const VERIFICATION_LEVEL_NONE: u8 = 0;
pub const VERIFICATION_LEVEL_BASIC: u8 = 1;
pub const VERIFICATION_LEVEL_STRICT: u8 = 2;

#[derive(Debug, Clone)]
pub struct PriceInjectionPoint {
    pub transaction_index: usize,
    pub instruction_index: usize,
    pub price_account: [u8; 32],
    pub required_price_id: [u8; 32],
}

pub fn extract_price_injection_points(bundle: &TransactionBundle) -> Vec<PriceInjectionPoint> {
    let mut injection_points = Vec::new();
    
    if bundle.transactions.is_null() || bundle.transaction_count == 0 {
        return injection_points;
    }

    unsafe {
        let transactions = std::slice::from_raw_parts(
            bundle.transactions,
            bundle.transaction_count as usize,
        );

        for (tx_idx, transaction) in transactions.iter().enumerate() {
            if transaction.message.instructions.is_null() {
                continue;
            }

            let instructions = std::slice::from_raw_parts(
                transaction.message.instructions,
                transaction.message.instructions_count as usize,
            );

            for (inst_idx, instruction) in instructions.iter().enumerate() {
                if is_price_update_instruction(instruction) {
                    if let Some(price_account) = extract_price_account(instruction, &transaction.message) {
                        injection_points.push(PriceInjectionPoint {
                            transaction_index: tx_idx,
                            instruction_index: inst_idx,
                            price_account,
                            required_price_id: derive_price_id_from_account(&price_account),
                        });
                    }
                }
            }
        }
    }

    injection_points
}

pub unsafe fn is_price_update_instruction(instruction: &CompiledInstruction) -> bool {
    if instruction.data.is_null() || instruction.data_len < 8 {
        return false;
    }

    let instruction_data = std::slice::from_raw_parts(instruction.data, instruction.data_len.into());
    
    // Check for Pyth program instruction discriminators
    instruction_data.len() >= 8 && (
        instruction_data[0..8] == [0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00] || // update_price
        instruction_data[0..8] == [0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]    // update_price_feeds
    )
}

unsafe fn extract_price_account(instruction: &CompiledInstruction, message: &TransactionMessage) -> Option<[u8; 32]> {
    if instruction.accounts.is_null() || instruction.accounts_count == 0 {
        return None;
    }

    if message.account_keys.is_null() || message.account_keys_count == 0 {
        return None;
    }

    let account_indices = std::slice::from_raw_parts(instruction.accounts, instruction.accounts_count.into());
    let account_keys = std::slice::from_raw_parts(message.account_keys, message.account_keys_count as usize);

    // First account is typically the price account for Pyth updates
    if account_indices.len() > 0 {
        let account_index = account_indices[0] as usize;
        if account_index < account_keys.len() {
            return Some(account_keys[account_index].bytes);
        }
    }

    None
}

fn derive_price_id_from_account(price_account: &[u8; 32]) -> [u8; 32] {
    // For now, use the account key as the price ID
    // In a real implementation, this would involve parsing the price account data
    *price_account
}

pub fn calculate_price_confidence_score(price_data: &PriceData, current_time: i64) -> u8 {
    let age_seconds = current_time - price_data.publish_time;
    let confidence_ratio = if price_data.price == 0 {
        100.0
    } else {
        (price_data.conf as f64 / price_data.price.abs() as f64) * 100.0
    };

    // Score based on age and confidence
    let age_score = if age_seconds < 10 {
        100
    } else if age_seconds < 30 {
        80
    } else if age_seconds < 60 {
        50
    } else {
        20
    };

    let conf_score = if confidence_ratio < 0.1 {
        100
    } else if confidence_ratio < 0.5 {
        80
    } else if confidence_ratio < 1.0 {
        60
    } else {
        30
    };

    ((age_score + conf_score) / 2).min(100) as u8
}