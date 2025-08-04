#![warn(
    clippy::all,
    clippy::pedantic,
    clippy::perf,
    clippy::style,
    clippy::complexity,
    clippy::correctness,
    clippy::unwrap_used
)]

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_completions_command_exists() {
    // Test that the completions command is available
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("completions")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Generate shell completion scripts",
        ));
}

#[test]
fn test_completions_requires_shell_argument() {
    // Test that the completions command requires a shell argument
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("completions")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "error: the following required arguments were not provided",
        ));
}

#[test]
fn test_completions_bash() {
    // Test generating bash completions
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("completions")
        .arg("bash")
        .assert()
        .success()
        .stdout(predicate::str::contains("_checkle()"))
        .stdout(predicate::str::contains("COMPREPLY"));
}

#[test]
fn test_completions_zsh() {
    // Test generating zsh completions
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("completions")
        .arg("zsh")
        .assert()
        .success()
        .stdout(predicate::str::contains("#compdef checkle"))
        .stdout(predicate::str::contains("_checkle()"));
}

#[test]
fn test_completions_fish() {
    // Test generating fish completions
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("completions")
        .arg("fish")
        .assert()
        .success()
        .stdout(predicate::str::contains("complete -c checkle"));
}

#[test]
fn test_completions_powershell() {
    // Test generating PowerShell completions
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("completions")
        .arg("powershell")
        .assert()
        .success()
        .stdout(predicate::str::contains("Register-ArgumentCompleter"));
}

#[test]
fn test_completions_elvish() {
    // Test generating elvish completions
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("completions")
        .arg("elvish")
        .assert()
        .success()
        .stdout(predicate::str::contains("use builtin;"))
        .stdout(predicate::str::contains(
            "edit:completion:arg-completer[checkle]",
        ));
}

#[test]
fn test_completions_invalid_shell() {
    // Test that invalid shell names are rejected
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("completions")
        .arg("invalid-shell")
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value"));
}

#[test]
fn test_completions_aliases() {
    // Test that completion and comp aliases work
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("completion") // Using alias instead of full command
        .arg("bash")
        .assert()
        .success()
        .stdout(predicate::str::contains("_checkle()"));

    // Test 'comp' alias
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("comp") // Using shorter alias
        .arg("bash")
        .assert()
        .success()
        .stdout(predicate::str::contains("_checkle()"));
}

#[test]
fn test_completions_contains_all_commands() {
    // Test that generated completions include all main commands
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("completions")
        .arg("bash")
        .assert()
        .success()
        .stdout(predicate::str::contains("hash"))
        .stdout(predicate::str::contains("verify"))
        .stdout(predicate::str::contains("verify-many"))
        .stdout(predicate::str::contains("completions"));
}

#[test]
fn test_completions_includes_global_options() {
    // Test that generated completions include global options
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("completions")
        .arg("bash")
        .assert()
        .success()
        .stdout(predicate::str::contains("--algorithm"))
        .stdout(predicate::str::contains("--threads"))
        .stdout(predicate::str::contains("--chunk-size-kb"))
        .stdout(predicate::str::contains("--parallel-readers"));
}
