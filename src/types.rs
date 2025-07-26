use libc::c_char;
use serde::{Deserialize, Serialize};

#[repr(C)]
#[derive(Debug, Clone)]
pub struct TransactionBundle {
    pub transaction_count: u32,
    pub transactions: *mut Transaction,
    pub metadata: BundleMetadata,
    pub attestation: *mut Attestation,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct Transaction {
    pub signatures: *mut Signature,
    pub signature_count: u8,
    pub message: TransactionMessage,
    pub priority_fee: u64,
    pub compute_limit: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Signature {
    pub bytes: [u8; 64],
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct TransactionMessage {
    pub header: MessageHeader,
    pub account_keys: *mut Pubkey,
    pub account_keys_count: u8,
    pub recent_blockhash: [u8; 32],
    pub instructions: *mut CompiledInstruction,
    pub instructions_count: u8,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MessageHeader {
    pub num_required_signatures: u8,
    pub num_readonly_signed_accounts: u8,
    pub num_readonly_unsigned_accounts: u8,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Pubkey {
    pub bytes: [u8; 32],
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct CompiledInstruction {
    pub program_id_index: u8,
    pub accounts: *mut u8,
    pub accounts_count: u8,
    pub data: *mut u8,
    pub data_len: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BundleMetadata {
    pub slot: u64,
    pub timestamp: u64,
    pub leader_pubkey: [u8; 32],
    pub plugin_fees: u64,
    pub tip_amount: u64,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct Attestation {
    pub version: u32,
    pub node_id: [u8; 32],
    pub bundle_hash: [u8; 32],
    pub timestamp: u64,
    pub signature: [u8; 64],
    pub tee_report: *mut u8,
    pub tee_report_len: u32,
}

#[repr(C)]
pub struct PluginInterface {
    pub version: u32,
    pub capabilities: u32,
    pub name: *const c_char,
    
    // Lifecycle functions
    pub init: extern "C" fn(*const u8, usize) -> i32,
    pub shutdown: extern "C" fn() -> i32,
    
    // Processing functions
    pub process_bundle: extern "C" fn(*mut TransactionBundle) -> i32,
    pub get_fee_estimate: extern "C" fn(*const TransactionBundle) -> u64,
    
    // State management
    pub get_state: extern "C" fn(*mut u8, usize) -> i32,
    pub set_state: extern "C" fn(*const u8, usize) -> i32,
}

// Safety: PluginInterface is immutable and only contains function pointers
// The name field points to a static string
unsafe impl Sync for PluginInterface {}

// Plugin capabilities
pub const CAPABILITY_BUNDLE_PROCESSING: u32 = 0x01;
pub const CAPABILITY_TRANSACTION_INJECTION: u32 = 0x02;
pub const CAPABILITY_PRIORITY_ORDERING: u32 = 0x04;
pub const CAPABILITY_FEE_COLLECTION: u32 = 0x08;
pub const CAPABILITY_ORACLE_PROCESSING: u32 = 0x10;

// Error codes
pub const SUCCESS: i32 = 0;
pub const ERROR_NULL_POINTER: i32 = -1;
pub const ERROR_INVALID_BUNDLE: i32 = -2;
pub const ERROR_PROCESSING_FAILED: i32 = -3;
pub const ERROR_INSUFFICIENT_FEE: i32 = -4;
pub const ERROR_INVALID_STATE: i32 = -5;
pub const ERROR_ALLOCATION_FAILED: i32 = -6;

// Oracle error codes (V2) - unified namespace
pub const ERROR_ORACLE_STALE_PRICE: i32 = -100;
pub const ERROR_ORACLE_INVALID_ACCOUNT: i32 = -101;
pub const ERROR_ORACLE_NETWORK_FAILURE: i32 = -102;
pub const ERROR_ORACLE_PARSE_FAILURE: i32 = -103;
pub const ERROR_ORACLE_CACHE_MISS: i32 = -104;

// Internal state for metrics and configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginState {
    pub bundles_processed: u64,
    pub total_fees_collected: u64,
    pub average_processing_time_us: u64,
    pub last_error: Option<String>,
    pub config: PluginConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    pub min_fee_lamports: u64,
    pub fee_percentage: f32,
    pub max_bundle_size: u32,
    pub enable_metrics: bool,
    pub enable_debug_logging: bool,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            min_fee_lamports: 5000,  // 0.000005 SOL minimum
            fee_percentage: 0.001,    // 0.1% fee
            max_bundle_size: 100,     // Max 100 transactions per bundle
            enable_metrics: true,
            enable_debug_logging: false,
        }
    }
}

impl Default for PluginState {
    fn default() -> Self {
        Self {
            bundles_processed: 0,
            total_fees_collected: 0,
            average_processing_time_us: 0,
            last_error: None,
            config: PluginConfig::default(),
        }
    }
}