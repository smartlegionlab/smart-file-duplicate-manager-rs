use clap::Parser;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
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

fn scan_directory(path: &Path) -> ScanReport {
    let mut report = ScanReport::default();

    for entry in WalkDir::new(path).follow_links(false) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if entry.depth() == 0 {
            continue;
        }

        if entry.file_type().is_dir() {
            report.dirs += 1;
        } else if entry.file_type().is_file() {
            report.files += 1;
        }
    }

    report
}

fn print_report(path: &Path, report: &ScanReport) {
    println!("Scanning directory: {}", path.display());
    println!();
    println!("Directories found: {}", report.dirs);
    println!("Files found:       {}", report.files);
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

    let report = scan_directory(path);
    print_report(path, &report);

    print_footer();
}
