#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::missing_panics_doc,
    clippy::unit_arg,
    clippy::semicolon_if_nothing_returned,
    clippy::uninlined_format_args,
    clippy::format_push_string,
    clippy::manual_div_ceil
)]

use checkle::{
    constants::CHUNK_SIZE,
    hashing::{Hasher, HashingAlgo},
    io::{FileHashPair, FilesToCheck, collect_files},
};
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use std::{fs, path::PathBuf};
use tempfile::{NamedTempFile, TempDir};

fn benchmark_hashing_algorithms(c: &mut Criterion) {
    let mut group = c.benchmark_group("hashing_algorithms");

    // Test different file sizes
    let file_sizes = vec![
        ("1KB", 1024),
        ("10KB", 10 * 1024),
        ("100KB", 100 * 1024),
        ("1MB", CHUNK_SIZE),
        ("10MB", 10 * CHUNK_SIZE),
    ];

    for (size_name, size_bytes) in file_sizes {
        // Create test data
        let test_data = vec![0x42u8; size_bytes];

        // Benchmark MD5
        group.throughput(Throughput::Bytes(size_bytes as u64));
        group.bench_with_input(BenchmarkId::new("MD5", size_name), &test_data, |b, data| {
            b.iter(|| {
                let temp_file = NamedTempFile::new().expect("Failed to create temp file");
                fs::write(temp_file.path(), data).expect("Failed to write test data");

                let hasher = Hasher::new_md5(temp_file.path());
                let result = hasher.find_root_hash();
                black_box(result.expect("Hash should succeed"))
            });
        });

        // Benchmark SHA256
        group.bench_with_input(
            BenchmarkId::new("SHA256", size_name),
            &test_data,
            |b, data| {
                b.iter(|| {
                    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
                    fs::write(temp_file.path(), data).expect("Failed to write test data");

                    let hasher = Hasher::new_sha2(temp_file.path());
                    let result = hasher.find_root_hash();
                    black_box(result.expect("Hash should succeed"))
                });
            },
        );
    }

    group.finish();
}

fn benchmark_merkle_tree_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("merkle_tree_performance");

    // Test files that will create different numbers of chunks
    let chunk_counts = vec![
        ("1_chunk", CHUNK_SIZE),
        ("2_chunks", 2 * CHUNK_SIZE + 1),
        ("4_chunks", 4 * CHUNK_SIZE + 1),
        ("8_chunks", 8 * CHUNK_SIZE + 1),
    ];

    for (name, size_bytes) in chunk_counts {
        let test_data = vec![0xAAu8; size_bytes];

        group.throughput(Throughput::Bytes(size_bytes as u64));
        group.bench_with_input(
            BenchmarkId::new("merkle_tree", name),
            &test_data,
            |b, data| {
                b.iter(|| {
                    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
                    fs::write(temp_file.path(), data).expect("Failed to write test data");

                    let hasher = Hasher::new_md5(temp_file.path());
                    let result = hasher.find_root_hash();
                    black_box(result.expect("Hash should succeed"))
                });
            },
        );
    }

    group.finish();
}

fn benchmark_checksum_verification(c: &mut Criterion) {
    let mut group = c.benchmark_group("checksum_verification");

    let file_sizes = vec![
        ("1MB", CHUNK_SIZE),
        ("5MB", 5 * CHUNK_SIZE),
        ("10MB", 10 * CHUNK_SIZE),
    ];

    for (size_name, size_bytes) in file_sizes {
        let test_data = vec![0x33u8; size_bytes];

        group.throughput(Throughput::Bytes(size_bytes as u64));
        group.bench_with_input(
            BenchmarkId::new("verify_checksum", size_name),
            &test_data,
            |b, data| {
                // Pre-compute the hash
                let temp_file = NamedTempFile::new().expect("Failed to create temp file");
                fs::write(temp_file.path(), data).expect("Failed to write test data");

                let hasher = Hasher::new_md5(temp_file.path());
                let expected_hash = hasher.find_root_hash().expect("Hash should succeed");

                b.iter(|| {
                    let verifier = Hasher::new_md5(temp_file.path());
                    let result = verifier.checksum(&expected_hash);
                    black_box(result.expect("Checksum should succeed"))
                });
            },
        );
    }

    group.finish();
}

fn benchmark_file_collection(c: &mut Criterion) {
    let mut group = c.benchmark_group("file_collection");

    let file_counts = vec![10, 50, 100, 500];

    for count in file_counts {
        group.bench_with_input(
            BenchmarkId::new("collect_many_files", count),
            &count,
            |b, &file_count| {
                b.iter_batched(
                    || {
                        // Setup: create temporary directory with many files
                        let temp_dir = TempDir::new().expect("Failed to create temp dir");
                        let original_dir =
                            std::env::current_dir().expect("Failed to get current dir");

                        for i in 0..file_count {
                            let file_path = temp_dir.path().join(format!("test_file_{:04}.txt", i));
                            fs::write(&file_path, format!("content {}", i))
                                .expect("Failed to write file");
                        }

                        (temp_dir, original_dir)
                    },
                    |(temp_dir, original_dir)| {
                        std::env::set_current_dir(temp_dir.path()).expect("Failed to change dir");
                        let filter_config = checkle::io::FileFilterConfig::new();
                        let result = collect_files(&PathBuf::from("*"), false, &filter_config);
                        std::env::set_current_dir(original_dir).expect("Failed to restore dir");
                        black_box(result.expect("File collection should succeed"))
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn benchmark_checksum_file_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("checksum_file_parsing");

    let entry_counts = vec![10, 100, 1000];

    for count in entry_counts {
        group.bench_with_input(
            BenchmarkId::new("parse_checksum_file", count),
            &count,
            |b, &entry_count| {
                b.iter_batched(
                    || {
                        // Setup: create checksum file and temporary files
                        let temp_files: Vec<_> = (0..entry_count)
                            .map(|_| NamedTempFile::new().expect("Failed to create temp file"))
                            .collect();

                        let checksum_file =
                            NamedTempFile::new().expect("Failed to create checksum file");
                        let mut checksum_content = String::new();

                        for (i, temp_file) in temp_files.iter().enumerate() {
                            checksum_content.push_str(&format!(
                                "hash{:08x}\t{}\n",
                                i,
                                temp_file.path().display()
                            ));
                        }

                        fs::write(checksum_file.path(), checksum_content)
                            .expect("Failed to write checksum file");

                        (checksum_file, temp_files)
                    },
                    |(checksum_file, _temp_files)| {
                        let result = FilesToCheck::new_from_txt(checksum_file.path());
                        black_box(result.expect("Checksum file parsing should succeed"))
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn benchmark_batch_checksums(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_checksums");

    let batch_sizes = vec![5, 10, 25];

    for batch_size in batch_sizes {
        group.bench_with_input(
            BenchmarkId::new("verify_batch", batch_size),
            &batch_size,
            |b, &size| {
                b.iter_batched(
                    || {
                        // Setup: create files and their hashes
                        let temp_files: Vec<_> = (0..size)
                            .map(|i| {
                                let temp_file =
                                    NamedTempFile::new().expect("Failed to create temp file");
                                let content = format!("test content {}", i);
                                fs::write(temp_file.path(), content)
                                    .expect("Failed to write content");
                                temp_file
                            })
                            .collect();

                        // Generate correct hashes
                        let pairs: Vec<_> = temp_files
                            .iter()
                            .map(|temp_file| {
                                let hasher = Hasher::new_md5(temp_file.path());
                                let hash = hasher.find_root_hash().expect("Hash should succeed");
                                FileHashPair::new(temp_file.path().to_path_buf(), hash)
                            })
                            .collect();

                        let files_to_check = FilesToCheck::from_vec(pairs);

                        (files_to_check, temp_files)
                    },
                    |(files_to_check, _temp_files)| {
                        let result = files_to_check.checksum_all(&HashingAlgo::Md5);
                        black_box(result.expect("Batch checksum should succeed"))
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn benchmark_memory_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_efficiency");

    // Test memory usage with different chunk counts
    let chunk_multipliers = vec![1, 5, 10, 20];

    for multiplier in chunk_multipliers {
        let size_bytes = multiplier * CHUNK_SIZE;
        let test_data = vec![0x55u8; size_bytes];

        group.throughput(Throughput::Bytes(size_bytes as u64));
        group.bench_with_input(
            BenchmarkId::new("memory_usage", format!("{}x_chunk", multiplier)),
            &test_data,
            |b, data| {
                b.iter(|| {
                    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
                    fs::write(temp_file.path(), data).expect("Failed to write test data");

                    let hasher = Hasher::new_sha2(temp_file.path()); // Use SHA2 as it's more intensive
                    let result = hasher.find_root_hash();
                    black_box(result.expect("Hash should succeed"))
                });
            },
        );
    }

    group.finish();
}

fn benchmark_genomics_workload(c: &mut Criterion) {
    let mut group = c.benchmark_group("genomics_workload");

    // Simulate different genomics file sizes
    let genomics_sizes = vec![
        ("small_read", 1024),                  // Small sequencing read file
        ("medium_assembly", 10 * 1024 * 1024), // Medium assembly file
        ("large_genome", 50 * 1024 * 1024),    // Large genome file (simulated)
    ];

    for (name, size_bytes) in genomics_sizes {
        // Create genomics-like data (ATCG patterns)
        let base_pattern = b"ATCGATCGATCGATCGNNNNTAGCTAGCTAGCTGCAATTGCATGCATGCATGC";
        let mut genomics_data = Vec::new();
        let pattern_repeats = (size_bytes + base_pattern.len() - 1) / base_pattern.len();

        for _ in 0..pattern_repeats {
            genomics_data.extend_from_slice(base_pattern);
        }
        genomics_data.truncate(size_bytes);

        group.throughput(Throughput::Bytes(size_bytes as u64));
        group.bench_with_input(
            BenchmarkId::new("genomics_checksum", name),
            &genomics_data,
            |b, data| {
                b.iter(|| {
                    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
                    fs::write(temp_file.path(), data).expect("Failed to write genomics data");

                    let hasher = Hasher::new_sha2(temp_file.path()); // Use SHA2 for genomics data
                    let result = hasher.find_root_hash();
                    black_box(result.expect("Genomics hash should succeed"))
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_hashing_algorithms,
    benchmark_merkle_tree_performance,
    benchmark_checksum_verification,
    benchmark_file_collection,
    benchmark_checksum_file_parsing,
    benchmark_batch_checksums,
    benchmark_memory_efficiency,
    benchmark_genomics_workload
);

criterion_main!(benches);
