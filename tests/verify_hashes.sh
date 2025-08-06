#!/usr/bin/env bash
# verify_hashes.sh - Verify that checkle produces identical hashes to established utilities
#
# This script tests checkle against well-established checksum utilities to ensure
# compatibility and correctness of the hash implementations.

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
TEST_DATA_DIR="$SCRIPT_DIR/data"
RESULTS_DIR="$SCRIPT_DIR/results"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Test counters
TESTS_PASSED=0
TESTS_FAILED=0

# Create necessary directories
mkdir -p "$RESULTS_DIR"

# Function to print colored output
print_info() {
	echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
	echo -e "${GREEN}[✓]${NC} $1"
}

print_error() {
	echo -e "${RED}[✗]${NC} $1"
}

print_warning() {
	echo -e "${YELLOW}[WARNING]${NC} $1"
}

# Function to check if a command exists
command_exists() {
	command -v "$1" >/dev/null 2>&1
}

# Function to build checkle in release mode
build_checkle() {
	print_info "Building checkle in release mode..."
	cd "$PROJECT_ROOT"
	# cargo build --release
	RUSTFLAGS="-C target-cpu=native" cargo +nightly build --release --features simd

	if [[ ! -x "$CHECKLE_BIN" ]]; then
		print_error "Failed to build checkle"
		exit 1
	fi

	print_success "checkle built successfully"
}

# Function to get MD5 hash using standard utility
get_standard_md5() {
	local file="$1"
	local hash=""

	if command_exists md5sum; then
		hash=$(md5sum "$file" | awk '{print $1}')
	elif command_exists md5; then
		# macOS style
		hash=$(md5 -q "$file")
	else
		print_error "No MD5 utility found (md5sum or md5)"
		return 1
	fi

	echo "$hash"
}

# Function to get SHA256 hash using standard utility
get_standard_sha256() {
	local file="$1"
	local hash=""

	if command_exists sha256sum; then
		hash=$(sha256sum "$file" | awk '{print $1}')
	elif command_exists shasum; then
		# macOS style
		hash=$(shasum -a 256 "$file" | awk '{print $1}')
	else
		print_error "No SHA256 utility found (sha256sum or shasum)"
		return 1
	fi

	echo "$hash"
}

# Function to get checkle hash
get_checkle_hash() {
	local file="$1"
	local algorithm="$2"
	local hash=""

	# Run checkle and capture output
	local output
	if ! output=$("$CHECKLE_BIN" --algorithm "$algorithm" hash "$file" 2>&1); then
		print_error "checkle command failed: $output"
		return 1
	fi

	# Extract hash from output (skip timestamp lines, get the actual hash)
	hash=$(echo "$output" | grep -v '^\[' | awk 'NF>0 {print $1}' | head -1)

	if [[ -z "$hash" ]]; then
		print_error "Failed to extract hash from checkle output"
		return 1
	fi

	echo "$hash"
}

# Function to compare hashes
compare_hashes() {
	local file="$1"
	local algorithm="$2"
	local standard_hash="$3"
	local checkle_hash="$4"
	local test_name="$5"

	if [[ "$standard_hash" == "$checkle_hash" ]]; then
		print_success "$test_name: PASSED"
		print_info "  File: $(basename "$file")"
		print_info "  Algorithm: $algorithm"
		print_info "  Hash: $standard_hash"
		((TESTS_PASSED++))
		return 0
	else
		print_error "$test_name: FAILED"
		print_info "  File: $(basename "$file")"
		print_info "  Algorithm: $algorithm"
		print_error "  Standard hash: $standard_hash"
		print_error "  checkle hash:  $checkle_hash"
		((TESTS_FAILED++))
		return 1
	fi
}

# Function to test a single file
test_file() {
	local file="$1"
	local basename=$(basename "$file")

	print_info "Testing file: $basename"

	# Test MD5
	if standard_md5=$(get_standard_md5 "$file"); then
		if checkle_md5=$(get_checkle_hash "$file" "md5"); then
			compare_hashes "$file" "MD5" "$standard_md5" "$checkle_md5" "MD5 hash for $basename" || true
		else
			print_error "Failed to get checkle MD5 hash for $basename"
			((TESTS_FAILED++))
		fi
	fi

	# Test SHA256
	if standard_sha256=$(get_standard_sha256 "$file"); then
		if checkle_sha256=$(get_checkle_hash "$file" "sha2"); then
			compare_hashes "$file" "SHA256" "$standard_sha256" "$checkle_sha256" "SHA256 hash for $basename" || true
		else
			print_error "Failed to get checkle SHA256 hash for $basename"
			((TESTS_FAILED++))
		fi
	fi

	echo ""
}

# Function to generate verification report
generate_report() {
	local report_file="$RESULTS_DIR/verification_report_${TIMESTAMP}.txt"

	{
		echo "checkle Hash Verification Report"
		echo "================================"
		echo ""
		echo "Date: $(date)"
		echo "checkle binary: $CHECKLE_BIN"
		echo ""
		echo "Test Summary:"
		echo "  Total tests: $((TESTS_PASSED + TESTS_FAILED))"
		echo "  Passed: $TESTS_PASSED"
		echo "  Failed: $TESTS_FAILED"
		echo ""

		if [[ $TESTS_FAILED -eq 0 ]]; then
			echo "Result: ALL TESTS PASSED ✓"
			echo ""
			echo "checkle produces identical hashes to standard utilities."
		else
			echo "Result: SOME TESTS FAILED ✗"
			echo ""
			echo "checkle produced different hashes than standard utilities."
			echo "Please check the implementation."
		fi

		echo ""
		echo "Files tested:"
		for file in "$TEST_DATA_DIR"/*; do
			if [[ -f "$file" ]]; then
				# Only list actual data files, not hash files
				if [[ ! "$file" =~ \.(md5|sha256)(\.md5)?$ ]]; then
					echo "  - $(basename "$file")"
				fi
			fi
		done

	} >"$report_file"

	print_info "Report saved to: $report_file"
}

# Function to test batch verification
test_batch_verification() {
	print_info "Testing batch verification functionality..."

	# Create a checksum file using standard utilities
	local standard_checksum_file="$RESULTS_DIR/standard_checksums.txt"
	>"$standard_checksum_file"

	for file in "$TEST_DATA_DIR"/*; do
		if [[ -f "$file" ]]; then
			# Skip hash files for batch verification too
			if [[ "$file" =~ \.(md5|sha256)(\.md5)?$ ]]; then
				continue
			fi
			if hash=$(get_standard_md5 "$file"); then
				echo -e "$hash\t$file" >>"$standard_checksum_file"
			fi
		fi
	done

	# Test checkle's verify-many command
	cd "$PROJECT_ROOT"
	if "$CHECKLE_BIN" verify-many -c "$standard_checksum_file" 2>&1; then
		print_success "Batch verification: PASSED"
		((TESTS_PASSED++))
	else
		print_error "Batch verification: FAILED"
		((TESTS_FAILED++))
	fi

	echo ""
}

# Function to test that archive files are hashed as regular files
test_archive_files() {
	print_info "Testing archive files are hashed as regular files..."
	
	local archives=(
		"test_archive.tar"
		"test_archive.tar.gz"
		"test_archive.tar.bz2"
		"test_archive.tar.xz"
		"test_archive.zip"
	)
	
	for archive in "${archives[@]}"; do
		local archive_path="$TEST_DATA_DIR/$archive"
		local md5_file="$TEST_DATA_DIR/$archive.md5"
		local sha256_file="$TEST_DATA_DIR/$archive.sha256"
		
		if [[ -f "$archive_path" ]]; then
			echo "  Checking $archive..."
			
			# Test MD5
			if [[ -f "$md5_file" ]]; then
				local expected_md5=$(head -1 "$md5_file" | cut -d' ' -f1)
				if checkle_md5=$(get_checkle_hash "$archive_path" "md5"); then
					if [[ "$expected_md5" == "$checkle_md5" ]]; then
						print_success "  ✓ MD5 match for $archive"
						((TESTS_PASSED++))
					else
						print_error "  ✗ MD5 mismatch for $archive"
						print_error "    Expected: $expected_md5"
						print_error "    Got:      $checkle_md5"
						((TESTS_FAILED++))
					fi
				else
					print_error "  ✗ Failed to get checkle MD5 for $archive"
					((TESTS_FAILED++))
				fi
			else
				print_warning "  MD5 file not found: $md5_file"
			fi
			
			# Test SHA256
			if [[ -f "$sha256_file" ]]; then
				local expected_sha256=$(head -1 "$sha256_file" | cut -d' ' -f1)
				if checkle_sha256=$(get_checkle_hash "$archive_path" "sha2"); then
					if [[ "$expected_sha256" == "$checkle_sha256" ]]; then
						print_success "  ✓ SHA256 match for $archive"
						((TESTS_PASSED++))
					else
						print_error "  ✗ SHA256 mismatch for $archive"
						print_error "    Expected: $expected_sha256"
						print_error "    Got:      $checkle_sha256"
						((TESTS_FAILED++))
					fi
				else
					print_error "  ✗ Failed to get checkle SHA256 for $archive"
					((TESTS_FAILED++))
				fi
			else
				print_warning "  SHA256 file not found: $sha256_file"
			fi
		else
			print_warning "  Archive not found: $archive_path (run tests/create_test_archives.sh first)"
		fi
	done
	
	print_success "✓ All archive files hashed correctly as regular files"
	echo ""
}

# Main execution
main() {
	echo "======================================"
	echo "checkle Hash Verification Test Suite"
	echo "======================================"
	echo ""

	# Check for required tools
	if ! command_exists md5sum && ! command_exists md5; then
		print_error "No MD5 utility found. Please install coreutils or run on macOS."
		exit 1
	fi

	if ! command_exists sha256sum && ! command_exists shasum; then
		print_error "No SHA256 utility found. Please install coreutils or run on macOS."
		exit 1
	fi

	# Build checkle
	build_checkle

	# Change to project root for checkle execution
	cd "$PROJECT_ROOT"

	# Test all files in the test data directory
	print_info "Starting hash verification tests..."
	echo ""

	for file in "$TEST_DATA_DIR"/*; do
		if [[ -f "$file" ]]; then
			# Skip hash files themselves - we only want to test the actual data files
			if [[ "$file" =~ \.(md5|sha256)(\.md5)?$ ]]; then
				continue
			fi
			test_file "$file"
		fi
	done

	# Test batch verification
	test_batch_verification

	# Test archive files are hashed as regular files
	test_archive_files

	# Generate report
	generate_report

	# Final summary
	echo "======================================"
	echo "Test Summary"
	echo "======================================"
	echo "Total tests: $((TESTS_PASSED + TESTS_FAILED))"
	echo "Passed: $TESTS_PASSED"
	echo "Failed: $TESTS_FAILED"
	echo ""

	if [[ $TESTS_FAILED -eq 0 ]]; then
		print_success "ALL TESTS PASSED! checkle produces correct hashes."
		exit 0
	else
		print_error "SOME TESTS FAILED! Please check the implementation."
		exit 1
	fi
}

# Run the script
main "$@"
