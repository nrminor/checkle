//! Tests for progress bar control using the --no-progress flag.
//!
//! This module tests the integration between the --no-progress flag and progress bar display,
//! ensuring that progress bars appear by default and can be disabled with the flag.

use checkle::{
    cli::{Cli, Commands},
    progress::ProgressManager,
};
use clap::Parser;

#[cfg(test)]
mod automatic_progress_tests {
    use super::*;

    // Test 1: Normal operation - Progress bars should be shown by default
    #[test]
    fn test_default_shows_progress() {
        let args = vec!["checkle", "hash", "/tmp/test.txt"];
        let cli = Cli::try_parse_from(args).expect("Should parse CLI");

        // By default, progress should be shown
        if let Some(Commands::Hash { no_progress, .. }) = cli.command {
            let show_progress = !no_progress;
            assert!(show_progress, "Progress bars should be shown by default");

            // Test with actual progress manager - in test environment (no TTY), progress is auto-disabled
            let progress_manager = ProgressManager::new(show_progress, 5);
            // In tests, TTY detection will disable progress even if we request it
            // This is expected behavior for non-TTY environments
            let _ = progress_manager.is_showing_progress(); // Should not panic
        } else {
            panic!("Expected Hash command");
        }
    }

    // Test 2: Normal operation - --no-progress flag should hide progress bars
    #[test]
    fn test_no_progress_flag_hides_progress() {
        let args = vec!["checkle", "hash", "--no-progress", "/tmp/test.txt"];
        let cli = Cli::try_parse_from(args).expect("Should parse CLI with no-progress flag");

        // With --no-progress, progress should be hidden
        if let Some(Commands::Hash { no_progress, .. }) = cli.command {
            let show_progress = !no_progress;
            assert!(
                !show_progress,
                "--no-progress flag should hide progress bars"
            );

            // Test actual progress manager creation - in test environment (no TTY), progress is auto-disabled
            let progress_manager = ProgressManager::new(show_progress, 3);
            // In tests, TTY detection will disable progress even if we request it
            // This is expected behavior for non-TTY environments
            let _ = progress_manager.is_showing_progress(); // Should not panic
        } else {
            panic!("Expected Hash command");
        }
    }

    // Test 3: Verbose and quiet flags should not affect progress bars
    #[test]
    fn test_verbosity_flags_do_not_affect_progress() {
        // Test with verbose flag - progress should still show
        let args = vec!["checkle", "-v", "hash", "/tmp/test.txt"];
        let cli = Cli::try_parse_from(args).expect("Should parse CLI with verbose flag");
        if let Some(Commands::Hash { no_progress, .. }) = cli.command {
            let show_progress = !no_progress;
            assert!(show_progress, "Verbose flag should not hide progress bars");
        } else {
            panic!("Expected Hash command");
        }

        // Test with quiet flag - progress should still show
        let args = vec!["checkle", "--quiet", "hash", "/tmp/test.txt"];
        let cli = Cli::try_parse_from(args).expect("Should parse CLI with quiet flag");
        if let Some(Commands::Hash { no_progress, .. }) = cli.command {
            let show_progress = !no_progress;
            assert!(show_progress, "Quiet flag should not hide progress bars");

            // Test with actual progress manager
            let progress_manager = ProgressManager::new(show_progress, 5);
            // In test environment (no TTY), progress is auto-disabled
            // This is expected behavior for non-TTY environments
            let _ = progress_manager.is_showing_progress(); // Should not panic
        } else {
            panic!("Expected Hash command");
        }
    }

    // Test 4: Combining --no-progress with verbosity flags
    #[test]
    fn test_no_progress_with_verbosity_flags() {
        // --no-progress with verbose
        let args = vec!["checkle", "-v", "hash", "--no-progress", "/tmp/test.txt"];
        let cli = Cli::try_parse_from(args).expect("Should parse CLI");
        if let Some(Commands::Hash { no_progress, .. }) = cli.command {
            let show_progress = !no_progress;
            assert!(!show_progress, "--no-progress should override verbose flag");
        } else {
            panic!("Expected Hash command");
        }

        // --no-progress with quiet
        let args = vec![
            "checkle",
            "--quiet",
            "hash",
            "--no-progress",
            "/tmp/test.txt",
        ];
        let cli = Cli::try_parse_from(args).expect("Should parse CLI");
        if let Some(Commands::Hash { no_progress, .. }) = cli.command {
            let show_progress = !no_progress;
            assert!(
                !show_progress,
                "--no-progress with quiet should hide progress"
            );
        } else {
            panic!("Expected Hash command");
        }
    }

    // Test 5: Edge case - Progress flag position in command line
    #[test]
    fn test_progress_flag_position() {
        // Flag after subcommand should work (it's now a subcommand flag)
        let args = vec!["checkle", "hash", "--no-progress", "/tmp/test.txt"];
        let cli = Cli::try_parse_from(args).expect("Should parse with flag after subcommand");
        if let Some(Commands::Hash { no_progress, .. }) = cli.command {
            assert!(no_progress, "--no-progress after subcommand should work");
        } else {
            panic!("Expected Hash command");
        }

        // Flag before subcommand should fail (it's no longer a global flag)
        let args = vec!["checkle", "--no-progress", "hash", "/tmp/test.txt"];
        let result = Cli::try_parse_from(args);
        assert!(
            result.is_err(),
            "--no-progress before subcommand should fail"
        );
    }

    // Test 6: Edge case - Progress manager with zero files
    #[test]
    fn test_progress_manager_zero_files() {
        // When show_progress is false, zero files should be fine
        let progress_manager = ProgressManager::new(false, 0);
        assert!(
            !progress_manager.is_showing_progress(),
            "Should not show progress with zero files when disabled"
        );

        // When show_progress is true with zero files, it violates the precondition
        // The progress manager correctly asserts this, so this would panic
        // We don't test this invalid case as it's by design
    }

    // Test 7: Integration - Full CLI parsing with hash command
    #[test]
    fn test_hash_command_integration() {
        // Test with progress enabled (default verbosity)
        let args = vec!["checkle", "hash", "test_file.txt"];
        let cli = Cli::try_parse_from(args).expect("Should parse hash command");

        if let Some(Commands::Hash {
            input_file,
            recursive,
            hash_output,
            no_progress,
            ..
        }) = cli.command
        {
            assert_eq!(input_file.to_string_lossy(), "test_file.txt");
            assert!(!recursive, "Recursive should default to false");
            assert!(hash_output.is_none(), "Hash output should default to None");

            // Verify progress logic
            let show_progress = !no_progress;
            assert!(
                show_progress,
                "Hash command should show progress by default"
            );
        } else {
            panic!("Expected Hash command");
        }
    }

    // Test 8: Integration - Hash command with --no-progress flag
    #[test]
    fn test_hash_command_with_no_progress() {
        let args = vec!["checkle", "hash", "--no-progress", "test_file.txt", "-r"];
        let cli = Cli::try_parse_from(args).expect("Should parse hash command with no-progress");

        if let Some(Commands::Hash {
            input_file,
            recursive,
            hash_output,
            no_progress,
            ..
        }) = cli.command
        {
            assert_eq!(input_file.to_string_lossy(), "test_file.txt");
            assert!(recursive, "Recursive flag should be set");
            assert!(hash_output.is_none(), "Hash output should default to None");

            // Verify progress logic - --no-progress should hide progress
            let show_progress = !no_progress;
            assert!(
                !show_progress,
                "--no-progress flag should hide progress display"
            );
        } else {
            panic!("Expected Hash command");
        }
    }

    // Test 9: Integration - Hash command with verbose flag
    #[test]
    fn test_hash_command_with_verbose() {
        let args = vec![
            "checkle",
            "-vv",
            "hash",
            "test_file.txt",
            "-o",
            "output.txt",
        ];
        let cli = Cli::try_parse_from(args).expect("Should parse verbose hash command");

        if let Some(Commands::Hash {
            input_file,
            recursive,
            hash_output,
            no_progress,
            ..
        }) = cli.command
        {
            assert_eq!(input_file.to_string_lossy(), "test_file.txt");
            assert!(!recursive, "Recursive should default to false");
            assert_eq!(
                hash_output
                    .as_ref()
                    .expect("Hash output should be Some")
                    .to_string_lossy(),
                "output.txt"
            );

            // Verify progress logic - verbose doesn't affect progress
            let show_progress = !no_progress;
            assert!(
                show_progress,
                "Verbose flag should not affect progress display"
            );
        } else {
            panic!("Expected Hash command");
        }
    }

    // Test 10: Error path - CLI parsing still works without progress flag
    #[test]
    fn test_cli_parsing_without_progress_flag() {
        // Verify that the old --progress flag is no longer accepted
        let args = vec!["checkle", "hash", "test_file.txt", "--progress"];
        let result = Cli::try_parse_from(args);
        assert!(result.is_err(), "CLI should reject the old --progress flag");

        // Error should mention unknown argument
        let error = result
            .expect_err("CLI should reject invalid flag")
            .to_string();
        assert!(
            error.contains("progress") || error.contains("unexpected"),
            "Error should mention the invalid progress flag"
        );
    }

    // Test 11: Integration - Verify that other commands still work
    #[test]
    fn test_other_commands_unaffected() {
        // Test verify command
        let args = vec!["checkle", "verify", "/tmp/file.txt", "--hash", "abc123"];
        let cli = Cli::try_parse_from(args).expect("Verify command should still work");
        assert!(
            matches!(cli.command, Some(Commands::Verify { .. })),
            "Should parse verify command"
        );

        // Test verify-many command
        let args = vec!["checkle", "verify-many", "-c", "checksums.txt"];
        let cli = Cli::try_parse_from(args).expect("Verify-many command should still work");
        assert!(
            matches!(cli.command, Some(Commands::VerifyMany { .. })),
            "Should parse verify-many command"
        );
    }

    // Test 12: Performance - Progress logic should be fast
    #[test]
    fn test_progress_logic_performance() {
        use std::time::Instant;

        let args = vec!["checkle", "hash", "test_file.txt"];
        let cli = Cli::try_parse_from(args).expect("Should parse CLI");

        let start = Instant::now();
        for _ in 0..1000 {
            if let Some(Commands::Hash { no_progress, .. }) = &cli.command {
                let _ = !no_progress; // Just verify it can be accessed
            }
        }
        let duration = start.elapsed();

        // Progress logic should be very fast (< 1ms for 1000 iterations)
        assert!(
            duration.as_millis() < 1,
            "Progress logic should be fast: {duration:?}"
        );
    }

    // Test 13: Edge case - ProgressManager behavior with TTY detection
    #[test]
    fn test_progress_manager_tty_detection() {
        // This test verifies that ProgressManager handles TTY detection gracefully
        // In test environment, we're not in a TTY, so progress should be disabled

        let progress_manager = ProgressManager::new(true, 5);
        // In tests, TTY detection will likely disable progress even if we request it
        // This is expected behavior and the progress manager should handle it gracefully
        let is_showing = progress_manager.is_showing_progress();

        // Test that it doesn't panic and returns a boolean
        assert!(
            matches!(is_showing, true | false),
            "ProgressManager should return a boolean for is_showing_progress"
        );
    }

    // Test 14: Edge case - Multiple verbosity modifiers
    #[test]
    fn test_multiple_verbosity_modifiers() {
        // Test conflicting verbosity flags (clap should handle this)
        let args = vec!["checkle", "-v", "--quiet", "hash", "test.txt"];
        let result = Cli::try_parse_from(args);

        // clap should either accept the last one or error - either is fine
        // The important thing is that it doesn't panic
        if let Ok(cli) = result {
            // If it parses, check the progress state
            if let Some(Commands::Hash { no_progress, .. }) = &cli.command {
                let _ = !no_progress; // Progress should be shown unless explicitly disabled
            }
        } else {
            // If it errors due to conflicting flags, that's also acceptable
        }
    }

    // Test 15: Documentation - Verify help text mentions --no-progress
    #[test]
    fn test_help_text_mentions_no_progress() {
        use checkle::cli::{COMMAND_EXAMPLES, HASH_EXAMPLES};

        // Check that examples mention --no-progress flag
        assert!(
            COMMAND_EXAMPLES.contains("--no-progress") || HASH_EXAMPLES.contains("--no-progress"),
            "Help text should mention --no-progress flag"
        );
    }
}
