#![allow(clippy::print_stderr, clippy::print_stdout)]

use checkle::hashing::Hasher;
use std::fs;
use tempfile::NamedTempFile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a test file
    let temp_file = NamedTempFile::new()?;
    let test_data = vec![0x42u8; 2 * 1024 * 1024]; // 2MB file
    fs::write(temp_file.path(), &test_data)?;

    println!("Demonstrating the new builder pattern API for checkle's Hasher:");
    println!("File size: {} bytes", test_data.len());

    // Example 1: Basic usage (backward compatible)
    println!("\n1. Basic usage (unchanged from before):");
    let hasher_basic = Hasher::new_md5(temp_file.path());
    let hash_basic = hasher_basic.find_root_hash()?;
    println!("   MD5 hash: {hash_basic}");

    // Example 2: Custom chunk size
    println!("\n2. Custom chunk size (512KB instead of default 256KB):");
    let hasher_custom_chunk = Hasher::new_md5(temp_file.path()).with_chunk_size(512 * 1024)?; // 512KB chunks
    println!("   Chunk size: {}KB", hasher_custom_chunk.chunk_size / 1024);
    let hash_custom_chunk = hasher_custom_chunk.find_root_hash()?;
    println!("   MD5 hash: {hash_custom_chunk}");

    // Example 3: Custom parallel readers
    println!("\n3. Custom parallel readers (2 threads):");
    let hasher_custom_threads = Hasher::new_md5(temp_file.path()).with_parallel_readers(2);
    println!(
        "   Parallel readers: {}",
        hasher_custom_threads.parallel_readers
    );
    let hash_custom_threads = hasher_custom_threads.find_root_hash()?;
    println!("   MD5 hash: {hash_custom_threads}");

    // Example 4: Chained configuration
    println!("\n4. Chained configuration (custom chunk size + parallel readers):");
    let hasher_chained = Hasher::new_sha2(temp_file.path())
        .with_chunk_size(1024 * 1024)? // 1MB chunks
        .with_parallel_readers(4); // 4 threads
    println!(
        "   Chunk size: {}MB",
        hasher_chained.chunk_size / (1024 * 1024)
    );
    println!("   Parallel readers: {}", hasher_chained.parallel_readers);
    let hash_chained = hasher_chained.find_root_hash()?;
    println!("   SHA256 hash: {hash_chained}");

    // Example 5: Demonstrate that all methods produce consistent results for small adjustments
    println!("\n5. Consistency verification:");
    println!("   Basic MD5:  {hash_basic}");
    println!("   Custom MD5: {hash_custom_chunk}");
    println!("   Thread MD5: {hash_custom_threads}");

    if hash_basic == hash_custom_chunk && hash_custom_chunk == hash_custom_threads {
        println!("   ✓ All MD5 hashes are identical (as expected)");
    } else {
        println!("   ✗ Hash mismatch - this shouldn't happen!");
    }

    println!("\nThe new builder pattern maintains full backward compatibility while");
    println!("providing flexible configuration options for advanced use cases!");

    Ok(())
}
