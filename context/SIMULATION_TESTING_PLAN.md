# Deterministic Simulation Testing System for checkle

## Executive Summary

This document outlines a comprehensive implementation plan for adding a deterministic simulation testing system to checkle. The system will create files with random contents and sizes deterministically (given a seed), process them with checkle, compare results against established checksum utilities, and report discrepancies. The plan considers both external dependency and statically-linked implementation options.

## Core Requirements

1. **Deterministic File Generation**: Create files with reproducible random content based on a seed
2. **Multi-Algorithm Testing**: Verify checkle against md5sum, sha256sum, and other standard utilities
3. **Automated Comparison**: Compare checkle's outputs with reference implementations
4. **Resource Management**: Clean up generated files after testing
5. **Performance Tracking**: Measure and report performance differences
6. **Error Resilience**: Continue testing if reference tools fail
7. **Genomics-Focused**: Include tests specific to bioinformatics file patterns

## Architecture Overview

### Option 1: External Dependencies Approach

Use external command-line tools (md5sum, sha256sum, etc.) for comparison.

**Advantages:**
- Always tests against actual system utilities
- No additional code complexity for hash implementations
- Tests real-world compatibility
- Easier to maintain (no hash implementation updates needed)

**Disadvantages:**
- Platform-dependent (different tools on macOS vs Linux)
- Requires external tools to be installed
- Cannot run in isolated environments
- Slower due to process spawning overhead

### Option 2: Statically Linked Approach

Bundle reference hash implementations directly into checkle binary.

**Advantages:**
- Self-contained, dependency-free testing
- Consistent across all platforms
- Can run in any environment
- Faster execution (no process spawning)
- Better for CI/CD pipelines

**Disadvantages:**
- Increases binary size
- Must maintain reference implementations
- May not catch platform-specific issues
- Additional code complexity

### Recommended Hybrid Approach

Implement a hybrid system that supports both modes:
- Primary mode uses statically linked implementations for portability
- Secondary mode can use external tools when available for verification
- Configuration flag to choose mode: `--simulation-mode [internal|external|both]`

## Implementation Design

### 1. New Subcommand Structure

```rust
// src/cli.rs additions
#[derive(Debug, Args)]
pub struct SimulateCommand {
    /// Random seed for deterministic file generation
    #[arg(long, default_value = "42")]
    pub seed: u64,
    
    /// Number of test iterations
    #[arg(short = 'n', long, default_value = "100")]
    pub iterations: usize,
    
    /// Minimum file size in bytes
    #[arg(long, default_value = "1024")]
    pub min_size: u64,
    
    /// Maximum file size in bytes
    #[arg(long, default_value = "104857600")] // 100MB
    pub max_size: u64,
    
    /// Test directory for generated files
    #[arg(short = 'd', long, default_value = "./checkle_simulation")]
    pub test_dir: PathBuf,
    
    /// Simulation mode
    #[arg(long, value_enum, default_value = "internal")]
    pub mode: SimulationMode,
    
    /// Keep generated files after testing
    #[arg(long)]
    pub keep_files: bool,
    
    /// Output format for results
    #[arg(short = 'o', long, value_enum, default_value = "summary")]
    pub output: OutputFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SimulationMode {
    /// Use internal reference implementations
    Internal,
    /// Use external command-line tools
    External,
    /// Use both and compare
    Both,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    /// Summary statistics only
    Summary,
    /// Detailed results for each file
    Detailed,
    /// JSON output for parsing
    Json,
    /// CSV for analysis
    Csv,
}
```

### 2. File Generation Module

```rust
// src/simulation/generator.rs
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

pub struct FileGenerator {
    rng: ChaCha8Rng,
    patterns: Vec<FilePattern>,
}

#[derive(Debug, Clone)]
pub enum FilePattern {
    /// Pure random bytes
    Random,
    /// Genomics FASTA format
    Fasta { sequence_length: usize },
    /// Genomics FASTQ format
    Fastq { read_count: usize },
    /// Highly compressible (repeated patterns)
    Compressible { pattern_size: usize },
    /// Binary with specific byte distribution
    Binary { zero_ratio: f64 },
    /// Text file with Unicode
    Text { language: TextLanguage },
}

impl FileGenerator {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: ChaCha8Rng::seed_from_u64(seed),
            patterns: Self::default_patterns(),
        }
    }
    
    pub fn generate_file(
        &mut self,
        path: &Path,
        size: u64,
        pattern: &FilePattern,
    ) -> Result<FileMetadata> {
        // Implementation details...
    }
    
    fn default_patterns() -> Vec<FilePattern> {
        vec![
            FilePattern::Random,
            FilePattern::Fasta { sequence_length: 10_000 },
            FilePattern::Fastq { read_count: 1000 },
            FilePattern::Compressible { pattern_size: 1024 },
            FilePattern::Binary { zero_ratio: 0.3 },
        ]
    }
}

pub struct FileMetadata {
    pub path: PathBuf,
    pub size: u64,
    pub pattern: FilePattern,
    pub creation_time: std::time::Instant,
}
```

### 3. Reference Implementation Module (Statically Linked)

```rust
// src/simulation/reference.rs
use md5::{Digest as Md5Digest, Md5};
use sha2::{Sha256, Digest as Sha256Digest};

pub trait ReferenceHasher {
    fn hash_file(&self, path: &Path) -> Result<String>;
    fn algorithm_name(&self) -> &'static str;
}

pub struct ReferenceMd5;
impl ReferenceHasher for ReferenceMd5 {
    fn hash_file(&self, path: &Path) -> Result<String> {
        let mut file = File::open(path)?;
        let mut hasher = Md5::new();
        let mut buffer = vec![0u8; 8192];
        
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 { break; }
            hasher.update(&buffer[..n]);
        }
        
        Ok(format!("{:x}", hasher.finalize()))
    }
    
    fn algorithm_name(&self) -> &'static str {
        "MD5 (reference)"
    }
}

pub struct ReferenceSha256;
impl ReferenceHasher for ReferenceSha256 {
    // Similar implementation for SHA256
}

pub struct ReferenceHashers {
    hashers: HashMap<HashingAlgo, Box<dyn ReferenceHasher>>,
}
```

### 4. External Tool Integration

```rust
// src/simulation/external.rs
use std::process::Command;

pub struct ExternalHasher {
    algorithm: HashingAlgo,
}

impl ExternalHasher {
    pub fn hash_file(&self, path: &Path) -> Result<String> {
        match self.algorithm {
            HashingAlgo::Md5 => self.run_md5sum(path),
            HashingAlgo::Sha2 => self.run_sha256sum(path),
        }
    }
    
    fn run_md5sum(&self, path: &Path) -> Result<String> {
        // Try md5sum first, then md5 (macOS)
        if let Ok(output) = Command::new("md5sum")
            .arg(path)
            .output() 
        {
            // Parse output
        } else if let Ok(output) = Command::new("md5")
            .arg("-q")
            .arg(path)
            .output()
        {
            // Parse macOS md5 output
        } else {
            return Err(Error::ExternalToolNotFound("md5sum or md5"));
        }
    }
}
```

### 5. Comparison Engine

```rust
// src/simulation/comparison.rs
pub struct ComparisonEngine {
    checkle_hasher: CheckleHasher,
    reference_hashers: ReferenceHashers,
    external_hashers: Option<ExternalHashers>,
    results: Vec<ComparisonResult>,
}

#[derive(Debug)]
pub struct ComparisonResult {
    pub file: FileMetadata,
    pub algorithm: HashingAlgo,
    pub checkle_hash: String,
    pub reference_hash: String,
    pub external_hash: Option<String>,
    pub checkle_time: Duration,
    pub reference_time: Duration,
    pub external_time: Option<Duration>,
    pub match_status: MatchStatus,
}

#[derive(Debug, PartialEq)]
pub enum MatchStatus {
    /// All hashes match
    Success,
    /// checkle differs from reference
    Mismatch { expected: String, actual: String },
    /// External tool failed (but checkle matches reference)
    ExternalToolFailure { error: String },
}

impl ComparisonEngine {
    pub fn run_comparison(&mut self, file: &Path) -> Result<ComparisonResult> {
        // 1. Run checkle
        let checkle_start = Instant::now();
        let checkle_hash = self.checkle_hasher.hash_file(file)?;
        let checkle_time = checkle_start.elapsed();
        
        // 2. Run reference implementation
        let reference_start = Instant::now();
        let reference_hash = self.reference_hashers.hash_file(file)?;
        let reference_time = reference_start.elapsed();
        
        // 3. Optionally run external tool
        let (external_hash, external_time) = if let Some(ext) = &self.external_hashers {
            match ext.hash_file(file) {
                Ok(hash) => (Some(hash), Some(external_time)),
                Err(e) => (None, None), // Continue even if external fails
            }
        } else {
            (None, None)
        };
        
        // 4. Compare results
        let match_status = self.determine_match_status(
            &checkle_hash,
            &reference_hash,
            &external_hash,
        );
        
        Ok(ComparisonResult {
            // ... populate fields
        })
    }
}
```

### 6. Simulation Orchestrator

```rust
// src/simulation/orchestrator.rs
pub struct SimulationOrchestrator {
    config: SimulateCommand,
    generator: FileGenerator,
    comparison_engine: ComparisonEngine,
    progress: ProgressReporter,
}

impl SimulationOrchestrator {
    pub fn run(&mut self) -> Result<SimulationReport> {
        // 1. Create test directory
        self.setup_test_directory()?;
        
        // 2. Run iterations
        for i in 0..self.config.iterations {
            // Generate file with deterministic properties
            let size = self.generate_file_size(i);
            let pattern = self.select_pattern(i);
            let file = self.generator.generate_file(
                &self.test_file_path(i),
                size,
                &pattern,
            )?;
            
            // Run comparison
            let result = self.comparison_engine.run_comparison(&file.path)?;
            self.progress.update(i, &result);
            
            // Clean up if needed
            if !self.config.keep_files {
                std::fs::remove_file(&file.path)?;
            }
        }
        
        // 3. Generate report
        Ok(self.generate_report())
    }
    
    fn generate_file_size(&mut self, iteration: usize) -> u64 {
        // Deterministic size generation based on iteration
        // Include edge cases: empty files, exact chunk boundaries, etc.
    }
    
    fn select_pattern(&self, iteration: usize) -> FilePattern {
        // Rotate through patterns deterministically
    }
}
```

### 7. Reporting Module

```rust
// src/simulation/reporting.rs
pub struct SimulationReport {
    pub total_files: usize,
    pub successful_matches: usize,
    pub mismatches: Vec<ComparisonResult>,
    pub external_failures: Vec<ComparisonResult>,
    pub performance_summary: PerformanceSummary,
    pub edge_cases_tested: Vec<EdgeCase>,
}

pub struct PerformanceSummary {
    pub checkle_avg_time: Duration,
    pub reference_avg_time: Duration,
    pub speedup_factor: f64,
    pub throughput_mbps: f64,
}

impl SimulationReport {
    pub fn print_summary(&self) {
        println!("Simulation Testing Report");
        println!("========================");
        println!("Total files tested: {}", self.total_files);
        println!("Successful matches: {} ({:.1}%)", 
            self.successful_matches,
            (self.successful_matches as f64 / self.total_files as f64) * 100.0
        );
        
        if !self.mismatches.is_empty() {
            println!("\nMISMATCHES FOUND:");
            for mismatch in &self.mismatches {
                println!("  File: {} ({})", 
                    mismatch.file.path.display(),
                    mismatch.file.pattern
                );
                println!("    Expected: {}", mismatch.reference_hash);
                println!("    Got:      {}", mismatch.checkle_hash);
            }
        }
        
        println!("\nPerformance Summary:");
        println!("  checkle average: {:?}", self.performance_summary.checkle_avg_time);
        println!("  Reference average: {:?}", self.performance_summary.reference_avg_time);
        println!("  Speedup: {:.2}x", self.performance_summary.speedup_factor);
    }
    
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap()
    }
    
    pub fn to_csv(&self) -> String {
        // CSV generation for analysis
    }
}
```

## Edge Cases to Test

The simulation system should specifically test:

1. **File Size Boundaries**
   - Empty files (0 bytes)
   - Single byte files
   - Exact chunk size (256KB)
   - One byte over chunk size
   - One byte under chunk size
   - Files at parallel I/O threshold (1MB)
   - Very large files (1GB+)

2. **Content Patterns**
   - All zeros
   - All ones (0xFF)
   - Alternating patterns
   - Random data
   - Highly compressible data
   - Unicode text
   - Binary data with specific distributions

3. **Genomics-Specific**
   - FASTA files with long sequences
   - FASTQ files with quality scores
   - Multi-line vs single-line FASTA
   - Compressed genomics formats

4. **Filesystem Edge Cases**
   - Files with special characters in names
   - Very long filenames
   - Symlinks (if following enabled)
   - Files on different mount points

## Integration with Existing Test Suite

### 1. justfile Integration

```bash
# Add to justfile
# Run simulation tests with internal reference
test-simulation:
    cargo run --release -- simulate --iterations 1000

# Run simulation with external tools
test-simulation-external:
    cargo run --release -- simulate --mode external --iterations 100

# Run comprehensive simulation
test-simulation-comprehensive:
    cargo run --release -- simulate --mode both --iterations 1000 --output detailed

# Run simulation with specific seed for debugging
test-simulation-debug seed="123":
    cargo run --release -- simulate --seed {{seed}} --iterations 10 --keep-files --output detailed
```

### 2. CI/CD Integration

```yaml
# Add to .github/workflows/ci.yml
simulation-test:
  name: Simulation Testing
  runs-on: ${{ matrix.os }}
  strategy:
    matrix:
      os: [ubuntu-latest, macos-latest, windows-latest]
  steps:
    - uses: actions/checkout@v4
    - name: Build checkle
      run: cargo build --release
    - name: Run simulation tests (internal)
      run: cargo run --release -- simulate --iterations 500
    - name: Run simulation tests (external)
      run: cargo run --release -- simulate --mode external --iterations 100
      continue-on-error: true  # Don't fail if external tools missing
```

## Performance Considerations

1. **Parallel Testing**: Run multiple file comparisons in parallel using rayon
2. **Memory Usage**: Implement streaming for large file generation
3. **Disk I/O**: Option to use RAM disk for temporary files
4. **Caching**: Cache reference implementations' results for repeated runs

## Security Considerations

1. **Resource Limits**: Enforce maximum file sizes and iteration counts
2. **Path Validation**: Ensure all generated files stay within test directory
3. **Signal Handling**: Proper cleanup on interruption
4. **Permissions**: Handle permission errors gracefully

## Future Extensions

1. **Network Testing**: Test checksum verification over network streams
2. **Corruption Testing**: Introduce controlled bit flips to test error detection
3. **Benchmark Mode**: Extended performance testing with detailed metrics
4. **Fuzzing Integration**: Use simulation infrastructure for fuzz testing
5. **Visual Reports**: Generate HTML reports with graphs

## Implementation Timeline

### Phase 1: Core Infrastructure (Week 1-2)
- File generation module
- Basic comparison engine
- Subcommand integration

### Phase 2: Reference Implementations (Week 2-3)
- Internal MD5/SHA256 implementations
- External tool integration
- Platform-specific handling

### Phase 3: Advanced Features (Week 3-4)
- Genomics-specific patterns
- Performance tracking
- Comprehensive reporting

### Phase 4: Integration & Polish (Week 4-5)
- CI/CD integration
- Documentation
- Performance optimization

## Tradeoffs Summary

### Subcommand vs Traditional Testing

**Subcommand Approach** (`checkle simulate`)
- ✅ User-accessible testing
- ✅ Can be run in production environments
- ✅ Useful for debugging issues
- ❌ Increases binary size
- ❌ Additional maintenance burden

**Traditional Test Approach** (`cargo test`)
- ✅ Follows Rust conventions
- ✅ Integrated with existing test suite
- ✅ No impact on release binary
- ❌ Only available during development
- ❌ Cannot test in production

**Recommendation**: Implement as subcommand for maximum utility, but ensure it can be conditionally compiled out for minimal builds if needed.

## Conclusion

This simulation testing system will provide comprehensive validation of checkle's correctness while maintaining the project's focus on performance and reliability. The hybrid approach balances portability with real-world compatibility testing, and the deterministic generation ensures reproducible results across platforms.

The system aligns with checkle's principles:
- **Performance**: Parallel testing and efficient implementations
- **Correctness**: Comprehensive edge case coverage
- **Usability**: Clear reporting and actionable error messages
- **Genomics Focus**: Specific patterns for bioinformatics workflows

Implementation should follow TIGER_STYLE principles with careful attention to resource limits, assertions, and error handling.