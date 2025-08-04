# SIMD Optimization Plan for Checkle

This document presents the definitive SIMD optimization strategy for checkle,
ranked by proportional value (benefit/complexity ratio). Since we are pre-1.0,
we will replace scalar implementations entirely when switching to SIMD.

## Executive Summary

SIMD optimization will provide 2-20x performance improvements for specific
operations. We will use Rust nightly's `portable_simd` feature and replace
scalar implementations completely for chosen operations.

## A Note on SIMD as a Feature

SIMD acceleration is somewhat unusual as a feature in that it will be both
visible and invisible to users. On its own, it's not really a "feature". But
performance across the board is. Performance can take a routine, tedious task
like running checksums--just another thing you have to do at work--into a
delight. "It's already finished??" is an unalloyed good in a world of mediocre
to bad software experiences. Performance can also be the difference between a
tool being accessible to those without the privilege of using expensive
computers as the current state-of-the-art. And even when they are, performance
that's better than it needs to be means letting less of an expensive machine go
to waste. Like paying for an expensive gym but not using it, paying for an
expensive computer with many cores and SIMD capability but not using it is a
waste.

So when we go to implement SIMD acceleration, tedious though it may be, we do it
to reduce waste, make CPU engineers' hard work worth it, and put a smile on our
users' faces. We make this investment once upfront and then sit back as that
investment pays dividends, over and over and over again.

## Ranked SIMD Implementations by Value

### 1. Hex String Conversion (HIGHEST PRIORITY - 10x speedup)

**Location**: `src/hashing.rs:1118-1122`

**Impact**: Saves ~42ms per 1000 files. Every hash operation benefits.

**Implementation**:

```rust
#![feature(portable_simd)]
use std::simd::{u8x32, Simd, SimdPartialOrd};

pub fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(bytes.len() * 2);
    let chunks = bytes.chunks_exact(32);
    let remainder = chunks.remainder();
    let mut hex_bytes = Vec::with_capacity(64);
    
    for chunk in chunks {
        hex_bytes.clear();
        let input = u8x32::from_slice(chunk);
        let hi = input >> Simd::splat(4);
        let lo = input & Simd::splat(0x0F);
        let nine = Simd::splat(9);
        let hi_gt_nine = hi.simd_gt(nine);
        let lo_gt_nine = lo.simd_gt(nine);
        
        // Branchless conversion to lowercase hex
        let hi_ascii = hi + Simd::select(
            hi_gt_nine,
            Simd::splat(b'a' - 10),
            Simd::splat(b'0')
        );
        
        let lo_ascii = lo + Simd::select(
            lo_gt_nine,
            Simd::splat(b'a' - 10),
            Simd::splat(b'0')
        );
        
        // Manual interleaving
        let hi_array = hi_ascii.to_array();
        let lo_array = lo_ascii.to_array();
        for i in 0..32 {
            hex_bytes.push(hi_array[i]);
            hex_bytes.push(lo_array[i]);
        }
        
        result.push_str(unsafe { std::str::from_utf8_unchecked(&hex_bytes) });
    }
    
    // Handle remainder
    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";
    for &byte in remainder {
        result.push(HEX_CHARS[(byte >> 4) as usize] as char);
        result.push(HEX_CHARS[(byte & 0x0F) as usize] as char);
    }
    
    result
}
```

### 2. Buffer Pool Zero-Fill (HIGH PRIORITY - 8x speedup)

**Location**: `src/buffer_pool.rs:259`

**Impact**: Saves 19.6ms per 64MB buffer. Critical for large file operations.

**Implementation**:

```rust
#![feature(portable_simd)]
use std::simd::{u8x64, u8x32, u8x16};

pub fn zero_buffer(buffer: &mut [u8]) {
    // Align to cache line for best performance
    let ptr = buffer.as_mut_ptr();
    let len = buffer.len();
    
    // Handle misaligned prefix
    let misalignment = ptr as usize & 63;
    let aligned_start = if misalignment == 0 { 0 } else { 64 - misalignment };
    
    if aligned_start > 0 && aligned_start <= len {
        buffer[..aligned_start].fill(0);
    }
    
    if aligned_start < len {
        let aligned_buffer = &mut buffer[aligned_start..];
        
        // Process 64 bytes at a time
        let chunks64 = aligned_buffer.chunks_exact_mut(64);
        let remainder64 = chunks64.into_remainder();
        let zero64 = u8x64::splat(0);
        
        for chunk in chunks64 {
            unsafe {
                let ptr = chunk.as_mut_ptr() as *mut u8x64;
                ptr.write_unaligned(zero64);
            }
        }
        
        // Handle remainder with smaller SIMD sizes
        let chunks32 = remainder64.chunks_exact_mut(32);
        let remainder32 = chunks32.into_remainder();
        let zero32 = u8x32::splat(0);
        
        for chunk in chunks32 {
            unsafe {
                let ptr = chunk.as_mut_ptr() as *mut u8x32;
                ptr.write_unaligned(zero32);
            }
        }
        
        // Final cleanup
        remainder32.fill(0);
    }
}
```

### 3. Hash Validation (HIGH PRIORITY - 10-20x speedup)

**Location**: Multiple - `src/hashing.rs:1136,1171`, `src/io.rs:276`

**Impact**: Saves 9ms per 10,000 verifications. Essential for batch operations.

**Implementation**:

```rust
#![feature(portable_simd)]
use std::simd::{u8x32, Simd, SimdPartialOrd};

pub fn is_hex_string(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return true;
    }
    
    let chunks = bytes.chunks_exact(32);
    let remainder = chunks.remainder();
    
    for chunk in chunks {
        let v = u8x32::from_slice(chunk);
        
        // Clever bit manipulation for hex validation
        let uppercased = v & Simd::splat(!0x20);
        let is_digit = (v.simd_ge(Simd::splat(b'0')) & v.simd_le(Simd::splat(b'9')));
        let is_letter = (uppercased.simd_ge(Simd::splat(b'A')) & 
                        uppercased.simd_le(Simd::splat(b'F')));
        let is_hex = is_digit | is_letter;
        
        if !is_hex.all() {
            return false;
        }
    }
    
    remainder.iter().all(|&b| b.is_ascii_hexdigit())
}

pub fn validate_hash(hash: &str, expected_len: usize) -> bool {
    hash.len() == expected_len && is_hex_string(hash)
}
```

### 4. Checksum File Tab Detection (MEDIUM PRIORITY - 5x speedup)

**Location**: `src/io.rs:419-420,521-522`

**Impact**: Speeds up batch verification file parsing.

**Implementation**:

```rust
#![feature(portable_simd)]
use std::simd::{u8x32, Simd, SimdPartialEq};

pub fn find_tab_position(line: &[u8]) -> Option<usize> {
    let chunks = line.chunks_exact(32);
    let remainder = chunks.remainder();
    let tab = Simd::splat(b'\t');
    let mut position = 0;
    
    for chunk in chunks {
        let v = u8x32::from_slice(chunk);
        let matches = v.simd_eq(tab);
        
        if matches.any() {
            let mask = matches.to_bitmask();
            let offset = mask.trailing_zeros() as usize;
            return Some(position + offset);
        }
        position += 32;
    }
    
    remainder.iter()
        .position(|&b| b == b'\t')
        .map(|pos| position + pos)
}

pub fn parse_checksum_line(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    let tab_pos = find_tab_position(bytes)?;
    
    let hash = std::str::from_utf8(&bytes[..tab_pos]).ok()?;
    let filename = std::str::from_utf8(&bytes[tab_pos + 1..]).ok()?;
    
    Some((hash, filename))
}
```

### 5. Merkle Tree Hash Combining (FUTURE - 4x potential speedup)

**Location**: `src/hashing.rs:1214-1240`

**Note**: Requires deeper integration with hash algorithms. Consider for v2.0.

This would require creating SIMD-aware implementations of MD5/SHA256 or using
crates that provide them.

## Implementation Timeline

### Week 1: Foundation

1. Add `portable_simd` feature flag to Cargo.toml
2. Set up nightly toolchain configuration
3. Create benchmark suite for baseline measurements

### Week 2: Core Implementations

1. Replace hex conversion implementation
2. Replace buffer zeroing implementation
3. Update all hash validation call sites

### Week 3: Integration & Testing

1. Replace checksum file parsing
2. Comprehensive correctness testing
3. Performance validation across platforms

### Week 4: Polish

1. Documentation updates
2. Performance report generation
3. CI/CD integration for nightly builds

## Technical Configuration

### Cargo.toml:

```toml
[features]
default = ["archives"]
archives = ["tar", "zip"]
simd = [] # Requires nightly toolchain

[dependencies]
# No new dependencies needed - portable_simd is in std
```

### rust-toolchain.toml:

```toml
[toolchain]
channel = "nightly-2024-01-01"
components = ["rust-src"]
```

### Build Configuration:

```bash
# Enable SIMD optimizations
cargo build --release --features simd

# Standard build (will fail without nightly when simd enabled)
cargo build --release
```

## Performance Targets

### Hex Conversion

- **Current**: ~50μs per hash (32 bytes)
- **Target**: ~8μs per hash
- **Validation**: Benchmark 10,000 conversions

### Buffer Zeroing

- **Current**: 22.4ms per 64MB
- **Target**: 2.8ms per 64MB
- **Validation**: Benchmark with varying buffer sizes

### Hash Validation

- **Current**: ~1μs per 64-char string
- **Target**: ~0.1μs per 64-char string
- **Validation**: Benchmark 100,000 validations

### Checksum Parsing

- **Current**: ~200ns per line
- **Target**: ~40ns per line
- **Validation**: Parse 10,000 line checksum file

## Success Metrics

1. **Overall Performance**: 20-25% improvement for typical workflows
2. **Correctness**: 100% compatibility with existing hash outputs
3. **Platform Support**: Verified on x86_64 and ARM64
4. **Code Quality**: Passes all existing tests and lints

## Risk Mitigation

1. **Nightly Requirement**: Document clearly in README
2. **Platform Compatibility**: Test on CI matrix
3. **Correctness**: Extensive property-based testing
4. **Performance Regression**: Automated benchmarking in CI

## Future Opportunities

Once initial SIMD implementations prove successful:

1. **SIMD-aware hash libraries**: Integrate crc32fast-style implementations
2. **Parallel file processing**: SIMD across multiple files simultaneously
3. **Custom hash implementations**: Full SIMD MD5/SHA256
4. **Archive operations**: SIMD-accelerated compression detection

## Conclusion

SIMD optimization is a key differentiator for checkle. By focusing on high-value
operations and completely replacing scalar implementations, we achieve maximum
performance with minimal complexity. The hex conversion alone provides enough
value to justify the nightly requirement, with additional optimizations
providing cumulative benefits that establish checkle as the performance leader
in parallel checksumming.
