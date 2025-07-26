# Relay BAM Plugin

A V1 Bundle Forwarder Plugin for Solana's Block Assembly Marketplace (BAM). This plugin provides minimal transaction bundle processing with latency optimization and fee collection.

## Overview

The Relay plugin is a Rust dynamic library that integrates with BAM Nodes to process transaction bundles. It implements the BAM Plugin Interface specification for:

- Transaction bundle validation
- Priority-based transaction ordering
- Fee calculation and collection
- Performance metrics tracking

## Architecture

### Core Components

- **FFI Interface** (`lib.rs`): C-compatible exports for BAM Node integration
- **Types** (`types.rs`): BAM protocol type definitions
- **Processing** (`processing.rs`): Bundle processing and optimization logic
- **Validation** (`validation.rs`): Bundle structure and content validation
- **Fees** (`fees.rs`): Dynamic fee calculation
- **Metrics** (`metrics.rs`): Performance tracking

### Plugin Capabilities

- `CAPABILITY_BUNDLE_PROCESSING` (0x01): Process and optimize transaction bundles
- `CAPABILITY_FEE_COLLECTION` (0x08): Collect plugin fees from bundles

## Building

```bash
# Build the plugin
cargo build --release

# Run tests
cargo test

# Run benchmarks
cargo bench

# Build with optimizations
cargo build --release --target x86_64-unknown-linux-gnu
```

The compiled plugin will be at `target/release/librelay_bam_plugin.so`

## API Reference

### Plugin Interface

```c
typedef struct {
    uint32_t version;
    uint32_t capabilities;
    const char* name;
    
    // Lifecycle
    int32_t (*init)(const uint8_t* config, size_t len);
    int32_t (*shutdown)(void);
    
    // Processing
    int32_t (*process_bundle)(TransactionBundle* bundle);
    uint64_t (*get_fee_estimate)(const TransactionBundle* bundle);
    
    // State
    int32_t (*get_state)(uint8_t* buffer, size_t len);
    int32_t (*set_state)(const uint8_t* data, size_t len);
} PluginInterface;
```

### Error Codes

- `SUCCESS` (0): Operation completed successfully
- `ERROR_NULL_POINTER` (-1): Null pointer provided
- `ERROR_INVALID_BUNDLE` (-2): Bundle validation failed
- `ERROR_PROCESSING_FAILED` (-3): Processing error
- `ERROR_INSUFFICIENT_FEE` (-4): Fee too low
- `ERROR_INVALID_STATE` (-5): Invalid plugin state

### Configuration

```json
{
    "min_fee_lamports": 5000,
    "fee_percentage": 0.001,
    "max_bundle_size": 100,
    "enable_metrics": true,
    "enable_debug_logging": false
}
```

### Bundle Processing Flow

1. **Validation**: Verify bundle structure, attestation, and transactions
2. **Fee Check**: Ensure sufficient plugin fees are included
3. **Optimization**: Sort transactions by priority fee
4. **Metrics**: Track processing time and success rate
5. **Return**: Success or error code

## Integration Example

```rust
// Load plugin
let plugin = dlopen("librelay_bam_plugin.so");
let interface = plugin.symbol::<PluginInterface>("PLUGIN_INTERFACE");

// Initialize
let config = r#"{"min_fee_lamports": 10000}"#;
interface.init(config.as_ptr(), config.len());

// Process bundle
let mut bundle = create_bundle();
let result = interface.process_bundle(&mut bundle);

if result == 0 {
    println!("Bundle processed successfully");
}

// Shutdown
interface.shutdown();
```

## Performance

Target metrics:
- **Latency**: < 1ms per bundle
- **Throughput**: > 1000 bundles/second
- **Memory**: < 10MB overhead
- **CPU**: < 5% single core

## Testing

```bash
# Unit tests
cargo test

# Integration tests
cargo test --test integration_tests

# Example usage
cargo run --example test_plugin

# Benchmarks
cargo bench
```

## Security Considerations

- All FFI boundaries validate pointers
- No panics in external functions
- Bounded memory allocations
- Input size limits enforced
- No external dependencies in hot path

## Future Enhancements

- [ ] Advanced transaction deduplication
- [ ] MEV extraction optimization
- [ ] Multi-threaded processing
- [ ] Anchor-based fee collection
- [ ] TypeScript client library

## License

MIT