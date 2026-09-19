//! Smart File Duplicate Manager
//!
//! Author: Alexander Suvorov (smartlegionlab)
//! Repository: https://github.com/smartlegionlab

const APP_NAME: &str = "Smart File Duplicate Manager";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const AUTHOR: &str = "Alexander Suvorov";
const GITHUB: &str = "smartlegionlab";
const REPO_URL: &str = "https://github.com/smartlegionlab";

fn print_header() {
    println!("{} v{}", APP_NAME, VERSION);
    println!();
}

fn print_footer() {
    let year = 2025;
    println!();
    println!("Copyright (c) {} {} <{}>", year, AUTHOR, GITHUB);
    println!("Repo: {}", REPO_URL);
}

fn main() {
    print_header();
    print_footer();
}
