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
    let output = bin()
        .arg("--version")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    let expected = env!("CARGO_PKG_VERSION");
    assert!(
        text.contains(expected),
        "version output {:?} should contain {}",
        text,
        expected
    );
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

#[test]
fn test_interactive_requires_tty() {
    let dir = make_sandbox();
    bin()
        .args([
            "--path",
            &sandbox_path(&dir),
            "--action",
            "trash",
            "--interactive",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--interactive requires a TTY"));
}

#[test]
fn test_interactive_with_shell_output_rejected() {
    let dir = make_sandbox();
    bin()
        .args([
            "--path",
            &sandbox_path(&dir),
            "--output",
            "shell",
            "--action",
            "delete",
            "--interactive",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--interactive cannot be combined with --output shell"));
}

#[test]
fn test_report_keeps_all_files() {
    let dir = make_sandbox();
    let a_file = dir.path().join("a/file1.txt");
    let b_file = dir.path().join("b/file1.txt");

    assert!(a_file.exists(), "precondition: a/file1.txt exists");
    assert!(b_file.exists(), "precondition: b/file1.txt exists");

    bin()
        .args(["--path", &sandbox_path(&dir)])
        .assert()
        .success();

    assert!(a_file.exists(), "report must not delete the kept file");
    assert!(b_file.exists(), "report must not delete anything");
}

#[test]
fn test_dry_run_keeps_all_files() {
    let dir = make_sandbox();
    let a_file = dir.path().join("a/file1.txt");
    let b_file = dir.path().join("b/file1.txt");

    bin()
        .args(["--path", &sandbox_path(&dir), "--action", "trash"])
        .assert()
        .success();

    assert!(a_file.exists(), "dry-run must not delete the kept file");
    assert!(b_file.exists(), "dry-run must not delete anything");
}

#[test]
fn test_delete_keeps_keep_removes_del() {
    let dir = make_sandbox();
    let a_file = dir.path().join("a/file1.txt");
    let b_file = dir.path().join("b/file1.txt");

    bin()
        .args(["--path", &sandbox_path(&dir), "--action", "delete", "--yes"])
        .assert()
        .success();

    assert!(
        a_file.exists(),
        "KEEP file must survive --action delete --yes"
    );
    assert!(
        !b_file.exists(),
        "DEL file must be removed by --action delete --yes"
    );
}

#[test]
fn test_move_keeps_keep_moves_del() {
    let dir = make_sandbox();
    let a_file = dir.path().join("a/file1.txt");
    let b_file = dir.path().join("b/file1.txt");
    let out_dir = dir.path().join("moved");

    bin()
        .args([
            "--path",
            &sandbox_path(&dir),
            "--action",
            "move",
            "--action-dir",
            out_dir.to_str().unwrap(),
            "--yes",
        ])
        .assert()
        .success();

    assert!(
        a_file.exists(),
        "KEEP file must survive --action move --yes"
    );
    assert!(
        !b_file.exists(),
        "DEL file must be gone from its original location"
    );
    assert!(
        out_dir.join("file1.txt").exists(),
        "DEL file must appear in the action-dir"
    );
}

#[test]
fn test_untouched_file_survives_action() {
    let dir = make_sandbox();
    let a_unique = dir.path().join("a/unique.txt");
    let b_unique = dir.path().join("b/untouched.txt");

    fs::write(&b_unique, b"i am not a duplicate").unwrap();

    assert!(a_unique.exists());
    assert!(b_unique.exists());

    bin()
        .args(["--path", &sandbox_path(&dir), "--action", "delete", "--yes"])
        .assert()
        .success();

    assert!(
        a_unique.exists(),
        "file with no duplicates must survive"
    );
    assert!(
        b_unique.exists(),
        "file not in the duplicate list must survive"
    );
}

#[test]
fn test_from_report_trash_keeps_keep() {
    let dir = make_sandbox();
    let json_path = dir.path().join("report.json");
    let a_file = dir.path().join("a/file1.txt");
    let b_file = dir.path().join("b/file1.txt");

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
        .args([
            "--from-report",
            json_path.to_str().unwrap(),
            "--action",
            "delete",
            "--yes",
        ])
        .assert()
        .success();

    assert!(
        a_file.exists(),
        "KEEP file must survive --from-report action"
    );
    assert!(
        !b_file.exists(),
        "DEL file must be removed by --from-report action"
    );
}

#[test]
fn test_from_report_validates_changed_file_and_skips() {
    let dir = make_sandbox();
    let json_path = dir.path().join("report.json");
    let a_file = dir.path().join("a/file1.txt");
    let b_file = dir.path().join("b/file1.txt");

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

    fs::write(&b_file, b"MODIFIED CONTENT").unwrap();

    bin()
        .args([
            "--from-report",
            json_path.to_str().unwrap(),
            "--action",
            "delete",
            "--yes",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Failed:"));

    assert!(a_file.exists(), "KEEP file must survive");
    assert!(
        b_file.exists(),
        "changed DEL file must survive validation rejection"
    );
}

#[test]
fn test_min_size_with_human_readable_suffix() {
    let dir = make_sandbox();
    bin()
        .args(["--path", &sandbox_path(&dir), "--min-size", "1K"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[KEEP]"));

    bin()
        .args(["--path", &sandbox_path(&dir), "--min-size", "10M"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No duplicates found"));
}

#[test]
fn test_max_size_with_human_readable_suffix() {
    let dir = make_sandbox();

    bin()
        .args(["--path", &sandbox_path(&dir), "--max-size", "2M"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[KEEP]"));

    let output = bin()
        .args(["--path", &sandbox_path(&dir), "--max-size", "500K"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    assert!(
        text.contains("file1.txt"),
        "--max-size 500K should keep the small files"
    );
    assert!(
        !text.contains("big.bin") && !text.contains("big_copy.bin"),
        "--max-size 500K should exclude the 1 MB files"
    );
}

#[test]
fn test_byte_suffix_still_works() {
    let dir = make_sandbox();
    bin()
        .args(["--path", &sandbox_path(&dir), "--min-size", "1048576"])
        .assert()
        .success();
}

#[test]
fn test_sample_threshold_with_suffix() {
    let dir = make_sandbox();
    bin()
        .args([
            "--path",
            &sandbox_path(&dir),
            "--sample-chunk",
            "1M",
            "--sample-threshold",
            "100K",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Sampling:"));
}

#[test]
fn test_min_size_invalid_value() {
    let dir = make_sandbox();
    bin()
        .args(["--path", &sandbox_path(&dir), "--min-size", "garbage"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid size"));
}