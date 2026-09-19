# Smart File Duplicate Manager

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
```

### Basic

```bash
smart_file_duplicate_manager --path ~/Downloads
smart_file_duplicate_manager -p ~/Pictures
```

### Options

| Option                 | Description                                                 | Default  |
|------------------------|-------------------------------------------------------------|----------|
| `-p, --path <PATH>`    | Directory to scan (required)                                | —        |
| `--min-size <BYTES>`   | Minimum file size                                           | `1`      |
| `--max-size <BYTES>`   | Maximum file size                                           | —        |
| `--ext <LIST>`         | Only these extensions (comma-separated)                     | —        |
| `--exclude <LIST>`     | Skip paths containing these substrings                      | —        |
| `--hidden`             | Include hidden files                                        | `false`  |
| `--follow-links`       | Follow symbolic links                                       | `false`  |
| `--keep <STRATEGY>`    | Which file to keep: `first`, `newest`, `oldest`, `shortest` | `first`  |
| `--action <ACTION>`    | `report`, `trash`, `move`, `delete`, `hardlink`             | `report` |
| `--action-dir <DIR>`   | Destination for `move` action                               | —        |
| `--yes`                | Execute destructive actions (without it: dry-run)           | `false`  |
| `--dry-run`            | Force dry-run even with `--yes`                             | `false`  |
| `--limit <N>`          | Show only top N groups in report                            | —        |
| `--group-by-dir`       | Show directories with most duplicates                       | `false`  |
| `--output <FORMAT>`    | `text` or `json`                                            | `text`   |
| `--output-file <PATH>` | Write output to file                                        | —        |
| `--color <WHEN>`       | `auto`, `always`, `never`                                   | `auto`   |

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
- `1` — invalid arguments or path does not exist

## License

BSD 3-Clause License. See [LICENSE](LICENSE).
