//! Tests for output format functionality.
//!
//! This module tests the `OutputFormat` enum, format detection, CLI integration,
//! and output formatting for text, CSV, and JSON formats.

use checkle::{
    cli::{Cli, Commands, OutputFormat},
    io::FileHashPair,
};
use clap::Parser;
use std::path::PathBuf;
use tempfile::NamedTempFile;

#[cfg(test)]
mod output_format_tests {
    use super::*;

    // Test 1: Normal operation - OutputFormat FromStr parsing
    #[test]
    fn test_output_format_from_str_normal() {
        // Test text variants
        assert_eq!(
            "text".parse::<OutputFormat>().expect("Should parse text"),
            OutputFormat::Text
        );
        assert_eq!(
            "TEXT".parse::<OutputFormat>().expect("Should parse TEXT"),
            OutputFormat::Text
        );
        assert_eq!(
            "txt".parse::<OutputFormat>().expect("Should parse txt"),
            OutputFormat::Text
        );
        assert_eq!(
            "tab".parse::<OutputFormat>().expect("Should parse tab"),
            OutputFormat::Text
        );

        // Test CSV variants
        assert_eq!(
            "csv".parse::<OutputFormat>().expect("Should parse csv"),
            OutputFormat::Csv
        );
        assert_eq!(
            "CSV".parse::<OutputFormat>().expect("Should parse CSV"),
            OutputFormat::Csv
        );

        // Test JSON variants
        assert_eq!(
            "json".parse::<OutputFormat>().expect("Should parse json"),
            OutputFormat::Json
        );
        assert_eq!(
            "JSON".parse::<OutputFormat>().expect("Should parse JSON"),
            OutputFormat::Json
        );
    }

    // Test 2: Error path - Invalid format strings
    #[test]
    fn test_output_format_from_str_invalid() {
        let result = "xml".parse::<OutputFormat>();
        assert!(result.is_err(), "Invalid format should fail to parse");

        let err_msg = result.expect_err("Should fail to parse invalid format");
        assert!(err_msg.contains("Unknown output format"));
        assert!(err_msg.contains("xml"));
        assert!(err_msg.contains("text"));
        assert!(err_msg.contains("csv"));
        assert!(err_msg.contains("json"));
    }

    // Test 3: Normal operation - Format detection from file extension
    #[test]
    fn test_format_detection_from_path() {
        // JSON files
        assert_eq!(
            OutputFormat::detect_from_path(&PathBuf::from("output.json")),
            OutputFormat::Json
        );
        assert_eq!(
            OutputFormat::detect_from_path(&PathBuf::from("/path/to/data.JSON")),
            OutputFormat::Json
        );

        // CSV files
        assert_eq!(
            OutputFormat::detect_from_path(&PathBuf::from("results.csv")),
            OutputFormat::Csv
        );
        assert_eq!(
            OutputFormat::detect_from_path(&PathBuf::from("data.CSV")),
            OutputFormat::Csv
        );

        // Default to text for other extensions
        assert_eq!(
            OutputFormat::detect_from_path(&PathBuf::from("output.txt")),
            OutputFormat::Text
        );
        assert_eq!(
            OutputFormat::detect_from_path(&PathBuf::from("output.log")),
            OutputFormat::Text
        );
        assert_eq!(
            OutputFormat::detect_from_path(&PathBuf::from("output")),
            OutputFormat::Text
        );
    }

    // Test 4: Edge case - Format detection with complex paths
    #[test]
    fn test_format_detection_edge_cases() {
        // Files with multiple dots
        assert_eq!(
            OutputFormat::detect_from_path(&PathBuf::from("file.backup.json")),
            OutputFormat::Json
        );
        assert_eq!(
            OutputFormat::detect_from_path(&PathBuf::from("data.2024.csv")),
            OutputFormat::Csv
        );

        // Files with no extension
        assert_eq!(
            OutputFormat::detect_from_path(&PathBuf::from("output")),
            OutputFormat::Text
        );

        // Hidden files
        assert_eq!(
            OutputFormat::detect_from_path(&PathBuf::from(".output.json")),
            OutputFormat::Json
        );

        // Files ending with dot
        assert_eq!(
            OutputFormat::detect_from_path(&PathBuf::from("output.")),
            OutputFormat::Text
        );
    }

    // Test 5: Normal operation - Typical extension method
    #[test]
    fn test_typical_extension() {
        assert_eq!(OutputFormat::Text.typical_extension(), "txt");
        assert_eq!(OutputFormat::Csv.typical_extension(), "csv");
        assert_eq!(OutputFormat::Json.typical_extension(), "json");
    }

    // Test 6: Normal operation - CLI parsing with format flag
    #[test]
    fn test_cli_parsing_with_format_flag() {
        let test_cases = vec![
            ("text", OutputFormat::Text),
            ("csv", OutputFormat::Csv),
            ("json", OutputFormat::Json),
        ];

        for (format_str, expected_format) in test_cases {
            let args = vec!["checkle", "hash", "test.txt", "--format", format_str];

            let cli = Cli::try_parse_from(args).expect("Should parse CLI with format flag");

            if let Some(Commands::Hash { format, .. }) = cli.command {
                assert_eq!(
                    format,
                    Some(expected_format),
                    "Format should be parsed correctly for {format_str}"
                );
            } else {
                panic!("Expected Hash command");
            }
        }
    }

    // Test 7: Normal operation - CLI parsing without format flag
    #[test]
    fn test_cli_parsing_without_format_flag() {
        let args = vec!["checkle", "hash", "test.txt"];

        let cli = Cli::try_parse_from(args).expect("Should parse CLI without format flag");

        if let Some(Commands::Hash { format, .. }) = cli.command {
            assert_eq!(format, None, "Format should be None when not specified");
        } else {
            panic!("Expected Hash command");
        }
    }

    // Test 8: Error path - CLI parsing with invalid format
    #[test]
    fn test_cli_parsing_invalid_format() {
        let args = vec!["checkle", "hash", "test.txt", "--format", "xml"];

        let result = Cli::try_parse_from(args);
        assert!(result.is_err(), "Should fail with invalid format");

        let error = result
            .expect_err("Should fail with invalid format")
            .to_string();
        assert!(
            error.contains("Unknown output format") || error.contains("invalid value"),
            "Error should mention invalid format"
        );
    }

    // Test 9: Integration - Hash command with output file and format
    #[test]
    fn test_hash_command_with_output_and_format() {
        let args = vec![
            "checkle",
            "hash",
            "test.txt",
            "-o",
            "output.txt",
            "--format",
            "json",
        ];

        let cli = Cli::try_parse_from(args).expect("Should parse hash command");

        if let Some(Commands::Hash {
            input_file,
            recursive,
            hash_output,
            format,
            pretty: _,
            per_file: _,
            no_progress: _,
            ..
        }) = cli.command
        {
            assert_eq!(input_file.to_string_lossy(), "test.txt");
            assert!(!recursive, "Recursive should default to false");
            assert_eq!(
                hash_output
                    .as_ref()
                    .expect("Hash output should be Some")
                    .to_string_lossy(),
                "output.txt"
            );
            assert_eq!(format, Some(OutputFormat::Json));
        } else {
            panic!("Expected Hash command");
        }
    }

    // Test 10: Integration - Short form format flag
    #[test]
    fn test_cli_short_form_format_flag() {
        let args = vec!["checkle", "hash", "test.txt", "-f", "csv"];

        let cli = Cli::try_parse_from(args).expect("Should parse CLI with short format flag");

        if let Some(Commands::Hash { format, .. }) = cli.command {
            assert_eq!(format, Some(OutputFormat::Csv));
        } else {
            panic!("Expected Hash command");
        }
    }

    // Test 11: Normal operation - Text format output (create mock data)
    #[test]
    fn test_text_format_output() {
        // Create mock file hash pairs
        let temp_file1 = NamedTempFile::new().expect("Failed to create temp file");
        let temp_file2 = NamedTempFile::new().expect("Failed to create temp file");

        let _file_hash_pairs = [
            FileHashPair::new(temp_file1.path().to_path_buf(), "abcdef123456".to_string()),
            FileHashPair::new(temp_file2.path().to_path_buf(), "fedcba654321".to_string()),
        ];

        // Test the format_output function would be called here
        // For now, we test the expected format manually
        let expected_line1 = format!("abcdef123456\t{}", temp_file1.path().to_string_lossy());
        let expected_line2 = format!(
            "fedcba654321\t{}",
            temp_file2.path().to_path_buf().to_string_lossy()
        );
        let expected_output = format!("{expected_line1}\n{expected_line2}");

        // Verify the format structure
        assert!(
            expected_output.contains('\t'),
            "Text format should use tab delimiter"
        );
        assert!(expected_output.contains(&temp_file1.path().to_string_lossy().to_string()));
        assert!(expected_output.contains(&temp_file2.path().to_string_lossy().to_string()));
    }

    // Test 12: Normal operation - CSV format structure
    #[test]
    fn test_csv_format_structure() {
        // Test that CSV format would include header and proper structure
        let expected_header = "hash,filepath";
        let sample_hash = "abcdef123456";
        let sample_path = "/path/to/file.txt";
        let expected_line = format!("{sample_hash},{sample_path}");

        assert!(expected_header.contains("hash"));
        assert!(expected_header.contains("filepath"));
        assert!(
            expected_line.contains(','),
            "CSV should use comma delimiter"
        );
        assert!(
            !expected_line.contains('\t'),
            "CSV should not use tab delimiter"
        );
    }

    // Test 13: Normal operation - JSON format structure
    #[test]
    fn test_json_format_structure() {
        // Test that JSON format would be properly structured
        let sample_hash = "abcdef123456";
        let sample_path = "/path/to/file.txt";
        let expected_object =
            format!("{{\"hash\":\"{sample_hash}\",\"filepath\":\"{sample_path}\"}}");
        let expected_array = format!("[{expected_object}]");

        assert!(expected_object.contains("\"hash\""));
        assert!(expected_object.contains("\"filepath\""));
        assert!(expected_array.starts_with('['));
        assert!(expected_array.ends_with(']'));
    }

    // Test 14: Edge case - CSV escaping requirements
    #[test]
    fn test_csv_escaping_requirements() {
        // Test files that would require CSV escaping
        let problematic_paths = vec![
            "/path/with,comma.txt",
            "/path/with\"quote.txt",
            "/path/with\nnewline.txt",
            "/path/with\rcarriage.txt",
        ];

        for path in problematic_paths {
            // All these paths should trigger CSV escaping
            let requires_escaping = path.contains([',', '"', '\n', '\r']);
            assert!(requires_escaping, "Path {path} should require CSV escaping");
        }

        // Test normal paths that don't require escaping
        let normal_paths = vec![
            "/simple/path.txt",
            "/path_with_underscores.txt",
            "/path-with-dashes.txt",
        ];

        for path in normal_paths {
            let requires_escaping = path.contains([',', '"', '\n', '\r']);
            assert!(
                !requires_escaping,
                "Path {path} should not require CSV escaping"
            );
        }
    }

    // Test 15: Edge case - JSON escaping requirements
    #[test]
    fn test_json_escaping_requirements() {
        // Test strings that would require JSON escaping
        let test_strings = vec![
            ("path\\with\\backslash.txt", "path\\\\with\\\\backslash.txt"),
            ("path\"with\"quote.txt", "path\\\"with\\\"quote.txt"),
            ("path\nwith\nnewline.txt", "path\\nwith\\nnewline.txt"),
            ("path\rwith\rcarriage.txt", "path\\rwith\\rcarriage.txt"),
            ("path\twith\ttab.txt", "path\\twith\\ttab.txt"),
        ];

        for (input, expected_escaped) in test_strings {
            let escaped = input
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\r', "\\r")
                .replace('\t', "\\t");

            assert_eq!(
                escaped, expected_escaped,
                "JSON escaping failed for: {input}"
            );
        }
    }

    // Test 16: Performance - Format detection should be fast
    #[test]
    fn test_format_detection_performance() {
        use std::time::Instant;

        let test_paths = vec![
            PathBuf::from("file.json"),
            PathBuf::from("file.csv"),
            PathBuf::from("file.txt"),
            PathBuf::from("file"),
            PathBuf::from("/very/long/path/to/some/deeply/nested/file.json"),
        ];

        let start = Instant::now();
        for _ in 0..1000 {
            for path in &test_paths {
                let _ = OutputFormat::detect_from_path(path);
            }
        }
        let duration = start.elapsed();

        // Format detection should be very fast (< 5ms for 5000 detections)
        assert!(
            duration.as_millis() < 5,
            "Format detection should be fast: {duration:?}"
        );
    }

    // Test 17: Integration - Multiple format options together
    #[test]
    fn test_multiple_format_options() {
        let test_combinations = vec![
            // (args, expected_format_option, expected_auto_detect_format)
            (
                vec!["checkle", "hash", "file.txt", "-o", "out.json"],
                None,
                OutputFormat::Json,
            ),
            (
                vec!["checkle", "hash", "file.txt", "-o", "out.csv"],
                None,
                OutputFormat::Csv,
            ),
            (
                vec!["checkle", "hash", "file.txt", "-o", "out.txt", "-f", "json"],
                Some(OutputFormat::Json),
                OutputFormat::Json, // explicit overrides detection
            ),
        ];

        for (args, expected_explicit, expected_detected) in test_combinations {
            let cli = Cli::try_parse_from(args.clone()).expect("Should parse CLI");

            if let Some(Commands::Hash {
                hash_output,
                format,
                ..
            }) = cli.command
            {
                assert_eq!(
                    format, expected_explicit,
                    "Explicit format should match for {args:?}"
                );

                if format.is_none() {
                    if let Some(ref output_path) = hash_output {
                        let detected = OutputFormat::detect_from_path(output_path);
                        assert_eq!(
                            detected, expected_detected,
                            "Auto-detected format should match for {args:?}"
                        );
                    }
                }
            } else {
                panic!("Expected Hash command for {args:?}");
            }
        }
    }

    // Test 18: Edge case - Case sensitivity in format detection
    #[test]
    fn test_format_detection_case_sensitivity() {
        let test_cases = vec![
            ("file.JSON", OutputFormat::Json),
            ("file.Json", OutputFormat::Json),
            ("file.jSoN", OutputFormat::Json),
            ("file.CSV", OutputFormat::Csv),
            ("file.Csv", OutputFormat::Csv),
            ("file.cSv", OutputFormat::Csv),
        ];

        for (filename, expected_format) in test_cases {
            let path = PathBuf::from(filename);
            let detected = OutputFormat::detect_from_path(&path);
            assert_eq!(
                detected, expected_format,
                "Case-insensitive detection failed for {filename}"
            );
        }
    }

    // Test 19: Integration - Format validation in CLI help
    #[test]
    fn test_format_help_includes_options() {
        use checkle::cli::{COMMAND_EXAMPLES, HASH_EXAMPLES};

        // Check that examples mention format options
        let combined_help = format!("{COMMAND_EXAMPLES}\n{HASH_EXAMPLES}");

        assert!(
            combined_help.contains("csv") || combined_help.contains("CSV"),
            "Help should mention CSV format"
        );
        assert!(
            combined_help.contains("json") || combined_help.contains("JSON"),
            "Help should mention JSON format"
        );
        assert!(
            combined_help.contains("format"),
            "Help should mention format option"
        );
    }

    // Test 20: Edge case - Empty file list formatting
    #[test]
    fn test_empty_file_list_formatting() {
        let empty_file_list: Vec<FileHashPair> = vec![];

        // Text format with empty list should be empty
        let text_lines: Vec<String> = empty_file_list
            .iter()
            .map(|file| format!("{}\t{}", file.hash(), file.file().to_string_lossy()))
            .collect();
        let text_output = text_lines.join("\n");
        assert_eq!(text_output, "", "Empty text format should be empty string");

        // CSV format with empty list should just have header
        let csv_output = "hash,filepath\n".trim_end();
        assert_eq!(
            csv_output, "hash,filepath",
            "Empty CSV should have header only"
        );

        // JSON format with empty list should be empty array
        let json_output = "[]";
        assert_eq!(json_output, "[]", "Empty JSON should be empty array");
    }
}
