use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

const APP_NAME: &str = "Smart File Duplicate Manager";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const AUTHOR: &str = "Alexander Suvorov";
const GITHUB: &str = "smartlegionlab";
const REPO_URL: &str = "https://github.com/smartlegionlab";

#[derive(Parser, Debug)]
#[command(name = APP_NAME, version = VERSION, author = AUTHOR)]
struct Cli {
    #[arg(short = 'p', long = "path", value_name = "PATH")]
    path: PathBuf,

    #[arg(long = "count", help = "Only count files and directories (step 1)")]
    count: bool,
}

#[derive(Debug, Default)]
struct ScanReport {
    dirs: u64,
    files: u64,
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

fn make_progress_bar() -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner:.green} files: {msg}")
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
    );
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

fn step_count(path: &Path) -> ScanReport {
    println!("[Step 1] Recursive directory scan and count");
    println!("Scanning directory: {}", path.display());
    println!();

    let mut report = ScanReport::default();
    let pb = make_progress_bar();

    let mut last_update = Instant::now();
    let update_interval = Duration::from_millis(100);

    for entry in WalkDir::new(path).follow_links(false) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if entry.depth() == 0 {
            continue;
        }

        let ft = entry.file_type();
        if ft.is_dir() {
            report.dirs += 1;
        } else if ft.is_file() {
            report.files += 1;
        }

        if last_update.elapsed() >= update_interval {
            pb.set_message(format!("{}  dirs: {}", report.files, report.dirs));
            last_update = Instant::now();
        }
    }

    pb.finish_and_clear();

    println!("Directories found: {}", report.dirs);
    println!("Files found:       {}", report.files);

    report
}

fn run_all(path: &Path) {
    step_count(path);
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

    if cli.count {
        step_count(path);
    } else {
        run_all(path);
    }

    print_footer();
}
