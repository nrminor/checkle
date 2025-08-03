#!/usr/bin/env bash
# benchmark_checksums.sh - Comprehensive benchmarking of checkle against standard checksum utilities
#
# This script benchmarks checkle against various checksum utilities across different
# file sizes to demonstrate performance characteristics on multicore systems.

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
CHECKLE_BIN="$PROJECT_ROOT/target/release/checkle"
TEST_DATA_DIR="$SCRIPT_DIR/test_data"
RESULTS_DIR="$SCRIPT_DIR/results"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Test file sizes (in MB)
FILE_SIZES=(1 10 100 500 1000 5000)

# Number of warmup runs and benchmark runs
WARMUP_RUNS=3
MIN_RUNS=5

# Create necessary directories
mkdir -p "$TEST_DATA_DIR" "$RESULTS_DIR"

# Function to print colored output
print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

# Function to create test files
create_test_files() {
    print_info "Creating test files..."
    
    for size in "${FILE_SIZES[@]}"; do
        file="$TEST_DATA_DIR/test_${size}mb.bin"
        if [[ ! -f "$file" ]]; then
            print_info "Creating ${size}MB test file..."
            dd if=/dev/urandom of="$file" bs=1M count="$size" status=progress 2>&1 | grep -v "records"
        else
            print_info "Test file ${size}MB already exists, skipping..."
        fi
    done
    
    print_success "Test files created successfully"
}

# Function to build checkle in release mode
build_checkle() {
    print_info "Building checkle in release mode..."
    cd "$PROJECT_ROOT"
    cargo build --release
    
    if [[ ! -x "$CHECKLE_BIN" ]]; then
        print_error "Failed to build checkle"
        exit 1
    fi
    
    print_success "checkle built successfully"
}

# Function to check if a command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Function to run benchmark for a specific algorithm and file
run_benchmark() {
    local algorithm="$1"
    local file="$2"
    local size="$3"
    local output_file="$RESULTS_DIR/benchmark_${algorithm}_${size}mb_${TIMESTAMP}.json"
    
    print_info "Benchmarking $algorithm on ${size}MB file..."
    
    case "$algorithm" in
        "checkle_md5")
            if [[ -x "$CHECKLE_BIN" ]]; then
                hyperfine \
                    --warmup "$WARMUP_RUNS" \
                    --min-runs "$MIN_RUNS" \
                    --export-json "$output_file" \
                    --command-name "checkle (MD5)" \
                    "$CHECKLE_BIN hash '$file' --algorithm md5"
            fi
            ;;
        "checkle_sha256")
            if [[ -x "$CHECKLE_BIN" ]]; then
                hyperfine \
                    --warmup "$WARMUP_RUNS" \
                    --min-runs "$MIN_RUNS" \
                    --export-json "$output_file" \
                    --command-name "checkle (SHA256)" \
                    "$CHECKLE_BIN hash '$file' --algorithm sha2"
            fi
            ;;
        "md5sum")
            if command_exists md5sum; then
                hyperfine \
                    --warmup "$WARMUP_RUNS" \
                    --min-runs "$MIN_RUNS" \
                    --export-json "$output_file" \
                    --command-name "md5sum" \
                    "md5sum '$file'"
            elif command_exists md5; then
                # macOS fallback
                hyperfine \
                    --warmup "$WARMUP_RUNS" \
                    --min-runs "$MIN_RUNS" \
                    --export-json "$output_file" \
                    --command-name "md5" \
                    "md5 '$file'"
            fi
            ;;
        "sha256sum")
            if command_exists sha256sum; then
                hyperfine \
                    --warmup "$WARMUP_RUNS" \
                    --min-runs "$MIN_RUNS" \
                    --export-json "$output_file" \
                    --command-name "sha256sum" \
                    "sha256sum '$file'"
            elif command_exists shasum; then
                # macOS fallback
                hyperfine \
                    --warmup "$WARMUP_RUNS" \
                    --min-runs "$MIN_RUNS" \
                    --export-json "$output_file" \
                    --command-name "shasum -a 256" \
                    "shasum -a 256 '$file'"
            fi
            ;;
        "rhash_md5")
            if command_exists rhash; then
                hyperfine \
                    --warmup "$WARMUP_RUNS" \
                    --min-runs "$MIN_RUNS" \
                    --export-json "$output_file" \
                    --command-name "rhash (MD5)" \
                    "rhash --md5 '$file'"
            fi
            ;;
        "rhash_sha256")
            if command_exists rhash; then
                hyperfine \
                    --warmup "$WARMUP_RUNS" \
                    --min-runs "$MIN_RUNS" \
                    --export-json "$output_file" \
                    --command-name "rhash (SHA256)" \
                    "rhash --sha256 '$file'"
            fi
            ;;
        "xxhsum")
            if command_exists xxhsum; then
                hyperfine \
                    --warmup "$WARMUP_RUNS" \
                    --min-runs "$MIN_RUNS" \
                    --export-json "$output_file" \
                    --command-name "xxhsum" \
                    "xxhsum '$file'"
            fi
            ;;
        "b3sum")
            if command_exists b3sum; then
                hyperfine \
                    --warmup "$WARMUP_RUNS" \
                    --min-runs "$MIN_RUNS" \
                    --export-json "$output_file" \
                    --command-name "b3sum" \
                    "b3sum '$file'"
            fi
            ;;
    esac
}

# Function to run comparative benchmark
run_comparative_benchmark() {
    local size="$1"
    local file="$TEST_DATA_DIR/test_${size}mb.bin"
    local output_md="$RESULTS_DIR/comparison_${size}mb_${TIMESTAMP}.md"
    
    print_info "Running comparative benchmark for ${size}MB file..."
    
    # Build command list based on available tools
    local commands=()
    local command_names=()
    
    # Always try checkle first
    if [[ -x "$CHECKLE_BIN" ]]; then
        commands+=("$CHECKLE_BIN hash '$file' --algorithm md5")
        command_names+=("checkle (MD5)")
        
        commands+=("$CHECKLE_BIN hash '$file' --algorithm sha2")
        command_names+=("checkle (SHA256)")
    fi
    
    # Add standard utilities
    if command_exists md5sum; then
        commands+=("md5sum '$file'")
        command_names+=("md5sum")
    elif command_exists md5; then
        commands+=("md5 '$file'")
        command_names+=("md5")
    fi
    
    if command_exists sha256sum; then
        commands+=("sha256sum '$file'")
        command_names+=("sha256sum")
    elif command_exists shasum; then
        commands+=("shasum -a 256 '$file'")
        command_names+=("shasum -a 256")
    fi
    
    # Add additional utilities if available
    if command_exists rhash; then
        commands+=("rhash --md5 '$file'")
        command_names+=("rhash (MD5)")
        
        commands+=("rhash --sha256 '$file'")
        command_names+=("rhash (SHA256)")
    fi
    
    if command_exists xxhsum; then
        commands+=("xxhsum '$file'")
        command_names+=("xxhsum")
    fi
    
    if command_exists b3sum; then
        commands+=("b3sum '$file'")
        command_names+=("b3sum")
    fi
    
    # Build hyperfine command
    local hyperfine_cmd="hyperfine --warmup $WARMUP_RUNS --min-runs $MIN_RUNS --export-markdown '$output_md'"
    
    for i in "${!commands[@]}"; do
        hyperfine_cmd+=" --command-name '${command_names[$i]}' '${commands[$i]}'"
    done
    
    # Run the comparative benchmark
    eval "$hyperfine_cmd"
    
    print_success "Comparative benchmark completed for ${size}MB file"
}

# Function to display system information
show_system_info() {
    print_info "System Information:"
    echo "  OS: $(uname -s)"
    echo "  Architecture: $(uname -m)"
    echo "  CPU Cores: $(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo "unknown")"
    
    if [[ -f /proc/cpuinfo ]]; then
        echo "  CPU Model: $(grep "model name" /proc/cpuinfo | head -1 | cut -d: -f2 | xargs)"
    elif command_exists sysctl; then
        echo "  CPU Model: $(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo "unknown")"
    fi
    
    echo "  Total Memory: $(free -h 2>/dev/null | awk '/^Mem:/ {print $2}' || echo "unknown")"
    echo ""
}

# Function to generate summary report
generate_summary() {
    local summary_file="$RESULTS_DIR/benchmark_summary_${TIMESTAMP}.md"
    
    {
        echo "# checkle Benchmark Results"
        echo ""
        echo "**Date:** $(date)"
        echo ""
        echo "## System Information"
        echo ""
        show_system_info | sed 's/^/  /'
        echo ""
        echo "## Test Configuration"
        echo ""
        echo "- Warmup runs: $WARMUP_RUNS"
        echo "- Minimum runs: $MIN_RUNS"
        echo "- File sizes tested: ${FILE_SIZES[*]} MB"
        echo ""
        echo "## Results"
        echo ""
        
        for size in "${FILE_SIZES[@]}"; do
            local comparison_file="$RESULTS_DIR/comparison_${size}mb_${TIMESTAMP}.md"
            if [[ -f "$comparison_file" ]]; then
                echo "### ${size}MB File"
                echo ""
                cat "$comparison_file"
                echo ""
            fi
        done
        
        echo "## Notes"
        echo ""
        echo "- checkle uses Merkle trees to parallelize hashing across multiple CPU cores"
        echo "- Traditional tools (md5sum, sha256sum) are single-threaded"
        echo "- Performance gains increase with file size and available CPU cores"
        echo "- xxHash and BLAKE3 are included as examples of modern fast hash algorithms"
        
    } > "$summary_file"
    
    print_success "Summary report generated: $summary_file"
}

# Main execution
main() {
    echo "======================================"
    echo "checkle Benchmarking Suite"
    echo "======================================"
    echo ""
    
    # Show system info
    show_system_info
    
    # Check for required tools
    if ! command_exists hyperfine; then
        print_error "hyperfine is required but not installed"
        print_info "Please install hyperfine or run in the nix shell"
        exit 1
    fi
    
    # Build checkle
    build_checkle
    
    # Create test files
    create_test_files
    
    # Run benchmarks for each file size
    for size in "${FILE_SIZES[@]}"; do
        print_info "Starting benchmarks for ${size}MB file..."
        run_comparative_benchmark "$size"
        echo ""
    done
    
    # Generate summary report
    generate_summary
    
    print_success "All benchmarks completed!"
    print_info "Results saved in: $RESULTS_DIR"
}

# Run the script
main "$@"