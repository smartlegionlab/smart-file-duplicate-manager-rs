use blake3::Hasher;
use clap::{Parser, ValueEnum};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::io::{Read, Write};
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

    #[arg(long = "keep", value_enum, default_value_t = KeepStrategy::First)]
    keep: KeepStrategy,

    #[arg(long = "action", value_enum, default_value_t = Action::Report)]
    action: Action,

    #[arg(long = "action-dir", value_name = "DIR")]
    action_dir: Option<PathBuf>,

    #[arg(long = "yes")]
    yes: bool,

    #[arg(long = "dry-run")]
    dry_run: bool,
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

fn display_path(path: &Path) -> String {
    if let Some(home) = env::var_os("HOME") {
        let home = PathBuf::from(home);
        if let Ok(rel) = path.strip_prefix(&home) {
            return format!("~/{}", rel.display());
        }
    }
    path.display().to_string()
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

fn print_report(groups: &[Vec<FileEntry>], strategy: KeepStrategy) {
    println!();
    println!("=== Duplicate report (keep: {:?}) ===", strategy);
    println!();

    if groups.is_empty() {
        println!("No duplicates found.");
        return;
    }

    let mut indexed: Vec<(usize, &Vec<FileEntry>)> = groups.iter().enumerate().collect();
    indexed.sort_by(|a, b| {
        let wa = a.1[0].size * (a.1.len() as u64);
        let wb = b.1[0].size * (b.1.len() as u64);
        wb.cmp(&wa)
    });

    let total_files: usize = groups.iter().map(|g| g.len()).sum();
    let wasted: u64 = groups
        .iter()
        .map(|g| g[0].size * (g.len() as u64 - 1))
        .sum();

    for (display_idx, (_orig_idx, group)) in indexed.iter().enumerate() {
        let size = group[0].size;
        let group_wasted = size * (group.len() as u64 - 1);
        let keeper = pick_keeper(group, strategy);

        println!(
            "Group #{} — {} files, {} each, {} wasted",
            display_idx + 1,
            group.len(),
            format_size(size),
            format_size(group_wasted)
        );

        println!("  [KEEP] {}", display_path(&group[keeper].path));
        for (i, f) in group.iter().enumerate() {
            if i != keeper {
                println!("  [DEL]  {}", display_path(&f.path));
            }
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

fn collect_actions(
    groups: &[Vec<FileEntry>],
    strategy: KeepStrategy,
) -> Vec<(FileEntry, FileEntry)> {
    let mut actions = Vec::new();
    for group in groups {
        let keeper = pick_keeper(group, strategy);
        let keep = group[keeper].clone();
        for (i, f) in group.iter().enumerate() {
            if i != keeper {
                actions.push((f.clone(), keep.clone()));
            }
        }
    }
    actions
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
    let abs = fs::canonicalize(&dest).unwrap_or_else(|_| dest.clone());
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

    let _ = abs;
    Ok(dest)
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

    let pb = action_bar(
        &format!("[{}] Action:", format!("{:?}", action).to_lowercase()),
        actions.len() as u64,
    );

    for (victim, keeper) in actions {
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
        pb.inc(1);
    }

    pb.finish_and_clear();
    summary
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

    if cli.action == Action::Move && cli.action_dir.is_none() {
        eprintln!("Error: --action move requires --action-dir <DIR>");
        print_footer();
        std::process::exit(1);
    }

    let files = phase_scan(path, &cli);
    let by_size = phase_group_by_size(files);
    let by_prefix = phase_hash_prefix(by_size);
    let by_full = phase_hash_full(by_prefix);
    let confirmed = phase_confirm(by_full);

    print_report(&confirmed, cli.keep);

    let actions = collect_actions(&confirmed, cli.keep);
    let wasted: u64 = actions.iter().map(|(v, _)| v.size).sum();

    println!();
    println!("=== Action plan ({:?}) ===", cli.action);
    println!();
    println!("Files to process: {}", actions.len());
    println!("Space to free:    {}", format_size(wasted));

    if cli.action == Action::Report {
        println!();
        println!("Nothing was deleted. This is a report only.");
        print_footer();
        return;
    }

    let dry_run = cli.dry_run || !cli.yes;

    if !cli.yes && !cli.dry_run {
        println!();
        println!("This is a dry run. To execute, add --yes");
    }

    let summary = perform_actions(&actions, cli.action, cli.action_dir.as_deref(), dry_run);

    println!();
    if dry_run {
        println!("[DRY RUN] No changes were made.");
        println!("Processed (simulated): {}", summary.processed);
        println!(
            "Space to free:         {}",
            format_size(summary.bytes_freed)
        );
    } else {
        println!("Processed: {}", summary.processed);
        println!("Failed:    {}", summary.failed);
        println!("Freed:     {}", format_size(summary.bytes_freed));
    }

    if !summary.errors.is_empty() {
        println!();
        println!("Errors:");
        for e in summary.errors.iter().take(20) {
            println!("  {}", e);
        }
        if summary.errors.len() > 20 {
            println!("  ... and {} more", summary.errors.len() - 20);
        }
    }

    print_footer();
}
