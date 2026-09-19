use blake3::Hasher;
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

const APP_NAME: &str = "Smart File Duplicate Manager";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const AUTHOR: &str = "Alexander Suvorov";
const GITHUB: &str = "smartlegionlab";
const REPO_URL: &str = "https://github.com/smartlegionlab";

const PREFIX_BYTES: u64 = 4096;

#[derive(Parser, Debug)]
#[command(name = APP_NAME, version = VERSION, author = AUTHOR)]
struct Cli {
    #[arg(short = 'p', long = "path", value_name = "PATH")]
    path: PathBuf,

    #[arg(long = "min-size", value_name = "BYTES", default_value_t = 1)]
    min_size: u64,

    #[arg(long = "follow-links")]
    follow_links: bool,
}

#[derive(Debug, Clone)]
struct FileEntry {
    path: PathBuf,
    size: u64,
    mtime: u64,
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

fn print_header() {
    println!("{} v{}", APP_NAME, VERSION);
    println!();
}

fn print_footer() {
    let year = current_year();
    println!();
    println!("Copyright (c) {} {} <{}>", year, AUTHOR, GITHUB);
    println!("Repo: {}", REPO_URL);
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

fn phase_bar(prefix: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template(&format!("{{spinner:.green}} {} {{msg}}", prefix))
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
    );
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

fn phase_scan(path: &Path, cli: &Cli) -> Vec<FileEntry> {
    let pb = phase_bar("[1/5] Scanning:");
    let start = Instant::now();
    let mut files: Vec<FileEntry> = Vec::new();
    let mut dirs: u64 = 0;
    let mut last_update = Instant::now();

    for entry in WalkDir::new(path).follow_links(cli.follow_links) {
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
            if size < cli.min_size {
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
            pb.set_message(format!("files: {}  dirs: {}", files.len(), dirs));
            last_update = Instant::now();
        }
    }

    pb.finish_and_clear();
    println!(
        "[1/5] Scanning: {} files, {} dirs ({:.2}s)",
        files.len(),
        dirs,
        start.elapsed().as_secs_f64()
    );
    files
}

fn phase_group_by_size(files: Vec<FileEntry>) -> Vec<Vec<FileEntry>> {
    let start = Instant::now();

    let mut by_size: HashMap<u64, Vec<FileEntry>> = HashMap::new();
    for f in files {
        by_size.entry(f.size).or_default().push(f);
    }

    let groups: Vec<Vec<FileEntry>> = by_size.into_values().filter(|v| v.len() > 1).collect();

    let total: usize = groups.iter().map(|g| g.len()).sum();
    println!(
        "[2/5] Grouping by size: {} candidate groups, {} files ({:.2}s)",
        groups.len(),
        total,
        start.elapsed().as_secs_f64()
    );
    groups
}

fn hash_prefix(path: &Path, size: u64) -> Option<blake3::Hash> {
    let mut file = File::open(path).ok()?;
    let to_read = PREFIX_BYTES.min(size) as usize;
    let mut buf = vec![0u8; to_read];
    file.read_exact(&mut buf).ok()?;
    Some(blake3::hash(&buf))
}

fn hash_full(path: &Path) -> Option<blake3::Hash> {
    let mut file = File::open(path).ok()?;
    let mut hasher = Hasher::new();
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

fn phase_hash_prefix(groups: Vec<Vec<FileEntry>>) -> Vec<Vec<FileEntry>> {
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
    println!(
        "[3/5] Prefix hash: {} candidate groups, {} files ({:.2}s)",
        out.len(),
        total_out,
        start.elapsed().as_secs_f64()
    );
    out
}

fn phase_hash_full(groups: Vec<Vec<FileEntry>>) -> Vec<Vec<FileEntry>> {
    let start = Instant::now();
    let total: usize = groups.iter().map(|g| g.len()).sum();

    let pb = ProgressBar::new(total as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [4/5] Full hash:   [{bar:40.cyan/blue}] {pos}/{len}",
        )
        .unwrap()
        .progress_chars("#>-"),
    );

    let flat: Vec<(u64, FileEntry)> = groups
        .into_iter()
        .flat_map(|group| {
            let size = group[0].size;
            group.into_iter().map(move |f| (size, f))
        })
        .collect();

    let pb_ref = &pb;
    let results: Vec<(u64, FileEntry, blake3::Hash)> = flat
        .into_par_iter()
        .filter_map(|(size, f)| {
            let h = hash_full(&f.path);
            pb_ref.inc(1);
            h.map(|h| (size, f, h))
        })
        .collect();

    pb.finish_and_clear();

    let mut buckets: HashMap<(u64, blake3::Hash), Vec<FileEntry>> = HashMap::new();
    for (size, f, h) in results {
        buckets.entry((size, h)).or_default().push(f);
    }

    let out: Vec<Vec<FileEntry>> = buckets.into_values().filter(|v| v.len() > 1).collect();

    let total_out: usize = out.iter().map(|g| g.len()).sum();
    println!(
        "[4/5] Full hash:   {} duplicate groups, {} files ({:.2}s)",
        out.len(),
        total_out,
        start.elapsed().as_secs_f64()
    );
    out
}

fn phase_confirm(groups: Vec<Vec<FileEntry>>) -> Vec<Vec<FileEntry>> {
    let start = Instant::now();

    let confirmed: Vec<Vec<FileEntry>> = groups
        .into_par_iter()
        .filter_map(|group| {
            let reference = &group[0];
            let mut same = vec![reference.clone()];

            for other in group.iter().skip(1) {
                if files_equal(reference, other) {
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
    println!(
        "[5/5] Confirming:   {} groups, {} files ({:.2}s)",
        confirmed.len(),
        total,
        start.elapsed().as_secs_f64()
    );
    confirmed
}

fn files_equal(a: &FileEntry, b: &FileEntry) -> bool {
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

fn print_report(groups: &[Vec<FileEntry>]) {
    println!();
    println!("=== Duplicate report ===");
    println!();

    if groups.is_empty() {
        println!("No duplicates found.");
        return;
    }

    let total_files: usize = groups.iter().map(|g| g.len()).sum();
    let wasted: u64 = groups
        .iter()
        .map(|g| g[0].size * (g.len() as u64 - 1))
        .sum();

    let mut sorted: Vec<&Vec<FileEntry>> = groups.iter().collect();
    sorted.sort_by(|a, b| {
        let wa = a[0].size * (a.len() as u64);
        let wb = b[0].size * (b.len() as u64);
        wb.cmp(&wa)
    });

    for (i, group) in sorted.iter().enumerate() {
        let size = group[0].size;
        let group_wasted = size * (group.len() as u64 - 1);
        println!(
            "Group #{} — {} files, {} each, {} wasted",
            i + 1,
            group.len(),
            format_size(size),
            format_size(group_wasted)
        );
        for f in group.iter() {
            println!("  {}", f.path.display());
        }
        println!();
    }

    println!(
        "Total: {} groups, {} files, {} wasted",
        groups.len(),
        total_files,
        format_size(wasted),
    );
}

fn main() {
    let cli = Cli::parse();

    print_header();

    let path = &cli.path;
    if !path.exists() {
        eprintln!("Error: path does not exist: {}", path.display());
        print_footer();
        std::process::exit(1);
    }
    if !path.is_dir() {
        eprintln!("Error: path is not a directory: {}", path.display());
        print_footer();
        std::process::exit(1);
    }

    let files = phase_scan(path, &cli);
    let by_size = phase_group_by_size(files);
    let by_prefix = phase_hash_prefix(by_size);
    let by_full = phase_hash_full(by_prefix);
    let confirmed = phase_confirm(by_full);

    print_report(&confirmed);

    print_footer();
}
