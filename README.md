# Smart File Duplicate Manager

**A safe, fast, and controllable duplicate file manager for the command line.**

Not just a duplicate finder. It is a complete workflow for locating, reviewing,
and cleaning duplicate files — with byte-level accuracy, reversible actions,
multi-stage detection, and full user control at every step.

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

## Why this tool

Most duplicate finders give you a list and expect you to figure out the rest.
This one is built around **control** and **safety**:

- **Nothing is deleted by default.** Every destructive action is a dry-run
  unless you pass `--yes` or `--interactive`.
- **Byte-level accuracy.** Duplicates are confirmed by a byte-by-byte
  comparison, not just a hash.
- **Reversible by default.** The `trash` action moves files to the XDG trash
  (`~/.local/share/Trash`) — you can restore them at any time.
- **Per-file confirmation.** `--interactive` prompts before each deletion.
- **Reusable reports.** Scan once, inspect, act later — without rescanning.
- **Reviewable scripts.** `--output shell` produces a bash script you can read
  and edit before running.
- **Fast on large files.** An optional sampling mode reads only 3 chunks per
  file (start, middle, end) for 10–100× speedups on multi-gigabyte files.

## Features

### Detection

- Multi-stage pipeline: size → prefix hash (4 KB) → full BLAKE3 hash → byte-by-byte confirmation
- Reads full file contents only for real candidates, not for every file
- Parallel hashing via rayon
- Optional sampling mode for very large files (opt-in, off by default)
- Symlink-aware (`--follow-links`)

### Control

- Dry-run by default for every destructive action
- Reversible `trash` action (XDG-compliant, restore with file manager)
- Interactive mode: confirm each file individually
- Validation of size and mtime before each action (in `--from-report` mode)
- Never touches anything without an explicit flag

### Actions

- `report` — show groups, delete nothing (default)
- `trash` — move duplicates to `~/.local/share/Trash` (reversible)
- `move` — move duplicates to a chosen folder
- `delete` — permanently remove duplicates
- `hardlink` — replace duplicates with hard links to the kept file

### Strategies

- Keep `first` (alphabetical), `newest`, `oldest`, or `shortest` path

### Filters

- Minimum and maximum file size
- Extension allow-list
- Path exclusion (substring match, prunes subtrees)
- Hidden files (excluded by default)

### Output and integration

- Plain text with ANSI colors (auto-detected, no extra dependencies)
- JSON output for scripts and pipelines
- Shell script output for manual review
- Reusable reports: `--from-report <FILE>` skips scanning
- Progress bars for long-running phases
- Quiet mode when piping JSON

### Quality

- 55 unit and integration tests
- No `unsafe`
- No warnings on `cargo build --release`
- Tests run in isolated temporary directories

## Algorithm

Duplicate detection runs in five phases. Each phase filters the candidate set
further, so that expensive operations run only on files that survived all
previous checks.

1. **Scan** — walk the directory tree, collect files (path, size, mtime)
2. **Group by size** — files of different sizes cannot be duplicates
3. **Prefix hash** — hash the first 4 KB of each candidate (fast filter)
4. **Full hash** — BLAKE3 over full content, parallelized with rayon
5. **Confirm** — byte-by-byte comparison to rule out hash collisions

With the optional `--sample-chunk` flag, phases 4 and 5 read only three
chunks per file (start, middle, end) instead of the whole file. This is a
probabilistic check — see the sampling section below.

## Installation

Requires Rust 1.70 or newer.

```bash
git clone https://github.com/smartlegionlab/smart-file-duplicate-manager-rs
cd smart-file-duplicate-manager-rs
cargo build --release
```

Binary: `target/release/smart_file_duplicate_manager`

### Install for system-wide use (Linux)

After building, you can call the tool from any directory — no path needed.

```bash
mkdir -p ~/.local/bin
ln -sf "$PWD/target/release/smart_file_duplicate_manager" ~/.local/bin/sfdm
```

Make sure `~/.local/bin` is in your `PATH`:

```bash
echo $PATH | tr ':' '\n' | grep -q "$HOME/.local/bin" && echo "OK" || echo "NOT IN PATH"
```

If it prints `NOT IN PATH`, add this line to `~/.bashrc` (or `~/.zshrc`):

```bash
export PATH="$HOME/.local/bin:$PATH"
```

Then reload the shell config:

```bash
source ~/.bashrc
```

Verify the installation:

```bash
which sfdm
sfdm --version
```

Now you can run it from anywhere:

```bash
sfdm --path ~/Downloads
sfdm --path /media/data --min-size 1073741824
sfdm --help
```

**Why a symlink?** After every `cargo build --release`, `sfdm` already points
to the fresh binary. No reinstall needed.

**Remove at any time:**

```bash
rm ~/.local/bin/sfdm
```

No `sudo`, no system directories touched.

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
| `--interactive`              | Ask confirmation for each file (requires a TTY)                                        | `false`  |
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

Without `--yes` (and without `--interactive`), every action except `report`
runs in dry-run mode and prints the plan without touching files.

### Interactive mode

Instead of `--yes` (which confirms everything at once), use `--interactive` to
confirm each file individually:

```bash
smart_file_duplicate_manager --path ~/Downloads --action trash --interactive
```

For each file you will be prompted:

```
[1/13] Trash /path/to/file.mp4? [y/N/a/q]
```

- `y` — do it.
- `n` (or Enter) — skip this file.
- `a` — apply to all remaining files without asking.
- `q` — quit.

Rules:

- Requires a TTY (stdin must be a terminal). Cannot be used in scripts or pipes.
- Cannot be combined with `--output shell` (the script is already reviewable).
- Overrides `--yes` when both are given (interactive is stricter).
- Dry-run overrides `--interactive` (no questions asked).
- If a file changed since the scan (`--from-report`), validation rejects it
  before the prompt — such files are reported as errors.

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

Execute trash operation with per-file confirmation:

```bash
smart_file_duplicate_manager --path ~/Downloads --action trash --interactive
```

Export JSON report to file:

```bash
smart_file_duplicate_manager --path ~/Music --output json --output-file report.json
```

## Command cheat sheet

Quick reference for common tasks. Copy-paste and adapt the paths.

### Scan and report

```bash
# Report duplicates in a directory (safe, nothing is deleted)
smart_file_duplicate_manager --path ~/Downloads

# Report duplicates in current directory
smart_file_duplicate_manager --path .

# Only files larger than 100 MB
smart_file_duplicate_manager --path ~/Videos --min-size 104857600

# Only files between 10 MB and 1 GB
smart_file_duplicate_manager --path ~/Videos --min-size 10485760 --max-size 1073741824

# Only images
smart_file_duplicate_manager --path ~/Photos --ext jpg,jpeg,png,heic

# Only .zip archives
smart_file_duplicate_manager --path ~/Downloads --ext zip

# Exclude .git and node_modules
smart_file_duplicate_manager --path ~/Projects --exclude .git,node_modules,target

# Include hidden files
smart_file_duplicate_manager --path ~/Documents --hidden

# Show top 10 groups only
smart_file_duplicate_manager --path ~/Music --limit 10

# Show directories with the most duplicates
smart_file_duplicate_manager --path ~/Videos --group-by-dir

# No color (for logs or files)
smart_file_duplicate_manager --path ~/Downloads --color never
```

### Keep strategies

```bash
# Keep the first by path (alphabetical)
smart_file_duplicate_manager --path ~/Photos --keep first

# Keep the most recently modified
smart_file_duplicate_manager --path ~/Photos --keep newest

# Keep the oldest by modification time
smart_file_duplicate_manager --path ~/Photos --keep oldest

# Keep the file with the shortest path
smart_file_duplicate_manager --path ~/Photos --keep shortest
```

### Output formats

```bash
# Text (default)
smart_file_duplicate_manager --path ~/Downloads

# JSON to stdout
smart_file_duplicate_manager --path ~/Downloads --output json

# JSON to file
smart_file_duplicate_manager --path ~/Downloads --output json --output-file /tmp/dupes.json

# Shell script for review (requires --action delete|move|hardlink)
smart_file_duplicate_manager --path ~/Downloads --action delete --output shell

# Shell script to file
smart_file_duplicate_manager --path ~/Downloads --action delete --output shell --output-file /tmp/cleanup.sh
```

### Actions (dry-run by default)

```bash
# Dry-run (no changes, just the plan)
smart_file_duplicate_manager --path ~/Downloads --action trash
smart_file_duplicate_manager --path ~/Downloads --action delete
smart_file_duplicate_manager --path ~/Downloads --action move --action-dir /tmp/dupes_out
smart_file_duplicate_manager --path ~/Downloads --action hardlink

# Move duplicates to ~/.local/share/Trash (reversible)
smart_file_duplicate_manager --path ~/Downloads --action trash --yes

# Move duplicates to a custom folder
smart_file_duplicate_manager --path ~/Downloads --action move --action-dir /tmp/dupes_out --yes

# Replace duplicates with hard links to the kept file (same filesystem only)
smart_file_duplicate_manager --path ~/Downloads --action hardlink --yes

# Permanently delete duplicates (irreversible)
smart_file_duplicate_manager --path ~/Downloads --action delete --yes
```

### Interactive mode

```bash
# Confirm each file individually (requires a TTY)
smart_file_duplicate_manager --path ~/Downloads --action trash --interactive

# Interactive with a saved report (no rescanning)
smart_file_duplicate_manager --from-report /tmp/dupes.json --action trash --interactive

# Interactive move
smart_file_duplicate_manager --path ~/Downloads \
    --action move --action-dir /tmp/dupes_review --interactive
```

Prompt keys:

- `y` — do it
- `n` or Enter — skip
- `a` — apply to all remaining
- `q` — quit

### Fast mode for large files

```bash
# Sample 3 chunks per large file (fast, not guaranteed)
smart_file_duplicate_manager --path /media/data --sample-chunk 4194304

# Sample only files >= 500 MB, with 4 MB chunks
smart_file_duplicate_manager --path /media/data \
    --min-size 524288000 \
    --sample-chunk 4194304 \
    --sample-threshold 524288000
```

### Reuse a saved report (no rescanning)

```bash
# Step 1: scan once and save
smart_file_duplicate_manager --path /media/data \
    --min-size 104857600 \
    --output json --output-file /tmp/dupes.json

# Step 2: inspect the plan (instant)
smart_file_duplicate_manager --from-report /tmp/dupes.json

# Step 3: apply an action (still dry-run unless --yes is given)
smart_file_duplicate_manager --from-report /tmp/dupes.json --action trash

# Step 4: execute
smart_file_duplicate_manager --from-report /tmp/dupes.json --action trash --yes

# Step 4 alt: execute with per-file confirmation
smart_file_duplicate_manager --from-report /tmp/dupes.json \
    --action trash --interactive
```

### Common real-world examples

```bash
# Clean duplicates in Downloads (report only)
smart_file_duplicate_manager --path ~/Downloads

# Find duplicate movies larger than 1 GB, preview trash plan
smart_file_duplicate_manager --path ~/Videos --min-size 1073741824 --action trash

# Find duplicate .zip archives larger than 300 MB
smart_file_duplicate_manager --path /media/data --min-size 314572800 --ext zip

# Find duplicate .iso images larger than 1 GB, save report
smart_file_duplicate_manager --path /media/data \
    --min-size 1073741824 --ext iso \
    --output json --output-file /tmp/iso_dupes.json

# Fast scan of a huge disk for large-file duplicates
smart_file_duplicate_manager --path /media/data \
    --min-size 524288000 \
    --sample-chunk 4194304 \
    --group-by-dir

# Scan music library, keep the newest copy of each track
smart_file_duplicate_manager --path ~/Music --ext mp3,flac,ogg --keep newest

# Scan photo library, exclude RAW cache folders
smart_file_duplicate_manager --path ~/Photos --ext jpg,jpeg,png --exclude .cache

# Safe cleanup: move duplicates to a review folder, then decide manually
mkdir -p /tmp/dupes_review
smart_file_duplicate_manager --path ~/Downloads \
    --action move --action-dir /tmp/dupes_review --yes
```

### Help and version

```bash
# Full help
smart_file_duplicate_manager --help

# Short help
smart_file_duplicate_manager -h

# Version
smart_file_duplicate_manager --version
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

When `--output json` is combined with `--output-file`, the file contains
pure JSON. Progress bars, phase logs, and informational messages go to
stderr and remain visible in the terminal.

### Output file cleanliness

When `--output-file <PATH>` is used:

- The file contains only the report (text, JSON, or shell script).
- The program name banner and copyright footer are **not** written to the file.
- Progress and phase logs go to the terminal (stderr), not to the file.
- For `--output json`, the file starts with `{` and is valid JSON.

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

### Tests

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
  state after each action (KEEP survives, DEL is removed/moved).

Dev-dependencies: `assert_cmd`, `predicates`, `tempfile`.

Before committing, ensure `cargo test` is fully green.

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

### Regression checklist

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

## License

Author: [Alexander Suvorov](https://smartlegionlab.github.io)

BSD 3-Clause License. See [LICENSE](LICENSE).

