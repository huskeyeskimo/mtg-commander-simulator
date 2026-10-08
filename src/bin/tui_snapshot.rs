//! TUI Snapshot Generator — renders TUI frames to SVG for visual review.
//!
//! Generates SVG screenshots of the TUI at key game states, suitable for
//! automated PR review. Runs headlessly using ratatui's TestBackend.
//!
//! Usage:
//!   cargo run --features tui --bin tui_snapshot
//!   cargo run --features tui --bin tui_snapshot -- --preset red --output-dir screenshots
//!   cargo run --features tui --bin tui_snapshot -- --cols 120 --rows 40

use std::fs;
use std::path::Path;

use mtg_gto::tui;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let output_dir = get_arg(&args, "--output-dir")
        .unwrap_or_else(|| "tui_screenshots".into());
    let cols: u16 = get_arg(&args, "--cols")
        .and_then(|s| s.parse().ok())
        .unwrap_or(120);
    let rows: u16 = get_arg(&args, "--rows")
        .and_then(|s| s.parse().ok())
        .unwrap_or(40);
    let preset = get_arg(&args, "--preset")
        .unwrap_or_else(|| "kinnan".into());

    // Create output directory
    let out_path = Path::new(&output_dir);
    fs::create_dir_all(out_path).expect("Failed to create output directory");

    println!("Generating TUI snapshots...");
    println!("  Preset: {}", preset);
    println!("  Size: {}x{}", cols, rows);
    println!("  Output: {}/", output_dir);
    println!();

    let snapshots = match tui::generate_snapshots(&preset, cols, rows) {
        Ok(snapshots) => snapshots,
        Err(reason) => { eprintln!("INVALID reason={}", reason.code()); return; }
    };

    let mut manifest = String::new();
    manifest.push_str("# TUI Snapshots\n\n");
    manifest.push_str(&format!("Preset: `{}` | Terminal size: {}x{}\n\n", preset, cols, rows));

    for (scenario, svg) in &snapshots {
        let filename = format!("{}.svg", scenario.name);
        let filepath = out_path.join(&filename);
        fs::write(&filepath, svg).expect("Failed to write SVG");
        println!("  Wrote: {} ({})", filepath.display(), scenario.description);

        manifest.push_str(&format!("## {}\n\n", scenario.description));
        manifest.push_str(&format!("![{}]({})\n\n", scenario.name, filename));
    }

    // Write manifest
    let manifest_path = out_path.join("SNAPSHOTS.md");
    fs::write(&manifest_path, &manifest).expect("Failed to write manifest");
    println!();
    println!("  Manifest: {}", manifest_path.display());
    println!("Done. Generated {} snapshots.", snapshots.len());
}

fn get_arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}
