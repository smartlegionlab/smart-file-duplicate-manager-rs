use blake3::Hasher;
use clap::{Parser, ValueEnum};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

const APP_NAME: &str = "Smart File Duplicate Manager";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const AUTHOR: &str = "Alexander Suvorov";
const GITHUB: &str = "smartlegionlab";
const REPO_URL: &str = "https://github.com/smartlegionlab/smart-file-duplicate-manager-rs";

const PREFIX_BYTES: u64 = 4096;
const DEFAULT_SAMPLE_THRESHOLD: u64 = 100 * 1024 * 1024;

#[derive(Parser, Debug)]
#[command(
    name = APP_NAME,
    version = VERSION,
    author = AUTHOR,
    about = "Fast and efficient duplicate file finder written in Rust.",
    long_about = "Smart File Duplicate Manager finds byte-identical files in a directory tree. It uses a multi-stage algorithm: group by size, prefix hash (4 KB), full BLAKE3 hash, then byte-by-byte confirmation. Full file contents are read only for real candidates.",
    after_help = "Repository: https://github.com/smartlegionlab/smart-file-duplicate-manager-rs"
)]
struct Cli {
    #[arg(short = 'p', long = "path", value_name = "PATH", help = "Directory to scan (required unless --from-report is used)")]
    path: Option<PathBuf>,

    #[arg(long = "from-report", value_name = "FILE", help = "Load duplicate groups from a JSON report instead of scanning")]
    from_report: Option<PathBuf>,

    #[arg(long = "min-size", value_name = "BYTES", default_value_t = 1, help = "Minimum file size in bytes")]
    min_size: u64,

    #[arg(long = "max-size", value_name = "BYTES", help = "Maximum file size in bytes")]
    max_size: Option<u64>,

    #[arg(long = "sample-chunk", value_name = "BYTES", default_value_t = 0, help = "Sample large files: read only 3 chunks (start/middle/end) of this size instead of the full file (0 = read fully). Note: may miss differences outside sampled regions")]
    sample_chunk: u64,

    #[arg(long = "sample-threshold", value_name = "BYTES", default_value_t = DEFAULT_SAMPLE_THRESHOLD, help = "Apply sampling only to files at least this large (bytes)")]
    sample_threshold: u64,

    #[arg(long = "follow-links", help = "Follow symbolic links during scan")]
    follow_links: bool,

    #[arg(long = "hidden", help = "Include hidden files and directories")]
    hidden: bool,

    #[arg(long = "ext", value_name = "LIST", value_delimiter = ',', help = "Only these file extensions (comma-separated)")]
    ext: Vec<String>,

    #[arg(long = "exclude", value_name = "LIST", value_delimiter = ',', help = "Skip paths containing these substrings (comma-separated)")]
    exclude: Vec<String>,

    #[arg(long = "keep", value_enum, default_value_t = KeepStrategy::First, help = "Which file to keep in each duplicate group")]
    keep: KeepStrategy,

    #[arg(long = "action", value_enum, default_value_t = Action::Report, help = "What to do with duplicates")]
    action: Action,

    #[arg(long = "action-dir", value_name = "DIR", help = "Destination directory for the move action")]
    action_dir: Option<PathBuf>,

    #[arg(long = "yes", help = "Execute destructive actions (otherwise dry-run)")]
    yes: bool,

    #[arg(long = "dry-run", help = "Force dry-run even with --yes")]
    dry_run: bool,

    #[arg(long = "limit", value_name = "N", help = "Show only top N groups in the report")]
    limit: Option<usize>,

    #[arg(long = "group-by-dir", help = "Show directories with the most duplicates")]
    group_by_dir: bool,

    #[arg(long = "output", value_enum, default_value_t = OutputFormat::Text, help = "Output format: text, json, or shell script (requires --action delete|move|hardlink)")]
    output: OutputFormat,

    #[arg(long = "output-file", value_name = "PATH", help = "Write output to a file instead of stdout")]
    output_file: Option<PathBuf>,

    #[arg(long = "color", value_enum, default_value_t = ColorMode::Auto, help = "Colorize output")]
    color: ColorMode,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum KeepStrategy {
    First,
    Newest,
    Oldest,
    Shortest,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Report,
    Trash,
    Move,
    Delete,
    Hardlink,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum OutputFormat {
    Text,
    Json,
    Shell,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum ColorMode {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone)]
struct FileEntry {
    path: PathBuf,
    size: u64,
    mtime: u64,
}

#[derive(Debug, Default)]
struct ActionSummary {
    processed: u64,
    failed: u64,
    bytes_freed: u64,
    errors: Vec<String>,
}

#[derive(Debug, Default)]
struct ScanFilter {
    min_size: u64,
    max_size: Option<u64>,
    hidden: bool,
    ext: HashSet<String>,
    exclude: Vec<String>,
}

impl ScanFilter {
    fn from_cli(cli: &Cli) -> Self {
        let ext: HashSet<String> = cli
            .ext
            .iter()
            .map(|e| e.trim_start_matches('.').to_lowercase())
            .collect();
        Self {
            min_size: cli.min_size,
            max_size: cli.max_size,
            hidden: cli.hidden,
            ext,
            exclude: cli.exclude.clone(),
        }
    }

    fn file_size_ok(&self, size: u64) -> bool {
        if size < self.min_size {
            return false;
        }
        if let Some(max) = self.max_size {
            if size > max {
                return false;
            }
        }
        true
    }

    fn ext_ok(&self, path: &Path) -> bool {
        if self.ext.is_empty() {
            return true;
        }
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| self.ext.contains(&e.to_lowercase()))
            .unwrap_or(false)
    }
}

#[derive(Clone)]
struct WalkFilter {
    hidden_allowed: bool,
    exclude: Vec<String>,
}

impl ScanFilter {
    fn clone_for_walkdir(&self) -> WalkFilter {
        WalkFilter {
            hidden_allowed: self.hidden,
            exclude: self.exclude.clone(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct JsonFile {
    path: String,
    size: u64,
    mtime: u64,
}

#[derive(Serialize, Deserialize)]
struct JsonGroup {
    index: usize,
    size: u64,
    wasted: u64,
    keep: JsonFile,
    delete: Vec<JsonFile>,
}

#[derive(Serialize, Deserialize)]
struct JsonReport {
    path: String,
    keep_strategy: String,
    action: String,
    groups: Vec<JsonGroup>,
    total_groups: usize,
    total_files: usize,
    total_wasted: u64,
}

#[derive(Clone, Copy)]
struct SampleConfig {
    chunk: u64,
    threshold: u64,
}

impl SampleConfig {
    fn from_cli(cli: &Cli) -> Self {
        Self {
            chunk: cli.sample_chunk,
            threshold: cli.sample_threshold,
        }
    }

    fn should_sample(&self, size: u64) -> bool {
        self.chunk > 0 && size >= self.threshold
    }

    fn active(&self) -> bool {
        self.chunk > 0
    }
}

fn current_year() -> u64 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut year = 1970u64;
    let mut remaining = secs;

    loop {
        let days = if is_leap_year(year) { 366 } else { 365 };
        let year_secs = days * 86_400;
        if remaining < year_secs {
            break;
        }
        remaining -= year_secs;
        year += 1;
    }

    year
}

fn is_leap_year(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn should_colorize(mode: ColorMode) -> bool {
    match mode {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => {
            use std::io::IsTerminal;
            std::io::stdout().is_terminal()
        }
    }
}

struct Colors {
    enabled: bool,
}

impl Colors {
    fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    fn wrap(&self, code: &str, s: &str) -> String {
        if self.enabled {
            format!("\x1b[{}m{}\x1b[0m", code, s)
        } else {
            s.to_string()
        }
    }

    fn green(&self, s: &str) -> String {
        self.wrap("32", s)
    }

    fn red(&self, s: &str) -> String {
        self.wrap("31", s)
    }

    fn cyan(&self, s: &str) -> String {
        self.wrap("36", s)
    }

    fn bold(&self, s: &str) -> String {
        self.wrap("1", s)
    }

    fn yellow(&self, s: &str) -> String {
        self.wrap("33", s)
    }
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{:.2} {}", size, UNITS[unit])
    }
}

fn display_path(path: &Path) -> String {
    if let Some(home) = env::var_os("HOME") {
        let home = PathBuf::from(home);
        if let Ok(rel) = path.strip_prefix(&home) {
            return format!("~/{}", rel.display());
        }
    }
    path.display().to_string()
}

fn phase_bar(prefix: &str, quiet: bool) -> Option<ProgressBar> {
    if quiet {
        return None;
    }
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template(&format!("{{spinner:.green}} {} {{msg}}", prefix))
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
    );
    pb.enable_steady_tick(Duration::from_millis(100));
    Some(pb)
}

fn phase_scan(path: &Path, cli: &Cli, filter: &ScanFilter, quiet: bool) -> Vec<FileEntry> {
    let pb = phase_bar("[1/5] Scanning:", quiet);
    let start = Instant::now();
    let mut files: Vec<FileEntry> = Vec::new();
    let mut dirs: u64 = 0;
    let mut last_update = Instant::now();

    let walk_filter = filter.clone_for_walkdir();

    let walker = WalkDir::new(path)
        .follow_links(cli.follow_links)
        .into_iter()
        .filter_entry(move |e| {
            if e.depth() == 0 {
                return true;
            }
            if !walk_filter.hidden_allowed
                && e.file_name()
                    .to_str()
                    .map(|n| n.starts_with('.'))
                    .unwrap_or(false)
            {
                return false;
            }
            if !walk_filter.exclude.is_empty() {
                let s = e.path().to_string_lossy();
                if walk_filter.exclude.iter().any(|p| s.contains(p.as_str())) {
                    return false;
                }
            }
            true
        });

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if entry.depth() == 0 {
            continue;
        }

        let ft = entry.file_type();
        if ft.is_dir() {
            dirs += 1;
        } else if ft.is_file() {
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            let size = meta.len();
            if !filter.file_size_ok(size) {
                continue;
            }
            if !filter.ext_ok(entry.path()) {
                continue;
            }

            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            files.push(FileEntry {
                path: entry.path().to_path_buf(),
                size,
                mtime,
            });
        }

        if last_update.elapsed() >= Duration::from_millis(100) {
            if let Some(ref pb) = pb {
                pb.set_message(format!("files: {}  dirs: {}", files.len(), dirs));
            }
            last_update = Instant::now();
        }
    }

    if let Some(pb) = pb {
        pb.finish_and_clear();
    }
    if !quiet {
        println!(
            "[1/5] Scanning: {} files, {} dirs ({:.2}s)",
            files.len(),
            dirs,
            start.elapsed().as_secs_f64()
        );
    }
    files
}

fn phase_group_by_size(files: Vec<FileEntry>, quiet: bool) -> Vec<Vec<FileEntry>> {
    let start = Instant::now();

    let mut by_size: HashMap<u64, Vec<FileEntry>> = HashMap::new();
    for f in files {
        by_size.entry(f.size).or_default().push(f);
    }

    let groups: Vec<Vec<FileEntry>> = by_size.into_values().filter(|v| v.len() > 1).collect();

    let total: usize = groups.iter().map(|g| g.len()).sum();
    if !quiet {
        println!(
            "[2/5] Grouping by size: {} candidate groups, {} files ({:.2}s)",
            groups.len(),
            total,
            start.elapsed().as_secs_f64()
        );
    }
    groups
}

fn hash_prefix(path: &Path, size: u64) -> Option<blake3::Hash> {
    let mut file = File::open(path).ok()?;
    let to_read = PREFIX_BYTES.min(size) as usize;
    let mut buf = vec![0u8; to_read];
    file.read_exact(&mut buf).ok()?;
    Some(blake3::hash(&buf))
}

fn hash_range(file: &mut File, offset: u64, len: u64, hasher: &mut Hasher) -> Option<()> {
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut remaining = len;
    let mut buf = vec![0u8; 64 * 1024];
    while remaining > 0 {
        let to_read = remaining.min(buf.len() as u64) as usize;
        let n = file.read(&mut buf[..to_read]).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        remaining -= n as u64;
    }
    Some(())
}

fn hash_full(path: &Path, size: u64, sample: SampleConfig) -> Option<blake3::Hash> {
    let mut file = File::open(path).ok()?;
    let mut hasher = Hasher::new();

    if sample.should_sample(size) {
        let chunk = sample.chunk.min(size);

        hash_range(&mut file, 0, chunk, &mut hasher)?;

        if size > chunk {
            let middle_offset = (size - chunk) / 2;
            hash_range(&mut file, middle_offset, chunk, &mut hasher)?;
        }

        if size > 2 * chunk {
            let end_offset = size - chunk;
            hash_range(&mut file, end_offset, chunk, &mut hasher)?;
        }

        Some(hasher.finalize())
    } else {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = file.read(&mut buf).ok()?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Some(hasher.finalize())
    }
}

fn phase_hash_prefix(groups: Vec<Vec<FileEntry>>, quiet: bool) -> Vec<Vec<FileEntry>> {
    let start = Instant::now();

    let mut buckets: HashMap<(u64, blake3::Hash), Vec<FileEntry>> = HashMap::new();

    for group in groups {
        for f in group {
            if let Some(h) = hash_prefix(&f.path, f.size) {
                buckets.entry((f.size, h)).or_default().push(f);
            }
        }
    }

    let out: Vec<Vec<FileEntry>> = buckets.into_values().filter(|v| v.len() > 1).collect();

    let total_out: usize = out.iter().map(|g| g.len()).sum();
    if !quiet {
        println!(
            "[3/5] Prefix hash: {} candidate groups, {} files ({:.2}s)",
            out.len(),
            total_out,
            start.elapsed().as_secs_f64()
        );
    }
    out
}

fn phase_hash_full(
    groups: Vec<Vec<FileEntry>>,
    sample: SampleConfig,
    quiet: bool,
) -> Vec<Vec<FileEntry>> {
    let start = Instant::now();
    let total: usize = groups.iter().map(|g| g.len()).sum();

    let pb = if quiet {
        None
    } else {
        let pb = ProgressBar::new(total as u64);
        pb.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [4/5] Full hash:   [{bar:40.cyan/blue}] {pos}/{len}",
            )
            .unwrap()
            .progress_chars("#>-"),
        );
        Some(pb)
    };

    let flat: Vec<(u64, FileEntry)> = groups
        .into_iter()
        .flat_map(|group| {
            let size = group[0].size;
            group.into_iter().map(move |f| (size, f))
        })
        .collect();

    let pb_ref = pb.as_ref();
    let results: Vec<(u64, FileEntry, blake3::Hash)> = flat
        .into_par_iter()
        .filter_map(|(size, f)| {
            let h = hash_full(&f.path, size, sample);
            if let Some(pb) = pb_ref {
                pb.inc(1);
            }
            h.map(|h| (size, f, h))
        })
        .collect();

    if let Some(pb) = pb {
        pb.finish_and_clear();
    }

    let mut buckets: HashMap<(u64, blake3::Hash), Vec<FileEntry>> = HashMap::new();
    for (size, f, h) in results {
        buckets.entry((size, h)).or_default().push(f);
    }

    let out: Vec<Vec<FileEntry>> = buckets.into_values().filter(|v| v.len() > 1).collect();

    let total_out: usize = out.iter().map(|g| g.len()).sum();
    if !quiet {
        let note = if sample.active() {
            format!(
                " (sampling: {} B chunks for files >= {} B)",
                sample.chunk, sample.threshold
            )
        } else {
            String::new()
        };
        println!(
            "[4/5] Full hash:   {} duplicate groups, {} files ({:.2}s){}",
            out.len(),
            total_out,
            start.elapsed().as_secs_f64(),
            note
        );
    }
    out
}

fn files_equal(a: &FileEntry, b: &FileEntry, sample: SampleConfig) -> bool {
    if a.size != b.size {
        return false;
    }

    let mut fa = match File::open(&a.path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut fb = match File::open(&b.path) {
        Ok(f) => f,
        Err(_) => return false,
    };

    if sample.should_sample(a.size) {
        let chunk = sample.chunk.min(a.size);

        if !compare_range(&mut fa, &mut fb, 0, chunk) {
            return false;
        }
        if a.size > chunk {
            let mid = (a.size - chunk) / 2;
            if !compare_range(&mut fa, &mut fb, mid, chunk) {
                return false;
            }
        }
        if a.size > 2 * chunk {
            let end = a.size - chunk;
            if !compare_range(&mut fa, &mut fb, end, chunk) {
                return false;
            }
        }
        true
    } else {
        let mut ba = vec![0u8; 64 * 1024];
        let mut bb = vec![0u8; 64 * 1024];
        loop {
            let na = match fa.read(&mut ba) {
                Ok(n) => n,
                Err(_) => return false,
            };
            let nb = match fb.read(&mut bb) {
                Ok(n) => n,
                Err(_) => return false,
            };
            if na != nb {
                return false;
            }
            if na == 0 {
                return true;
            }
            if ba[..na] != bb[..nb] {
                return false;
            }
        }
    }
}

fn compare_range(fa: &mut File, fb: &mut File, offset: u64, len: u64) -> bool {
    if fa.seek(SeekFrom::Start(offset)).is_err() {
        return false;
    }
    if fb.seek(SeekFrom::Start(offset)).is_err() {
        return false;
    }
    let mut remaining = len;
    let mut ba = vec![0u8; 64 * 1024];
    let mut bb = vec![0u8; 64 * 1024];
    while remaining > 0 {
        let to_read = remaining.min(ba.len() as u64) as usize;
        let na = match fa.read(&mut ba[..to_read]) {
            Ok(n) => n,
            Err(_) => return false,
        };
        let nb = match fb.read(&mut bb[..to_read]) {
            Ok(n) => n,
            Err(_) => return false,
        };
        if na != nb {
            return false;
        }
        if na == 0 {
            break;
        }
        if ba[..na] != bb[..nb] {
            return false;
        }
        remaining -= na as u64;
    }
    true
}

fn phase_confirm(
    groups: Vec<Vec<FileEntry>>,
    sample: SampleConfig,
    quiet: bool,
) -> Vec<Vec<FileEntry>> {
    let start = Instant::now();

    let confirmed: Vec<Vec<FileEntry>> = groups
        .into_par_iter()
        .filter_map(|group| {
            let reference = &group[0];
            let mut same = vec![reference.clone()];

            for other in group.iter().skip(1) {
                if files_equal(reference, other, sample) {
                    same.push(other.clone());
                }
            }

            if same.len() > 1 {
                Some(same)
            } else {
                None
            }
        })
        .collect();

    let total: usize = confirmed.iter().map(|g| g.len()).sum();
    if !quiet {
        println!(
            "[5/5] Confirming:   {} groups, {} files ({:.2}s)",
            confirmed.len(),
            total,
            start.elapsed().as_secs_f64()
        );
    }
    confirmed
}

fn pick_keeper(group: &[FileEntry], strategy: KeepStrategy) -> usize {
    match strategy {
        KeepStrategy::First => {
            let mut idx = 0;
            for i in 1..group.len() {
                if group[i].path < group[idx].path {
                    idx = i;
                }
            }
            idx
        }
        KeepStrategy::Newest => {
            let mut idx = 0;
            for i in 1..group.len() {
                if group[i].mtime > group[idx].mtime {
                    idx = i;
                }
            }
            idx
        }
        KeepStrategy::Oldest => {
            let mut idx = 0;
            for i in 1..group.len() {
                if group[i].mtime < group[idx].mtime {
                    idx = i;
                }
            }
            idx
        }
        KeepStrategy::Shortest => {
            let mut idx = 0;
            for i in 1..group.len() {
                if group[i].path.as_os_str().len() < group[idx].path.as_os_str().len() {
                    idx = i;
                }
            }
            idx
        }
    }
}

struct PreparedGroup {
    size: u64,
    wasted: u64,
    keep: FileEntry,
    delete: Vec<FileEntry>,
}

fn prepare_groups(groups: &[Vec<FileEntry>], strategy: KeepStrategy) -> Vec<PreparedGroup> {
    let mut prepared: Vec<PreparedGroup> = groups
        .iter()
        .map(|group| {
            let keeper = pick_keeper(group, strategy);
            let keep = group[keeper].clone();
            let delete: Vec<FileEntry> = group
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != keeper)
                .map(|(_, f)| f.clone())
                .collect();
            let size = group[0].size;
            let wasted = size * (group.len() as u64 - 1);
            PreparedGroup {
                size,
                wasted,
                keep,
                delete,
            }
        })
        .collect();

    prepared.sort_by(|a, b| b.wasted.cmp(&a.wasted));
    prepared
}

fn load_groups_from_json(path: &Path) -> Result<Vec<PreparedGroup>, String> {
    let content =
        fs::read_to_string(path).map_err(|e| format!("read {}: {}", path.display(), e))?;
    let report: JsonReport =
        serde_json::from_str(&content).map_err(|e| format!("parse {}: {}", path.display(), e))?;

    let mut groups: Vec<PreparedGroup> = report
        .groups
        .into_iter()
        .map(|g| PreparedGroup {
            size: g.size,
            wasted: g.wasted,
            keep: FileEntry {
                path: PathBuf::from(g.keep.path),
                size: g.keep.size,
                mtime: g.keep.mtime,
            },
            delete: g
                .delete
                .into_iter()
                .map(|f| FileEntry {
                    path: PathBuf::from(f.path),
                    size: f.size,
                    mtime: f.mtime,
                })
                .collect(),
        })
        .collect();

    groups.sort_by(|a, b| b.wasted.cmp(&a.wasted));
    Ok(groups)
}

fn validate_entry(entry: &FileEntry) -> Result<(), String> {
    let meta = fs::metadata(&entry.path).map_err(|e| format!("not accessible: {}", e))?;
    if meta.len() != entry.size {
        return Err(format!(
            "size changed: expected {}, got {}",
            entry.size,
            meta.len()
        ));
    }
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if mtime != entry.mtime {
        return Err(format!(
            "mtime changed: expected {}, got {}",
            entry.mtime, mtime
        ));
    }
    Ok(())
}

fn print_report(
    out: &mut dyn Write,
    groups: &[PreparedGroup],
    strategy: KeepStrategy,
    limit: Option<usize>,
    group_by_dir: bool,
    colors: &Colors,
    quiet_footer: bool,
) {
    writeln!(out).ok();
    writeln!(
        out,
        "{}",
        colors.bold(&format!("=== Duplicate report (keep: {:?}) ===", strategy))
    )
    .ok();
    writeln!(out).ok();

    if groups.is_empty() {
        writeln!(out, "No duplicates found.").ok();
        return;
    }

    let total_files: usize = groups.iter().map(|g| g.delete.len() + 1).sum();
    let wasted: u64 = groups.iter().map(|g| g.wasted).sum();

    let display_count = match limit {
        Some(n) => n.min(groups.len()),
        None => groups.len(),
    };

    for (i, group) in groups.iter().take(display_count).enumerate() {
        writeln!(
            out,
            "{}",
            colors.bold(&format!(
                "Group #{} — {} files, {} each, {} wasted",
                i + 1,
                group.delete.len() + 1,
                format_size(group.size),
                format_size(group.wasted)
            ))
        )
        .ok();
        writeln!(
            out,
            "  {} {}",
            colors.green("[KEEP]"),
            display_path(&group.keep.path)
        )
        .ok();
        for f in &group.delete {
            writeln!(out, "  {} {}", colors.red("[DEL] "), display_path(&f.path)).ok();
        }
        writeln!(out).ok();
    }

    if display_count < groups.len() {
        writeln!(
            out,
            "{}",
            colors.yellow(&format!(
                "... {} more groups not shown (use --limit to change)",
                groups.len() - display_count
            ))
        )
        .ok();
        writeln!(out).ok();
    }

    writeln!(
        out,
        "Total: {} groups, {} files, {} wasted",
        groups.len(),
        total_files,
        format_size(wasted),
    )
    .ok();

    if group_by_dir {
        print_dir_groups(out, groups, colors);
    }

    if quiet_footer {
        writeln!(out, "Nothing was deleted. This is a report only.").ok();
    }
}

fn print_dir_groups(out: &mut dyn Write, groups: &[PreparedGroup], colors: &Colors) {
    let mut dir_counts: HashMap<PathBuf, u64> = HashMap::new();

    for group in groups {
        for del in &group.delete {
            if let Some(parent) = del.path.parent() {
                *dir_counts.entry(parent.to_path_buf()).or_insert(0) += 1;
            }
        }
    }

    let mut dirs: Vec<(PathBuf, u64)> = dir_counts.into_iter().collect();
    dirs.sort_by(|a, b| b.1.cmp(&a.1));

    let significant: Vec<(PathBuf, u64)> = dirs.into_iter().filter(|(_, c)| *c >= 2).collect();

    if significant.is_empty() {
        return;
    }

    writeln!(out).ok();
    writeln!(
        out,
        "{}",
        colors.bold("=== Directories with most duplicates ===")
    )
    .ok();
    writeln!(out).ok();
    for (dir, count) in significant.iter().take(20) {
        writeln!(
            out,
            "  {} {} duplicates",
            colors.cyan(&count.to_string()),
            display_path(dir)
        )
        .ok();
    }
}

fn print_json_report(
    out: &mut dyn Write,
    groups: &[PreparedGroup],
    strategy: KeepStrategy,
    action: Action,
    root: Option<&Path>,
    limit: Option<usize>,
) {
    let display_count = match limit {
        Some(n) => n.min(groups.len()),
        None => groups.len(),
    };

    let json_groups: Vec<JsonGroup> = groups
        .iter()
        .take(display_count)
        .enumerate()
        .map(|(i, g)| JsonGroup {
            index: i + 1,
            size: g.size,
            wasted: g.wasted,
            keep: JsonFile {
                path: g.keep.path.display().to_string(),
                size: g.keep.size,
                mtime: g.keep.mtime,
            },
            delete: g
                .delete
                .iter()
                .map(|f| JsonFile {
                    path: f.path.display().to_string(),
                    size: f.size,
                    mtime: f.mtime,
                })
                .collect(),
        })
        .collect();

    let total_files: usize = groups.iter().map(|g| g.delete.len() + 1).sum();
    let total_wasted: u64 = groups.iter().map(|g| g.wasted).sum();

    let report = JsonReport {
        path: root.map(|p| p.display().to_string()).unwrap_or_default(),
        keep_strategy: format!("{:?}", strategy),
        action: format!("{:?}", action),
        groups: json_groups,
        total_groups: groups.len(),
        total_files,
        total_wasted,
    };

    let json = serde_json::to_string_pretty(&report).unwrap();
    writeln!(out, "{}", json).ok();
}

fn shell_quote(path: &Path) -> String {
    let s = path.to_string_lossy();
    if s.chars()
        .all(|c| c.is_alphanumeric() || "/._-+@=,:".contains(c))
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

fn print_shell_script(
    out: &mut dyn Write,
    groups: &[PreparedGroup],
    action: Action,
    action_dir: Option<&Path>,
    strategy: KeepStrategy,
    root: Option<&Path>,
) -> Result<(), String> {
    if action == Action::Report {
        return Err("shell output requires --action delete|move|hardlink".to_string());
    }
    if action == Action::Trash {
        return Err(
            "shell output does not support the trash action (XDG metadata required); \
             use --action trash directly instead"
                .to_string(),
        );
    }
    if action == Action::Move && action_dir.is_none() {
        return Err("shell output with move requires --action-dir".to_string());
    }

    let total_files: usize = groups.iter().map(|g| g.delete.len()).sum();
    let total_wasted: u64 = groups.iter().map(|g| g.wasted).sum();

    writeln!(out, "#!/bin/bash").ok();
    writeln!(out, "set -euo pipefail").ok();
    writeln!(out).ok();
    writeln!(out, "# Generated by Smart File Duplicate Manager").ok();
    writeln!(out, "# Keep strategy: {:?}", strategy).ok();
    writeln!(out, "# Action: {:?}", action).ok();
    if let Some(r) = root {
        writeln!(out, "# Root: {}", r.display()).ok();
    }
    if let Some(dir) = action_dir {
        writeln!(out, "# Action dir: {}", dir.display()).ok();
    }
    writeln!(out, "# Groups: {}", groups.len()).ok();
    writeln!(out, "# Files to process: {}", total_files).ok();
    writeln!(out, "# Space to free: {}", format_size(total_wasted)).ok();
    writeln!(out, "#").ok();
    writeln!(out, "# Review carefully before running.").ok();
    writeln!(out).ok();

    for (i, group) in groups.iter().enumerate() {
        writeln!(
            out,
            "# Group #{}: {} each, {} wasted",
            i + 1,
            format_size(group.size),
            format_size(group.wasted)
        )
        .ok();
        writeln!(out, "# KEEP: {}", group.keep.path.display()).ok();

        for f in &group.delete {
            let cmd = match action {
                Action::Delete => format!("rm -- {}", shell_quote(&f.path)),
                Action::Move => {
                    let dir = action_dir.unwrap();
                    let name = f.path.file_name().unwrap_or_default();
                    let dest = dir.join(name);
                    format!(
                        "mv -- {} {}",
                        shell_quote(&f.path),
                        shell_quote(&dest)
                    )
                }
                Action::Hardlink => {
                    let tmp = f.path.with_extension("sfdm_tmp");
                    format!(
                        "ln -f -- {} {} && rm -- {} && mv -- {} {}",
                        shell_quote(&group.keep.path),
                        shell_quote(&tmp),
                        shell_quote(&f.path),
                        shell_quote(&tmp),
                        shell_quote(&f.path)
                    )
                }
                _ => unreachable!(),
            };
            writeln!(out, "{}", cmd).ok();
        }
        writeln!(out).ok();
    }

    Ok(())
}

fn action_bar(prefix: &str, total: u64) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::with_template(&format!(
            "{{spinner:.green}} {} [{{bar:40.cyan/blue}}] {{pos}}/{{len}}",
            prefix
        ))
        .unwrap()
        .progress_chars("#>-"),
    );
    pb
}

fn trash_dir() -> PathBuf {
    if let Some(home) = env::var_os("HOME") {
        PathBuf::from(home).join(".local/share/Trash")
    } else {
        PathBuf::from("/tmp/Trash")
    }
}

fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let mut candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    let ext = Path::new(name).extension().and_then(|s| s.to_str());
    for i in 1..=9999 {
        let new_name = match ext {
            Some(e) => format!("{}.{}.{}", stem, i, e),
            None => format!("{}.{}", stem, i),
        };
        candidate = dir.join(&new_name);
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{}.{}", name, std::process::id()))
}

fn format_trash_date(secs: u64) -> String {
    let mut year = 1970u64;
    let mut remaining = secs;
    loop {
        let days = if is_leap_year(year) { 366 } else { 365 };
        let year_secs = days * 86_400;
        if remaining < year_secs {
            break;
        }
        remaining -= year_secs;
        year += 1;
    }
    let day_of_year = remaining / 86_400;
    let mut month = 1u64;
    let mut day = day_of_year + 1;
    let month_days = |m: u64, y: u64| -> u64 {
        match m {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if is_leap_year(y) {
                    29
                } else {
                    28
                }
            }
            _ => 30,
        }
    };
    while day > month_days(month, year) {
        day -= month_days(month, year);
        month += 1;
    }
    let time_secs = remaining % 86_400;
    let hh = time_secs / 3600;
    let mm = (time_secs % 3600) / 60;
    let ss = time_secs % 60;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        year, month, day, hh, mm, ss
    )
}

fn move_to_trash(src: &Path) -> Result<PathBuf, String> {
    let trash = trash_dir();
    let files_dir = trash.join("files");
    let info_dir = trash.join("info");

    fs::create_dir_all(&files_dir).map_err(|e| format!("mkdir files: {}", e))?;
    fs::create_dir_all(&info_dir).map_err(|e| format!("mkdir info: {}", e))?;

    let name = src
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "invalid filename".to_string())?;

    let dest = unique_path(&files_dir, name);
    let dest_name = dest
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "invalid dest name".to_string())?;

    fs::rename(src, &dest)
        .or_else(|_| {
            fs::copy(src, &dest)?;
            fs::remove_file(src)
        })
        .map_err(|e| format!("move: {}", e))?;

    let info_path = info_dir.join(format!("{}.trashinfo", dest_name));
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let content = format!(
        "[Trash Info]\nPath={}\nDeletionDate={}\n",
        src.display(),
        format_trash_date(now),
    );
    let mut f = File::create(&info_path).map_err(|e| format!("info: {}", e))?;
    f.write_all(content.as_bytes())
        .map_err(|e| format!("info write: {}", e))?;

    Ok(dest)
}

fn do_trash(src: &Path, _keeper: &Path) -> Result<u64, String> {
    let size = fs::metadata(src).map(|m| m.len()).unwrap_or(0);
    move_to_trash(src)?;
    Ok(size)
}

fn do_move(src: &Path, dest_dir: &Path) -> Result<u64, String> {
    fs::create_dir_all(dest_dir).map_err(|e| format!("mkdir dest: {}", e))?;
    let name = src
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "invalid filename".to_string())?;
    let dest = unique_path(dest_dir, name);
    let size = fs::metadata(src).map(|m| m.len()).unwrap_or(0);
    fs::rename(src, &dest)
        .or_else(|_| {
            fs::copy(src, &dest)?;
            fs::remove_file(src)
        })
        .map_err(|e| format!("move: {}", e))?;
    Ok(size)
}

fn do_delete(src: &Path) -> Result<u64, String> {
    let size = fs::metadata(src).map(|m| m.len()).unwrap_or(0);
    fs::remove_file(src).map_err(|e| format!("delete: {}", e))?;
    Ok(size)
}

fn do_hardlink(src: &Path, keeper: &Path) -> Result<u64, String> {
    let size = fs::metadata(src).map(|m| m.len()).unwrap_or(0);
    let tmp = src.with_extension("sfdm_tmp");
    fs::hard_link(keeper, &tmp).map_err(|e| format!("hardlink: {}", e))?;
    if let Err(e) = fs::remove_file(src) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("remove after link: {}", e));
    }
    fs::rename(&tmp, src).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("rename after link: {}", e)
    })?;
    Ok(size)
}

fn perform_actions(
    actions: &[(FileEntry, FileEntry)],
    action: Action,
    action_dir: Option<&Path>,
    dry_run: bool,
    quiet: bool,
    validate: bool,
) -> ActionSummary {
    let mut summary = ActionSummary::default();

    if actions.is_empty() {
        return summary;
    }

    if dry_run {
        println!("[dry-run] Would perform {} actions:", actions.len());
        for (victim, keeper) in actions.iter().take(10) {
            println!(
                "  {:?} {} (keep {})",
                action,
                display_path(&victim.path),
                display_path(&keeper.path)
            );
        }
        if actions.len() > 10 {
            println!("  ... and {} more", actions.len() - 10);
        }
        summary.processed = actions.len() as u64;
        summary.bytes_freed = actions.iter().map(|(v, _)| v.size).sum();
        return summary;
    }

    let pb = if quiet {
        None
    } else {
        Some(action_bar(
            &format!("[{}] Action:", format!("{:?}", action).to_lowercase()),
            actions.len() as u64,
        ))
    };

    for (victim, keeper) in actions {
        if validate {
            if let Err(e) = validate_entry(victim) {
                summary.failed += 1;
                summary
                    .errors
                    .push(format!("{}: {}", display_path(&victim.path), e));
                if let Some(ref pb) = pb {
                    pb.inc(1);
                }
                continue;
            }
        }

        let result = match action {
            Action::Report => Ok(0),
            Action::Trash => do_trash(&victim.path, &keeper.path),
            Action::Move => match action_dir {
                Some(dir) => do_move(&victim.path, dir),
                None => Err("--action-dir is required for move".to_string()),
            },
            Action::Delete => do_delete(&victim.path),
            Action::Hardlink => do_hardlink(&victim.path, &keeper.path),
        };

        match result {
            Ok(size) => {
                summary.processed += 1;
                summary.bytes_freed += size;
            }
            Err(e) => {
                summary.failed += 1;
                summary
                    .errors
                    .push(format!("{}: {}", display_path(&victim.path), e));
            }
        }
        if let Some(ref pb) = pb {
            pb.inc(1);
        }
    }

    if let Some(pb) = pb {
        pb.finish_and_clear();
    }
    summary
}

fn write_footer(out: &mut dyn Write) {
    let year = current_year();
    writeln!(out).ok();
    writeln!(out, "Copyright (c) {} {} <{}>", year, AUTHOR, GITHUB).ok();
    writeln!(out, "Repo: {}", REPO_URL).ok();
}

fn main() {
    let cli = Cli::parse();

    let json_to_stdout = cli.output == OutputFormat::Json && cli.output_file.is_none();
    let writing_to_file = cli.output_file.is_some();
    let quiet = json_to_stdout;
    let colors = Colors::new(should_colorize(cli.color) && !quiet);
    let text_output = cli.output == OutputFormat::Text;
    let shell_output = cli.output == OutputFormat::Shell;
    let sample = SampleConfig::from_cli(&cli);

    if shell_output {
        if cli.action == Action::Report {
            eprintln!("Error: --output shell requires --action delete|move|hardlink");
            std::process::exit(1);
        }
        if cli.action == Action::Trash {
            eprintln!("Error: --output shell does not support the trash action; use --action trash directly");
            std::process::exit(1);
        }
        if cli.action == Action::Move && cli.action_dir.is_none() {
            eprintln!("Error: --output shell with move requires --action-dir");
            std::process::exit(1);
        }
    }

    let mut out: Box<dyn Write> = match &cli.output_file {
        Some(p) => match File::create(p) {
            Ok(f) => Box::new(f),
            Err(e) => {
                eprintln!("Error: cannot create output file: {}", e);
                std::process::exit(1);
            }
        },
        None => Box::new(std::io::stdout()),
    };

    if !quiet && !shell_output && !writing_to_file {
        writeln!(out, "{} v{}", APP_NAME, VERSION).ok();
        writeln!(out).ok();
    }

    if cli.path.is_none() && cli.from_report.is_none() {
        eprintln!("Error: --path or --from-report is required");
        std::process::exit(1);
    }
    if cli.path.is_some() && cli.from_report.is_some() {
        eprintln!("Error: --path and --from-report cannot be used together");
        std::process::exit(1);
    }

    if cli.action == Action::Move && cli.action_dir.is_none() {
        eprintln!("Error: --action move requires --action-dir <DIR>");
        std::process::exit(1);
    }

    let (prepared, root_for_report, use_validation) = if let Some(ref report_path) = cli.from_report {
        let groups = match load_groups_from_json(report_path) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("Error loading report: {}", e);
                std::process::exit(1);
            }
        };
        if !quiet && !shell_output && !writing_to_file {
            writeln!(
                out,
                "Loaded {} groups from report: {}",
                groups.len(),
                report_path.display()
            )
            .ok();
        } else if !quiet && !shell_output && writing_to_file {
            eprintln!(
                "Loaded {} groups from report: {}",
                groups.len(),
                report_path.display()
            );
        }
        (groups, None, true)
    } else {
        let path = cli.path.as_ref().unwrap();
        if !path.exists() {
            eprintln!("Error: path does not exist: {}", path.display());
            std::process::exit(1);
        }
        if !path.is_dir() {
            eprintln!("Error: path is not a directory: {}", path.display());
            std::process::exit(1);
        }

        if !quiet && !shell_output && sample.active() {
            eprintln!(
                "Sampling: {} B chunks for files >= {} B",
                sample.chunk, sample.threshold
            );
        }

        let filter = ScanFilter::from_cli(&cli);
        let files = phase_scan(path, &cli, &filter, quiet || shell_output);
        let by_size = phase_group_by_size(files, quiet || shell_output);
        let by_prefix = phase_hash_prefix(by_size, quiet || shell_output);
        let by_full = phase_hash_full(by_prefix, sample, quiet || shell_output);
        let confirmed = phase_confirm(by_full, sample, quiet || shell_output);
        let groups = prepare_groups(&confirmed, cli.keep);
        (groups, Some(path.as_path()), false)
    };

    if shell_output {
        if let Err(e) = print_shell_script(
            &mut out,
            &prepared,
            cli.action,
            cli.action_dir.as_deref(),
            cli.keep,
            root_for_report,
        ) {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
        out.flush().ok();
        return;
    }

    if cli.output == OutputFormat::Json {
        print_json_report(
            &mut out,
            &prepared,
            cli.keep,
            cli.action,
            root_for_report,
            cli.limit,
        );
    } else {
        let quiet_footer = cli.action == Action::Report;
        print_report(
            &mut out,
            &prepared,
            cli.keep,
            cli.limit,
            cli.group_by_dir,
            &colors,
            quiet_footer,
        );
    }

    if cli.action == Action::Report {
        if text_output && !writing_to_file {
            write_footer(&mut out);
        }
        out.flush().ok();
        return;
    }

    let actions: Vec<(FileEntry, FileEntry)> = prepared
        .iter()
        .flat_map(|g| {
            g.delete
                .iter()
                .map(|d| (d.clone(), g.keep.clone()))
                .collect::<Vec<_>>()
        })
        .collect();

    let wasted: u64 = actions.iter().map(|(v, _)| v.size).sum();

    if !quiet {
        writeln!(out).ok();
        writeln!(
            out,
            "{}",
            colors.bold(&format!("=== Action plan ({:?}) ===", cli.action))
        )
        .ok();
        writeln!(out).ok();
        writeln!(out, "Files to process: {}", actions.len()).ok();
        writeln!(out, "Space to free:    {}", format_size(wasted)).ok();
        if use_validation {
            writeln!(out, "Validation:       enabled (checking size and mtime)").ok();
        }
    }

    let dry_run = cli.dry_run || !cli.yes;

    if !cli.yes && !cli.dry_run && !quiet {
        writeln!(out).ok();
        writeln!(out, "This is a dry run. To execute, add --yes").ok();
    }

    let summary = perform_actions(
        &actions,
        cli.action,
        cli.action_dir.as_deref(),
        dry_run,
        quiet,
        use_validation,
    );

    if !quiet {
        writeln!(out).ok();
        if dry_run {
            writeln!(out, "[DRY RUN] No changes were made.").ok();
            writeln!(out, "Processed (simulated): {}", summary.processed).ok();
            writeln!(
                out,
                "Space to free:         {}",
                format_size(summary.bytes_freed)
            )
            .ok();
        } else {
            writeln!(out, "Processed: {}", summary.processed).ok();
            writeln!(out, "Failed:    {}", summary.failed).ok();
            writeln!(out, "Freed:     {}", format_size(summary.bytes_freed)).ok();
        }

        if !summary.errors.is_empty() {
            writeln!(out).ok();
            writeln!(out, "Errors:").ok();
            for e in summary.errors.iter().take(20) {
                writeln!(out, "  {}", e).ok();
            }
            if summary.errors.len() > 20 {
                writeln!(out, "  ... and {} more", summary.errors.len() - 20).ok();
            }
        }
    }

    if !quiet && text_output && !writing_to_file {
        write_footer(&mut out);
    }

    out.flush().ok();
}