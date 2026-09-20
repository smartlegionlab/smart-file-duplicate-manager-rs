# Tests and Developer Guide

Development, testing, and acceptance scenarios for Smart File Duplicate Manager.

Full user documentation lives in [README.md](README.md).

---

## Build

```bash
cargo build --release
```

Binary: `target/release/smart_file_duplicate_manager`.

Warnings are not expected. If any appear, investigate before committing.

If a change does not appear in the binary, force a rebuild:

```bash
touch src/main.rs
cargo build --release

# Or, if still stale:
cargo clean && cargo build --release
```

## Automated tests

```bash
cargo test
```

Runs unit tests (in `src/main.rs`) and integration tests (in
`tests/integration.rs`). All tests run in isolated temporary directories and
never touch real files.

- **Unit tests** cover `format_size`, `shell_quote`, `pick_keeper`,
  `files_equal`, `validate_entry`, `load_groups_from_json`, `SampleConfig`,
  `is_leap_year`, `parse_confirm`.
- **Integration tests** exercise the compiled binary via `assert_cmd`:
  scanning, JSON output, JSON file cleanliness, `--from-report`, dry-run,
  argument errors, shell output, filters, keep strategies, interactive TTY
  requirement, `--interactive` + `--output shell` conflict, and file system
  state after each action (KEEP survives, DEL is removed or moved).

Dev-dependencies: `assert_cmd`, `predicates`, `tempfile`.

Before committing, ensure `cargo test` is fully green.

---

## Sandbox setup

### Basic sandbox (small files)

```bash
rm -rf /tmp/dupes_test
mkdir -p /tmp/dupes_test/a /tmp/dupes_test/b
echo "hello world" > /tmp/dupes_test/a/file1.txt
echo "hello world" > /tmp/dupes_test/b/file1.txt
echo "different" > /tmp/dupes_test/a/unique.txt
dd if=/dev/urandom of=/tmp/dupes_test/a/big.bin bs=1M count=3 2>/dev/null
cp /tmp/dupes_test/a/big.bin /tmp/dupes_test/b/big_copy.bin
rm -f /tmp/dupes_test.json
```

### Sampling sandbox (large files)

```bash
rm -rf /tmp/dupes_test
mkdir -p /tmp/dupes_test/a /tmp/dupes_test/b
dd if=/dev/urandom of=/tmp/dupes_test/a/big.bin bs=1M count=200 2>/dev/null
cp /tmp/dupes_test/a/big.bin /tmp/dupes_test/b/big_copy.bin
cp /tmp/dupes_test/a/big.bin /tmp/dupes_test/b/big_modified.bin

# Modify middle of the third file (keeps size identical)
dd if=/dev/zero of=/tmp/dupes_test/b/big_modified.bin bs=1 count=100 seek=104000000 conv=notrunc 2>/dev/null
rm -f /tmp/dupes_test.json
```

---

## Manual tests

### Test — scan pipeline

```bash
# Default (full read, safest)
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/

# Filters
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --min-size 1000
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --max-size 5000000
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --ext txt
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --exclude b
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --hidden

# Keep strategies
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --keep first
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --keep newest
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --keep oldest
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --keep shortest

# Output
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --limit 5
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --group-by-dir
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --color never
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --output json
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --output json --output-file /tmp/dupes_test.json

# Validate JSON
python3 -c "import json; d=json.load(open('/tmp/dupes_test.json')); print('valid JSON, groups:', len(d['groups']))"
```

### Test — sampling correctness

```bash
# Difference at 1 MB (inside first chunk) → must be 0 groups
rm -rf /tmp/dupes_test
mkdir -p /tmp/dupes_test/a /tmp/dupes_test/b
dd if=/dev/urandom of=/tmp/dupes_test/a/big.bin bs=1M count=200 2>/dev/null
cp /tmp/dupes_test/a/big.bin /tmp/dupes_test/b/big_modified.bin
dd if=/dev/zero of=/tmp/dupes_test/b/big_modified.bin bs=1 count=100 seek=1000000 conv=notrunc 2>/dev/null
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ \
    --sample-chunk 4194304 --sample-threshold 104857600

# Difference at 104 MB (inside middle chunk) → must be 0 groups
rm -rf /tmp/dupes_test
mkdir -p /tmp/dupes_test/a /tmp/dupes_test/b
dd if=/dev/urandom of=/tmp/dupes_test/a/big.bin bs=1M count=200 2>/dev/null
cp /tmp/dupes_test/a/big.bin /tmp/dupes_test/b/big_modified.bin
dd if=/dev/zero of=/tmp/dupes_test/b/big_modified.bin bs=1 count=100 seek=104000000 conv=notrunc 2>/dev/null
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ \
    --sample-chunk 4194304 --sample-threshold 104857600

# Difference at 208 MB (inside last chunk) → must be 0 groups
rm -rf /tmp/dupes_test
mkdir -p /tmp/dupes_test/a /tmp/dupes_test/b
dd if=/dev/urandom of=/tmp/dupes_test/a/big.bin bs=1M count=200 2>/dev/null
cp /tmp/dupes_test/a/big.bin /tmp/dupes_test/b/big_modified.bin
dd if=/dev/zero of=/tmp/dupes_test/b/big_modified.bin bs=1 count=100 seek=208000000 conv=notrunc 2>/dev/null
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ \
    --sample-chunk 4194304 --sample-threshold 104857600

# Identical files → must be 1 group
rm -rf /tmp/dupes_test
mkdir -p /tmp/dupes_test/a /tmp/dupes_test/b
dd if=/dev/urandom of=/tmp/dupes_test/a/big.bin bs=1M count=200 2>/dev/null
cp /tmp/dupes_test/a/big.bin /tmp/dupes_test/b/big_copy.bin
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ \
    --sample-chunk 4194304 --sample-threshold 104857600
```

### Test — actions (safe, on sandbox)

```bash
# Dry-run (no --yes): nothing is changed
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --action trash
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --action delete
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --action move --action-dir /tmp/dupes_out
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --action hardlink

# Real trash (reversible)
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --action trash --yes
ls -la ~/.local/share/Trash/files/ | tail

# Real move
mkdir -p /tmp/dupes_out
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --action move --action-dir /tmp/dupes_out --yes
ls -la /tmp/dupes_out/

# Real hardlink (same filesystem only)
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --action hardlink --yes
ls -li /tmp/dupes_test/a/big.bin /tmp/dupes_test/b/big_copy.bin

# Real delete (irreversible)
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --action delete --yes
```

### Test — interactive mode

```bash
# Basic interactive (must be run in a real terminal, not a pipe)
./target/release/smart_file_duplicate_manager \
    --path /tmp/dupes_test/ --action trash --interactive

# Interactive with a saved report
./target/release/smart_file_duplicate_manager \
    --path /tmp/dupes_test/ --output json --output-file /tmp/dupes_test.json

./target/release/smart_file_duplicate_manager \
    --from-report /tmp/dupes_test.json --action trash --interactive

# Must fail when stdin is not a TTY (in tests, pipes, scripts)
echo | ./target/release/smart_file_duplicate_manager \
    --path /tmp/dupes_test/ --action trash --interactive

# Must fail when combined with shell output
./target/release/smart_file_duplicate_manager \
    --path /tmp/dupes_test/ --output shell --action delete --interactive
```

### Test — shell output

```bash
# Argument errors
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --output shell
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --output shell --action trash
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --output shell --action move

# Generate a delete script and inspect it
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ \
    --output shell --action delete

# Write to file and syntax-check
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ \
    --output shell --action delete --output-file /tmp/dupes_delete.sh
bash -n /tmp/dupes_delete.sh && echo "syntax OK"

# Execute the generated script (on a sandbox copy)
cp -r /tmp/dupes_test /tmp/dupes_test_copy
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test_copy/ \
    --output shell --action delete --output-file /tmp/dupes_delete.sh
bash /tmp/dupes_delete.sh
ls -la /tmp/dupes_test_copy/a /tmp/dupes_test_copy/b

# Move script
mkdir -p /tmp/dupes_out
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ \
    --output shell --action move --action-dir /tmp/dupes_out

# Hardlink script
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ \
    --output shell --action hardlink
```

### Test — reuse saved report

```bash
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ \
    --output json --output-file /tmp/dupes_test.json

./target/release/smart_file_duplicate_manager --from-report /tmp/dupes_test.json

./target/release/smart_file_duplicate_manager --from-report /tmp/dupes_test.json \
    --action trash --yes
```

### Test — validation of changed files

```bash
rm -rf /tmp/dupes_test
mkdir -p /tmp/dupes_test/a /tmp/dupes_test/b
echo "hello" > /tmp/dupes_test/a/file1.txt
echo "hello" > /tmp/dupes_test/b/file1.txt
rm -f /tmp/dupes_test.json

./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ \
    --output json --output-file /tmp/dupes_test.json

# Modify the file that was going to be deleted
echo "MODIFIED" > /tmp/dupes_test/b/file1.txt

# Validation must reject the changed file
./target/release/smart_file_duplicate_manager --from-report /tmp/dupes_test.json \
    --action trash --yes
```

Expected: `Failed: 1`, error `size changed: expected 6, got 9`. File is not
moved to trash.

### Test — argument errors

```bash
# Both --path and --from-report
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --from-report /tmp/dupes_test.json

# Neither --path nor --from-report
./target/release/smart_file_duplicate_manager

# --action move without --action-dir
./target/release/smart_file_duplicate_manager --path /tmp/dupes_test/ --action move

# Nonexistent path
./target/release/smart_file_duplicate_manager --path /tmp/does_not_exist_xyz
```

### Test — help and version

```bash
./target/release/smart_file_duplicate_manager --help
./target/release/smart_file_duplicate_manager -h
./target/release/smart_file_duplicate_manager --version
```

---

## Regression checklist

Before each commit, ensure:

1. `cargo build --release` — no warnings.
2. `cargo test` — all tests pass.
3. Default scan (`--path` only) — same output as before.
4. `--output json` — valid JSON, no trailing text.
5. `--output json --output-file` — valid JSON in file, no banner or footer.
6. `--output shell` — generates a script; `bash -n` passes.
7. `--output shell` without `--action delete|move|hardlink` — error.
8. `--output shell --action trash` — error.
9. `--action trash` without `--yes` — dry-run, files untouched.
10. `--action trash --yes` on sandbox — files moved to trash, kept file intact.
11. `--from-report` — loads saved report, validates size and mtime.
12. Sampling flag (`--sample-chunk > 0`) — off by default, opt-in only.
13. Phase logs and progress bars visible in terminal even with `--output-file`.
14. `--interactive` without TTY — error.
15. `--interactive` + `--output shell` — error.
16. After any action, KEEP files survive, only DEL files are affected.

