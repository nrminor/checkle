# Tests

Unit and integration tests for checkle.

## Running Tests

```bash
# Run all tests
cargo test

# Run tests with output displayed
cargo test -- --nocapture

# Run a specific test
cargo test test_md5_hasher_normal_operation
```

## Hash Verification

```bash
# Run hash verification against standard utilities
./tests/verify_hashes.sh

# Or use justfile
just verify-hashes
```

## Property-Based Tests

Tests use proptest to verify:
- Hash determinism
- Hash length invariants
- Input validation properties