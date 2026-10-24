//! Workspace automation. Run with `cargo xtask <command>`.
//!
//! - `size [board]`: release-build every firmware example for a board and
//!   report flash-image and RAM sizes. Binary size is a tracked metric in this
//!   project, so run it before and after any change that might affect it.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

struct Board {
    name: &'static str,
    chip: &'static str,
    target: &'static str,
}

const BOARDS: &[Board] = &[Board {
    name: "cardputer",
    chip: "esp32s3",
    target: "xtensa-esp32s3-none-elf",
}];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("size") => size(args.get(1).map(String::as_str).unwrap_or("cardputer")),
        _ => {
            eprintln!("usage: cargo xtask size [board]\nboards: {}", board_names());
            return ExitCode::FAILURE;
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn board_names() -> String {
    BOARDS.iter().map(|b| b.name).collect::<Vec<_>>().join(", ")
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn size(board: &str) -> Result<(), String> {
    let board = BOARDS
        .iter()
        .find(|b| b.name == board)
        .ok_or_else(|| format!("unknown board `{board}` (known: {})", board_names()))?;
    let example_dir = root().join("examples").join(board.name);

    // Building from the example dir picks up its target config. Bins with
    // `required-features` are built one by one with exactly those features,
    // so they never change how the plain bins are compiled.
    let bins = bins(&example_dir.join("Cargo.toml"))?;
    run(Command::new("cargo")
        .args(["build", "--release", "--bins"])
        .current_dir(&example_dir))?;
    for bin in bins.iter().filter(|b| !b.features.is_empty()) {
        run(Command::new("cargo")
            .args(["build", "--release", "--bin", &bin.name])
            .args(["--features", &bin.features.join(",")])
            .current_dir(&example_dir))?;
    }

    let out_dir = root().join("target").join(board.target).join("release");

    println!(
        "\n{:<14} {:>12} {:>10} {:>10}",
        "firmware", "flash image", ".data", ".bss"
    );
    println!("{}", "-".repeat(50));
    for Bin { name: bin, .. } in bins {
        let elf = out_dir.join(&bin);
        let image = out_dir.join(format!("{bin}.bin"));
        let status = Command::new("espflash")
            .args(["save-image", "--chip", board.chip])
            .arg(&elf)
            .arg(&image)
            .output()
            .map_err(|e| format!("running espflash: {e}"))?;
        if !status.status.success() {
            return Err(format!("espflash save-image failed for {bin}"));
        }
        let flash = std::fs::metadata(&image).map_err(|e| e.to_string())?.len();
        let (data, bss) = ram_sections(&elf, board.chip).unwrap_or((0, 0));
        println!(
            "{bin:<14} {:>12} {:>10} {:>10}",
            group(flash),
            group(data),
            group(bss)
        );
    }
    Ok(())
}

struct Bin {
    name: String,
    features: Vec<String>,
}

/// `[[bin]]` entries (`name`, `required-features`), read without a TOML
/// dependency. Assumes one `key = value` per line.
fn bins(manifest: &Path) -> Result<Vec<Bin>, String> {
    let text = std::fs::read_to_string(manifest).map_err(|e| e.to_string())?;
    let mut in_bin = false;
    let mut bins: Vec<Bin> = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_bin = line == "[[bin]]";
            continue;
        }
        let Some((key, value)) = line.split_once('=').filter(|_| in_bin) else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "name" => bins.push(Bin {
                name: value.trim_matches('"').to_string(),
                features: Vec::new(),
            }),
            "required-features" => {
                if let Some(bin) = bins.last_mut() {
                    bin.features = value
                        .trim_matches(['[', ']'])
                        .split(',')
                        .map(|f| f.trim().trim_matches('"').to_string())
                        .filter(|f| !f.is_empty())
                        .collect();
                }
            }
            _ => {}
        }
    }
    Ok(bins)
}

/// `.data` and `.bss` sizes via the toolchain's `size`, if it's on PATH
/// (it is after `source ~/export-esp.sh`).
fn ram_sections(elf: &Path, chip: &str) -> Option<(u64, u64)> {
    let tool = format!("xtensa-{chip}-elf-size");
    let out = Command::new(tool).arg("-A").arg(elf).output().ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let section = |name: &str| {
        text.lines()
            .filter_map(|l| {
                let mut it = l.split_whitespace();
                (it.next()? == name).then(|| it.next()?.parse::<u64>().ok())?
            })
            .next()
            .unwrap_or(0)
    };
    Some((section(".data"), section(".bss")))
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let status = cmd.status().map_err(|e| format!("spawning {cmd:?}: {e}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("{cmd:?} failed"))
}

fn group(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
