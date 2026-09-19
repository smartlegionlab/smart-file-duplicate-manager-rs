# Smart File Duplicate Manager (Rust)

Fast and efficient duplicate file finder written in Rust.

Finds byte-identical files in a directory tree using a multi-stage algorithm
that avoids reading entire files unless necessary.

---

[![GitHub top language](https://img.shields.io/github/languages/top/smartlegionlab/smart-file-duplicate-manager-rs)](https://github.com/smartlegionlab/smart-file-duplicate-manager-rs)
[![GitHub license](https://img.shields.io/github/license/smartlegionlab/smart-file-duplicate-manager-rs)](https://github.com/smartlegionlab/smart-file-duplicate-manager-rs/blob/master/LICENSE)
[![GitHub release](https://img.shields.io/github/v/release/smartlegionlab/smart-file-duplicate-manager-rs)](https://github.com/smartlegionlab/smart-file-duplicate-manager-rs/)
[![GitHub stars](https://img.shields.io/github/stars/smartlegionlab/smart-file-duplicate-manager-rs?style=social)](https://github.com/smartlegionlab/smart-file-duplicate-manager-rs/stargazers)
[![GitHub forks](https://img.shields.io/github/forks/smartlegionlab/smart-file-duplicate-manager-rs?style=social)](https://github.com/smartlegionlab/smart-file-duplicate-manager-rs/network/members)

---

## Disclaimer

**By using this software, you agree to the full disclaimer terms.**

**Summary:** Software provided "AS IS" without warranty. You assume all risks.

**Full legal disclaimer:** See [DISCLAIMER.md](https://github.com/smartlegionlab/smart-file-duplicate-manager-rs/blob/master/DISCLAIMER.md)

---


## Features

- Recursive directory scanning
- Multi-stage duplicate detection: size → prefix hash → full hash → byte-by-byte confirmation
- BLAKE3 hashing with parallel processing (rayon)
- Four strategies for choosing which file to keep
- Five actions: report, trash, move, delete, hardlink
- Dry-run by default for destructive actions
- XDG-compliant trash (reversible)
- JSON output for scripting and integration
- Reusable JSON reports: apply actions later without rescanning
- Size and mtime validation when applying actions from a saved report
- Shell script output: generate a reviewable bash script instead of executing
- Optional sampling mode for large files (opt-in, off by default)
- Filters: extension, path exclusion, size range, hidden files
- Colored output (ANSI, no external color crates)
- Progress bars for long operations

## Algorithm

Duplicate detection runs in five phases:

1. **Scan** — walk the directory tree, collect files (path, size, mtime)
2. **Group by size** — files of different sizes cannot be duplicates
3. **Prefix hash** — hash the first 4 KB of each candidate (fast filter)
4. **Full hash** — BLAKE3 over full content, parallelized with rayon
5. **Confirm** — byte-by-byte comparison to rule out hash collisions

This way, full file contents are read only for files that pass all previous
filters — typically a small fraction of the total.

## Installation

Requires Rust 1.70 or newer.

```bash
git clone https://github.com/smartlegionlab/smart-file-duplicate-manager-rs
cd smart-file-duplicate-manager-rs
cargo build --release
```

Binary: `target/release/smart_file_duplicate_manager`

## Usage

```
smart_file_duplicate_manager --path <PATH> [OPTIONS]
smart_file_duplicate_manager --from-report <FILE> [OPTIONS]
```

### Basic

```bash
smart_file_duplicate_manager --path ~/Downloads
smart_file_duplicate_manager -p ~/Pictures
```

### Options

| Option                       | Description                                                                            | Default  |
|------------------------------|----------------------------------------------------------------------------------------|----------|
| `-p, --path <PATH>`          | Directory to scan (required unless `--from-report` is used)                            | —        |
| `--from-report <FILE>`       | Load duplicate groups from a JSON report instead of scanning                           | —        |
| `--min-size <BYTES>`         | Minimum file size                                                                      | `1`      |
| `--max-size <BYTES>`         | Maximum file size                                                                      | —        |
| `--sample-chunk <BYTES>`     | Read only 3 chunks (start/middle/end) of large files (0 = read fully)                  | `0`      |
| `--sample-threshold <BYTES>` | Apply sampling only to files at least this large                                       | `100 MB` |
| `--ext <LIST>`               | Only these extensions (comma-separated)                                                | —        |
| `--exclude <LIST>`           | Skip paths containing these substrings                                                 | —        |
| `--hidden`                   | Include hidden files                                                                   | `false`  |
| `--follow-links`             | Follow symbolic links                                                                  | `false`  |
| `--keep <STRATEGY>`          | Which file to keep: `first`, `newest`, `oldest`, `shortest`                            | `first`  |
| `--action <ACTION>`          | `report`, `trash`, `move`, `delete`, `hardlink`                                        | `report` |
| `--action-dir <DIR>`         | Destination for `move` action                                                          | —        |
| `--yes`                      | Execute destructive actions (without it: dry-run)                                      | `false`  |
| `--dry-run`                  | Force dry-run even with `--yes`                                                        | `false`  |
| `--limit <N>`                | Show only top N groups in report                                                       | —        |
| `--group-by-dir`             | Show directories with most duplicates                                                  | `false`  |
| `--output <FORMAT>`          | `text`, `json`, or `shell` (shell requires `--action delete\|move\|hardlink`)          | `text`   |
| `--output-file <PATH>`       | Write output to file                                                                   | —        |
| `--color <WHEN>`             | `auto`, `always`, `never`                                                              | `auto`   |

### Actions

- **report** — print groups of duplicates, delete nothing (default)
- **trash** — move duplicates to `~/.local/share/Trash` (reversible)
- **move** — move duplicates to `--action-dir`
- **delete** — permanently remove duplicates
- **hardlink** — replace duplicates with hard links to the kept file

Without `--yes`, every action except `report` runs in dry-run mode and prints
the plan without touching files.

### Examples

Report only (safe):

```bash
smart_file_duplicate_manager --path ~/Photos
```

Only images, keep the newest copy, show top 10 groups:

```bash
smart_file_duplicate_manager --path ~/Photos \
    --ext jpg,jpeg,png,heic \
    --keep newest \
    --limit 10
```

Find duplicates larger than 10 MB, exclude `.git` and `node_modules`:

```bash
smart_file_duplicate_manager --path ~/Projects \
    --min-size 10485760 \
    --exclude .git,node_modules,target
```

Preview trash operation (dry-run):

```bash
smart_file_duplicate_manager --path ~/Downloads --action trash
```

Execute trash operation:

```bash
smart_file_duplicate_manager --path ~/Downloads --action trash --yes
```

Export JSON report to file:

```bash
smart_file_duplicate_manager --path ~/Music --output json --output-file report.json
```

### Large file sampling (optional)

By default, every candidate file is read in full. For very large files on
slow storage this can take a long time.

The `--sample-chunk <BYTES>` flag enables a faster mode: files larger than
`--sample-threshold <BYTES>` (default 100 MB) are read in three chunks —
beginning, middle, and end — instead of the full file.

```
sampled range = [0, chunk) + [middle, middle + chunk) + [size - chunk, size)
middle = (size - chunk) / 2
```

**Trade-off:** this is a probabilistic check. Two files that differ only in
an unsampled region will be reported as duplicates. Use this flag only when
speed matters more than certainty.

For safe behaviour, leave `--sample-chunk` at its default (`0`) — files are
read in full and compared byte by byte.

Example — 4 MB chunks, threshold 100 MB (fast, not guaranteed):

```bash
smart_file_duplicate_manager --path /media/data \
    --sample-chunk 4194304 \
    --sample-threshold 104857600
```

The report includes a note about the active sampling mode:

```
[4/5] Full hash:   14 duplicate groups, 41 files (0.02s) (sampling: 4194304 B chunks for files >= 104857600 B)
```

### Reusing a saved report

Scanning large directory trees can take a long time. Save the report once,
inspect it, then apply actions later without rescanning.

```bash
# 1. Scan and save the report
smart_file_duplicate_manager --path /media/data \
    --min-size 104857600 \
    --output json --output-file /tmp/dupes.json

# 2. Inspect the plan from the saved report (no rescanning)
smart_file_duplicate_manager --from-report /tmp/dupes.json

# 3. Execute an action (still dry-run unless --yes is given)
smart_file_duplicate_manager --from-report /tmp/dupes.json --action trash --yes
```

When `--from-report` is used, each file is validated against its recorded
size and mtime before any action is applied. If a file changed since the scan,
it is skipped and reported as an error — nothing is silently modified.

`--path` and `--from-report` are mutually exclusive.

### Shell script output

Instead of executing a destructive action directly, the tool can produce a
bash script that performs the action. Review it, then run it manually.

```bash
# Generate a delete script (does NOT execute it)
smart_file_duplicate_manager --path /media/data \
    --action delete --output shell --output-file /tmp/cleanup.sh

less /tmp/cleanup.sh     # inspect
bash -n /tmp/cleanup.sh  # syntax check
bash /tmp/cleanup.sh     # execute when ready
```

Supported actions:

| Action     | Generated command                                              |
|------------|----------------------------------------------------------------|
| `delete`   | `rm -- <path>`                                                 |
| `move`     | `mv -- <src> <--action-dir>/<name>`                            |
| `hardlink` | `ln -f -- <keep> <tmp> && rm -- <del> && mv -- <tmp> <del>`    |

**Not supported:**

- `--action report` — nothing to write (use `--output text`).
- `--action trash` — the XDG trash requires `.trashinfo` metadata; use
  `--action trash` directly instead.

The script is never executed by the tool. It is plain text you can inspect
and modify.

Example output for `--action delete`:

```bash
#!/bin/bash
set -euo pipefail

# Generated by Smart File Duplicate Manager
# Keep strategy: First
# Action: Delete
# Root: /media/data/
# Groups: 14
# Files to process: 41
# Space to free: 20.50 GB
#
# Review carefully before running.

# Group #1: 500.00 MB each, 500.00 MB wasted
# KEEP: /media/data/a.mp4
rm -- /media/data/b.mp4
...
```

### JSON output

```json
{
  "path": "/home/user/Downloads",
  "keep_strategy": "First",
  "action": "Report",
  "groups": [
    {
      "index": 1,
      "size": 5242880,
      "wasted": 5242880,
      "keep": {
        "path": "/home/user/Downloads/file.bin",
        "size": 5242880,
        "mtime": 1700000000
      },
      "delete": [
        {
          "path": "/home/user/Downloads/file copy.bin",
          "size": 5242880,
          "mtime": 1700000000
        }
      ]
    }
  ],
  "total_groups": 1,
  "total_files": 2,
  "total_wasted": 5242880
}
```

## Exit codes

- `0` — success
- `1` — invalid arguments, path does not exist, or report cannot be loaded

## Developer Guide

### Build

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

### Sandbox — basic (small files)

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

### Sandbox — sampling (large files)

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

### Regression checklist

Before each commit, ensure:

1. `cargo build --release` — no warnings.
2. Default scan (`--path` only) — same output as before.
3. `--output json` — valid JSON, no trailing text.
4. `--output json --output-file` — valid JSON in file.
5. `--output shell` — generates a script; `bash -n` passes.
6. `--output shell` without `--action delete|move|hardlink` — error.
7. `--output shell --action trash` — error.
8. `--action trash` without `--yes` — dry-run, files untouched.
9. `--action trash --yes` on sandbox — files moved to trash, kept file intact.
10. `--from-report` — loads saved report, validates size and mtime.
11. Sampling flag (`--sample-chunk > 0`) — off by default, opt-in only.

## License

BSD 3-Clause License. See [LICENSE](LICENSE).

