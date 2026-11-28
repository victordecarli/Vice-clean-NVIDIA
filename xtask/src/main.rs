//! Repository maintenance. Run with `cargo xtask <command>`.
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a command-line tool: its reports are its output"
)]

mod audit;
mod citations;
mod policy;
mod wincheck;

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xtask")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Enforce the asset policy: the repository holds only small text files
    /// we wrote. Checks tracked and untracked-but-not-ignored files.
    Policy {
        /// Check only what is staged for commit (used by the pre-commit hook).
        #[arg(long)]
        staged: bool,
    },
    /// Check that every path a contract document cites (`path`, `path:line`
    /// in inline code) exists from the repository root.
    Citations {
        /// Documents to check, relative to the repository root (default:
        /// the root and crate AGENTS.md files).
        #[arg(default_values_t = citations::CONTRACT_DOCS.map(String::from))]
        docs: Vec<String>,
    },
    /// Type-check and lint the whole workspace for x86_64-pc-windows-msvc
    /// from any host: both clippy runs of the Windows CI job, with
    /// `--target`. Does not link or run tests.
    Wincheck,
    /// Measure every workspace crate: textual counts, and with `--lints`
    /// clippy's counts outside tests for the Windows target. Measures only;
    /// gates nothing.
    AuditMetrics {
        /// Print the report as JSON (the format is documented in
        /// xtask/src/audit.rs) instead of a table.
        #[arg(long)]
        json: bool,
        /// Also run clippy (both feature variants, as `wincheck` does) to
        /// count long functions, panics and unwraps outside tests. Slow.
        #[arg(long)]
        lints: bool,
    },
    /// Point git at .githooks/ so every commit runs the policy check.
    InstallHooks,
}

fn main() -> Result<()> {
    let root = workspace_root()?;
    match Cli::parse().command {
        Cmd::Policy { staged } => policy::run(&root, staged),
        Cmd::Citations { docs } => citations::run(&root, &docs),
        Cmd::Wincheck => wincheck::run(&root),
        Cmd::AuditMetrics { json, lints } => audit::run(&root, json, lints),
        Cmd::InstallHooks => {
            let status = Command::new("git")
                .args(["config", "core.hooksPath", ".githooks"])
                .current_dir(&root)
                .status()
                .context("running git")?;
            anyhow::ensure!(status.success(), "git config failed (is this a git repository?)");
            println!("hooks installed: commits now run `cargo xtask policy --staged`");
            Ok(())
        }
    }
}

fn workspace_root() -> Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .context("xtask's manifest directory has no parent (the workspace root)")
}
