use checkle::simd::bytes_to_hex;
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};

/// Generate test data for benchmarking
fn generate_test_data(size: usize) -> Vec<u8> {
    (0..size)
        .map(|i| u8::try_from(i % 256).expect("modulo 256 always fits in u8"))
        .collect()
}

/// Benchmark the scalar hex conversion implementation
fn bench_scalar_hex_conversion(c: &mut Criterion) {
    let mut group = c.benchmark_group("hex_conversion_scalar");

    // Test common hash sizes
    for size in &[16, 32, 64, 128, 256, 512, 1024] {
        let data = generate_test_data(*size);

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &data, |b, data| {
            b.iter(|| {
                let result = bytes_to_hex(black_box(data));
                black_box(result);
            });
        });
    }

    group.finish();
}

/// Benchmark edge cases and special scenarios
fn bench_hex_edge_cases(c: &mut Criterion) {
    let mut group = c.benchmark_group("hex_conversion_edge_cases");

    // Empty input
    group.bench_function("empty", |b| {
        let data = vec![];
        b.iter(|| {
            let result = bytes_to_hex(black_box(&data));
            black_box(result);
        });
    });

    // Single byte
    group.bench_function("single_byte", |b| {
        let data = vec![0xFF];
        b.iter(|| {
            let result = bytes_to_hex(black_box(&data));
            black_box(result);
        });
    });

    // MD5 hash (16 bytes)
    group.bench_function("md5_hash", |b| {
        let data = vec![0xDE; 16];
        b.iter(|| {
            let result = bytes_to_hex(black_box(&data));
            black_box(result);
        });
    });

    // SHA256 hash (32 bytes)
    group.bench_function("sha256_hash", |b| {
        let data = vec![0xAB; 32];
        b.iter(|| {
            let result = bytes_to_hex(black_box(&data));
            black_box(result);
        });
    });

    // Large buffer (simulating Merkle tree node)
    group.bench_function("large_buffer_4k", |b| {
        let data = vec![0x55; 4096];
        b.iter(|| {
            let result = bytes_to_hex(black_box(&data));
            black_box(result);
        });
    });

    group.finish();
}

/// Benchmark realistic workloads
fn bench_realistic_workloads(c: &mut Criterion) {
    let mut group = c.benchmark_group("hex_conversion_workloads");

    // Simulate hashing 1000 MD5 hashes
    group.bench_function("batch_md5_1000", |b| {
        let hashes: Vec<Vec<u8>> = (0..1000)
            .map(|i| vec![u8::try_from(i % 256).expect("modulo 256 always fits in u8"); 16])
            .collect();

        b.iter(|| {
            for hash in &hashes {
                let result = bytes_to_hex(black_box(hash));
                black_box(result);
            }
        });
    });

    // Simulate hashing 1000 SHA256 hashes
    group.bench_function("batch_sha256_1000", |b| {
        let hashes: Vec<Vec<u8>> = (0..1000)
            .map(|i| vec![u8::try_from((i * 7) % 256).expect("modulo 256 always fits in u8"); 32])
            .collect();

        b.iter(|| {
            for hash in &hashes {
                let result = bytes_to_hex(black_box(hash));
                black_box(result);
            }
        });
    });

    // Mixed workload (alternating MD5 and SHA256)
    group.bench_function("mixed_workload_500_each", |b| {
        let mut hashes = Vec::new();
        for i in 0..500 {
            hashes.push(vec![
                u8::try_from(i % 256)
                    .expect("modulo 256 always fits in u8");
                16
            ]); // MD5
            hashes.push(vec![
                u8::try_from((i * 3) % 256)
                    .expect("modulo 256 always fits in u8");
                32
            ]); // SHA256
        }

        b.iter(|| {
            for hash in &hashes {
                let result = bytes_to_hex(black_box(hash));
                black_box(result);
            }
        });
    });

    group.finish();
}

#[cfg(feature = "simd")]
/// Benchmark SIMD vs scalar implementations
fn bench_simd_comparison(c: &mut Criterion) {
    use checkle::simd::{bytes_to_hex_scalar, bytes_to_hex_simd};

    let mut group = c.benchmark_group("hex_conversion_comparison");

    for size in &[16, 32, 64, 128, 256, 512, 1024] {
        let data = generate_test_data(*size);

        group.throughput(Throughput::Bytes(*size as u64));

        // Scalar implementation
        group.bench_with_input(BenchmarkId::new("scalar", size), &data, |b, data| {
            b.iter(|| {
                let result = bytes_to_hex_scalar(black_box(data));
                black_box(result);
            });
        });

        // SIMD implementation
        group.bench_with_input(BenchmarkId::new("simd", size), &data, |b, data| {
            b.iter(|| {
                let result = bytes_to_hex_simd(black_box(data));
                black_box(result);
            });
        });
    }

    group.finish();
}

// Define benchmark groups based on feature flags
#[cfg(not(feature = "simd"))]
criterion_group!(
    benches,
    bench_scalar_hex_conversion,
    bench_hex_edge_cases,
    bench_realistic_workloads
);

#[cfg(feature = "simd")]
criterion_group!(
    benches,
    bench_scalar_hex_conversion,
    bench_hex_edge_cases,
    bench_realistic_workloads,
    bench_simd_comparison
);

criterion_main!(benches);
