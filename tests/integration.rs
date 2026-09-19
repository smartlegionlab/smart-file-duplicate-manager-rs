use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn bin() -> Command {
    Command::cargo_bin("smart_file_duplicate_manager").unwrap()
}

fn make_sandbox() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("a")).unwrap();
    fs::create_dir_all(dir.path().join("b")).unwrap();
    fs::write(dir.path().join("a/file1.txt"), b"hello world").unwrap();
    fs::write(dir.path().join("b/file1.txt"), b"hello world").unwrap();
    fs::write(dir.path().join("a/unique.txt"), b"different").unwrap();
    fs::write(dir.path().join("a/big.bin"), vec![0u8; 1024 * 1024]).unwrap();
    fs::write(dir.path().join("b/big_copy.bin"), vec![0u8; 1024 * 1024]).unwrap();
    dir
}

fn sandbox_path(dir: &TempDir) -> String {
    format!("{}/", dir.path().display())
}

#[test]
fn test_help_exit_zero() {
    bin()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Smart File Duplicate Manager"))
        .stdout(predicate::str::contains("--path"))
        .stdout(predicate::str::contains("--from-report"))
        .stdout(predicate::str::contains("Repository:"));
}

#[test]
fn test_version() {
    bin()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("1.0.0"));
}

#[test]
fn test_basic_scan() {
    let dir = make_sandbox();
    bin()
        .args(["--path", &sandbox_path(&dir)])
        .assert()
        .success()
        .stdout(predicate::str::contains("Group #1"))
        .stdout(predicate::str::contains("[KEEP]"))
        .stdout(predicate::str::contains("[DEL]"))
        .stdout(predicate::str::contains("Nothing was deleted"));
}

#[test]
fn test_json_stdout_valid() {
    let dir = make_sandbox();
    let output = bin()
        .args(["--path", &sandbox_path(&dir), "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    assert!(parsed["groups"].is_array());
    assert!(text.starts_with('{'));
}

#[test]
fn test_json_file_clean() {
    let dir = make_sandbox();
    let json_path = dir.path().join("report.json");
    bin()
        .args([
            "--path",
            &sandbox_path(&dir),
            "--output",
            "json",
            "--output-file",
            json_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let content = fs::read_to_string(&json_path).unwrap();
    assert!(content.starts_with('{'), "JSON file must start with {{");
    let parsed: serde_json::Value = serde_json::from_str(&content).expect("valid JSON file");
    assert!(parsed["groups"].is_array());
}

#[test]
fn test_from_report() {
    let dir = make_sandbox();
    let json_path = dir.path().join("report.json");
    bin()
        .args([
            "--path",
            &sandbox_path(&dir),
            "--output",
            "json",
            "--output-file",
            json_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    bin()
        .args(["--from-report", json_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Loaded"))
        .stdout(predicate::str::contains("[KEEP]"));
}

#[test]
fn test_dry_run_trash() {
    let dir = make_sandbox();
    let before_b = fs::read_dir(dir.path().join("b")).unwrap().count();
    bin()
        .args(["--path", &sandbox_path(&dir), "--action", "trash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("DRY RUN"))
        .stdout(predicate::str::contains("No changes were made"));
    let after_b = fs::read_dir(dir.path().join("b")).unwrap().count();
    assert_eq!(before_b, after_b, "dry-run must not touch files");
}

#[test]
fn test_argument_errors() {
    bin()
        .assert()
        .failure()
        .stderr(predicate::str::contains("--path or --from-report"));

    let dir = make_sandbox();
    bin()
        .args(["--path", "/tmp/does_not_exist_xyz_12345"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));

    bin()
        .args(["--path", &sandbox_path(&dir), "--action", "move"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--action-dir"));
}

#[test]
fn test_path_and_from_report_conflict() {
    let dir = make_sandbox();
    let json_path = dir.path().join("report.json");
    fs::write(&json_path, "{}").unwrap();
    bin()
        .args([
            "--path",
            &sandbox_path(&dir),
            "--from-report",
            json_path.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used together"));
}

#[test]
fn test_shell_requires_action() {
    let dir = make_sandbox();
    bin()
        .args(["--path", &sandbox_path(&dir), "--output", "shell"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires --action"));
}

#[test]
fn test_shell_rejects_trash() {
    let dir = make_sandbox();
    bin()
        .args([
            "--path",
            &sandbox_path(&dir),
            "--output",
            "shell",
            "--action",
            "trash",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not support the trash action"));
}

#[test]
fn test_shell_delete_script() {
    let dir = make_sandbox();
    let script_path = dir.path().join("del.sh");
    bin()
        .args([
            "--path",
            &sandbox_path(&dir),
            "--output",
            "shell",
            "--action",
            "delete",
            "--output-file",
            script_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let content = fs::read_to_string(&script_path).unwrap();
    assert!(content.starts_with("#!/bin/bash"));
    assert!(content.contains("set -euo pipefail"));
    assert!(content.contains("rm --"));
}

#[test]
fn test_filters_min_size() {
    let dir = make_sandbox();
    bin()
        .args(["--path", &sandbox_path(&dir), "--min-size", "5000000"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No duplicates found"));
}

#[test]
fn test_filters_ext() {
    let dir = make_sandbox();
    bin()
        .args(["--path", &sandbox_path(&dir), "--ext", "txt"])
        .assert()
        .success();
}

#[test]
fn test_keep_strategy_shows_in_report() {
    let dir = make_sandbox();
    bin()
        .args(["--path", &sandbox_path(&dir), "--keep", "newest"])
        .assert()
        .success()
        .stdout(predicate::str::contains("keep: Newest"));
}

#[test]
fn test_output_file_no_banner() {
    let dir = make_sandbox();
    let out_path = dir.path().join("out.txt");
    bin()
        .args([
            "--path",
            &sandbox_path(&dir),
            "--output-file",
            out_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let content = fs::read_to_string(&out_path).unwrap();
    assert!(
        !content.contains("Smart File Duplicate Manager v"),
        "banner must not be in the file"
    );
    assert!(
        !content.contains("Copyright (c)"),
        "footer must not be in the file"
    );
}