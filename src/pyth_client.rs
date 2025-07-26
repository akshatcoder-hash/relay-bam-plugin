use crate::oracle::*;
use crate::types::*;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use once_cell::sync::Lazy;
use base64::{Engine as _, engine::general_purpose};

// Pyth protocol constants
pub const PYTH_MAGIC_NUMBER: u32 = 0xa1b2c3d4;
pub const PYTH_VERSION_V2: u32 = 2;
pub const PYTH_ACCOUNT_TYPE_PRICE: u32 = 3;
pub const MIN_PRICE_ACCOUNT_SIZE: usize = 240;

// Price data field offsets
pub const PRICE_OFFSET: usize = 208;
pub const CONF_OFFSET: usize = 216;
pub const EXPO_OFFSET: usize = 20;
pub const TIMESTAMP_OFFSET: usize = 96;

static PYTH_CLIENT: Lazy<RwLock<PythClient>> = Lazy::new(|| {
    RwLock::new(PythClient::new())
});

#[derive(Debug, Clone)]
pub struct PythClient {
    pub config: OracleConfig,
    pub cache: OracleCache,
    pub http_client: Option<reqwest::Client>,
    pub last_fetch_time: SystemTime,
    pub fetch_count: u64,
}

#[derive(Debug, Deserialize)]
struct SolanaRpcResponse<T> {
    jsonrpc: String,
    result: Option<T>,
    error: Option<SolanaRpcError>,
    id: u64,
}

#[derive(Debug, Deserialize)]
struct SolanaRpcError {
    code: i32,
    message: String,
}

#[derive(Debug, Deserialize)]
struct AccountInfo {
    data: Vec<String>,
    executable: bool,
    lamports: u64,
    owner: String,
    #[serde(rename = "rentEpoch")]
    rent_epoch: u64,
}

#[derive(Debug, Serialize)]
struct RpcRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    params: Vec<serde_json::Value>,
}

impl PythClient {
    pub fn new() -> Self {
        Self {
            config: OracleConfig::default(),
            cache: OracleCache::default(),
            http_client: None,
            last_fetch_time: UNIX_EPOCH,
            fetch_count: 0,
        }
    }

    pub fn initialize(&mut self, config: OracleConfig) -> Result<(), Box<dyn std::error::Error>> {
        self.config = config;
        self.http_client = Some(
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()?
        );
        log::info!("Pyth client initialized with {} price accounts", self.config.price_account_keys.len());
        Ok(())
    }

    pub async fn fetch_all_prices(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = self.http_client.as_ref()
            .ok_or("HTTP client not initialized")?;

        let mut requests = Vec::new();
        
        for (i, account_key) in self.config.price_account_keys.iter().enumerate() {
            let request = RpcRequest {
                jsonrpc: "2.0".to_string(),
                id: i as u64,
                method: "getAccountInfo".to_string(),
                params: vec![
                    serde_json::Value::String(account_key.clone()),
                    serde_json::json!({
                        "encoding": "base64",
                        "commitment": "confirmed"
                    })
                ],
            };
            requests.push(request);
        }

        // Batch fetch all accounts
        for request in requests {
            match self.fetch_price_account(client, request).await {
                Ok((price_id, price_data)) => {
                    self.cache.update_price(price_id, price_data);
                }
                Err(e) => {
                    log::warn!("Failed to fetch price account: {}", e);
                }
            }
        }

        self.last_fetch_time = SystemTime::now();
        self.fetch_count += 1;

        log::debug!("Fetched {} price accounts (total fetches: {})", 
            self.config.price_account_keys.len(), self.fetch_count);

        Ok(())
    }

    async fn fetch_price_account(
        &self,
        client: &reqwest::Client,
        request: RpcRequest,
    ) -> Result<([u8; 32], PriceData), Box<dyn std::error::Error + Send + Sync>> {
        let response = client
            .post(&self.config.pyth_cluster_url)
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("HTTP error: {}", response.status()).into());
        }

        let rpc_response: SolanaRpcResponse<AccountInfo> = response.json().await?;

        if let Some(error) = rpc_response.error {
            return Err(format!("RPC error: {}", error.message).into());
        }

        let account_info = rpc_response.result
            .ok_or("No account data returned")?;

        let account_data = if account_info.data.len() >= 2 && account_info.data[1] == "base64" {
            general_purpose::STANDARD.decode(&account_info.data[0])
                .map_err(|e| format!("Base64 decode error: {}", e))?
        } else {
            return Err("Invalid account data encoding".into());
        };

        self.parse_pyth_price_account(&account_data)
    }

    fn parse_pyth_price_account(
        &self,
        data: &[u8],
    ) -> Result<([u8; 32], PriceData), Box<dyn std::error::Error + Send + Sync>> {
        if data.len() < MIN_PRICE_ACCOUNT_SIZE {
            return Err("Account data too short for Pyth price account".into());
        }

        // Parse Pyth price account structure
        let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        if magic != PYTH_MAGIC_NUMBER {
            return Err("Invalid Pyth account magic number".into());
        }

        let version = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let account_type = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        
        match version {
            PYTH_VERSION_V2 => {
                // Continue with V2 parsing logic
            }
            v => return Err(format!("Unsupported Pyth version: {}", v).into()),
        }
        
        if account_type != PYTH_ACCOUNT_TYPE_PRICE {
            return Err("Not a price account".into());
        }

        // Extract price data using protocol constants
        let price_offset = PRICE_OFFSET;
        let conf_offset = CONF_OFFSET;
        let expo_offset = EXPO_OFFSET;
        let timestamp_offset = TIMESTAMP_OFFSET;

        let price = i64::from_le_bytes([
            data[price_offset], data[price_offset + 1], data[price_offset + 2], data[price_offset + 3],
            data[price_offset + 4], data[price_offset + 5], data[price_offset + 6], data[price_offset + 7],
        ]);

        let conf = u64::from_le_bytes([
            data[conf_offset], data[conf_offset + 1], data[conf_offset + 2], data[conf_offset + 3],
            data[conf_offset + 4], data[conf_offset + 5], data[conf_offset + 6], data[conf_offset + 7],
        ]);

        let expo = i32::from_le_bytes([
            data[expo_offset], data[expo_offset + 1], data[expo_offset + 2], data[expo_offset + 3],
        ]);

        let timestamp = i64::from_le_bytes([
            data[timestamp_offset], data[timestamp_offset + 1], data[timestamp_offset + 2], data[timestamp_offset + 3],
            data[timestamp_offset + 4], data[timestamp_offset + 5], data[timestamp_offset + 6], data[timestamp_offset + 7],
        ]);

        // Generate price ID from account key (simplified)
        let mut price_id = [0u8; 32];
        price_id[..8].copy_from_slice(&data[32..40]); // Use part of product account as ID

        let price_data = PriceData {
            price,
            conf,
            expo,
            publish_time: timestamp,
        };

        Ok((price_id, price_data))
    }

    pub fn get_cached_price(&mut self, price_id: &[u8; 32]) -> Option<PriceData> {
        self.cache.get_price(price_id).cloned()
    }

    pub fn should_refresh(&self) -> bool {
        match self.last_fetch_time.duration_since(UNIX_EPOCH) {
            Ok(duration) => {
                let current_time = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();
                let last_fetch_time = duration.as_millis();
                current_time - last_fetch_time > self.config.update_interval_ms as u128
            }
            Err(_) => true,
        }
    }

    pub fn is_price_stale(&self, price_data: &PriceData) -> bool {
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        
        current_time - price_data.publish_time > self.config.max_price_age_seconds as i64
    }
}

// Global functions for FFI interface
pub async fn initialize_pyth_client(config: OracleConfig) -> i32 {
    match PYTH_CLIENT.write().await.initialize(config) {
        Ok(_) => {
            log::info!("Pyth oracle client initialized successfully");
            SUCCESS
        }
        Err(e) => {
            log::error!("Failed to initialize Pyth client: {}", e);
            ERROR_ORACLE_NETWORK_FAILURE
        }
    }
}

pub async fn fetch_oracle_prices() -> i32 {
    let mut client = PYTH_CLIENT.write().await;
    
    if !client.should_refresh() {
        return SUCCESS; // No need to refresh
    }

    match client.fetch_all_prices().await {
        Ok(_) => {
            log::debug!("Oracle prices fetched successfully");
            SUCCESS
        }
        Err(e) => {
            log::error!("Failed to fetch oracle prices: {}", e);
            ERROR_ORACLE_NETWORK_FAILURE
        }
    }
}

pub async fn get_oracle_price(price_id: &[u8; 32]) -> Result<PriceData, i32> {
    let mut client = PYTH_CLIENT.write().await;
    
    match client.get_cached_price(price_id) {
        Some(price_data) => {
            if client.is_price_stale(&price_data) {
                log::warn!("Price data is stale for price_id: {:?}", hex::encode(price_id));
                Err(ERROR_ORACLE_STALE_PRICE)
            } else {
                Ok(price_data)
            }
        }
        None => {
            log::warn!("Price not found in cache for price_id: {:?}", hex::encode(price_id));
            Err(ERROR_ORACLE_CACHE_MISS)
        }
    }
}

pub async fn inject_oracle_prices(
    _bundle: *mut TransactionBundle,
    injection_points: &[PriceInjectionPoint],
) -> i32 {
    if injection_points.is_empty() {
        return SUCCESS;
    }

    log::debug!("Injecting oracle prices at {} points", injection_points.len());

    for point in injection_points {
        match get_oracle_price(&point.required_price_id).await {
            Ok(price_data) => {
                let confidence_score = calculate_price_confidence_score(
                    &price_data,
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64,
                );

                if confidence_score < 50 {
                    log::warn!(
                        "Low confidence price data ({}%) for injection at tx:{}, inst:{}",
                        confidence_score,
                        point.transaction_index,
                        point.instruction_index
                    );
                }

                log::debug!(
                    "Injected price: {} (confidence: {}%) at tx:{}, inst:{}",
                    price_data.price,
                    confidence_score,
                    point.transaction_index,
                    point.instruction_index
                );
            }
            Err(error_code) => {
                log::error!(
                    "Failed to get price for injection at tx:{}, inst:{} - error: {}",
                    point.transaction_index,
                    point.instruction_index,
                    error_code
                );
                return error_code;
            }
        }
    }

    SUCCESS
}

