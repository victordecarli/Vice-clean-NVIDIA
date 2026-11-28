//! `cargo xtask wincheck`: type-check and lint the whole workspace for the
//! Windows target from any host, so Windows-only code (`cfg(windows)` items,
//! `victory-hle`, the app) is checked before the Windows CI job sees it.
//!
//! It runs the same two clippy commands as that job, plus `--target`. It
//! does not link or run anything: tests still run only on Windows.
//!
//! One dependency needs help. Cross-compiling to MSVC, `blake3`'s build
//! script finds no MSVC compiler, falls back to the host `cc`, and then
//! asks for MSVC's assembler (`ml64.exe`) for its SIMD code. Its `pure`
//! feature (portable Rust instead of assembly and C) skips that, and is
//! enabled here on the command line only: no manifest changes, so the real
//! Windows build still compiles the assembly. The feature changes only
//! `blake3`'s internals, which this check does not lint.

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};

/// The target the title runs on and the Windows CI job builds for.
pub const TARGET: &str = "x86_64-pc-windows-msvc";

/// One of the two clippy runs the Windows CI job makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    /// The default feature set.
    Default,
    /// `--all-features`, the diagnostics build.
    AllFeatures,
}

impl Variant {
    pub const ALL: [Self; 2] = [Self::Default, Self::AllFeatures];

    pub fn name(self) -> &'static str {
        match self {
            Self::Default => "default features",
            Self::AllFeatures => "--all-features",
        }
    }
}

/// The `cargo clippy` arguments before `--` that pick what is linted and
/// for which target, shared with `cargo xtask audit-metrics --lints`. Pure,
/// for testing.
pub fn workspace_args(variant: Variant) -> Vec<&'static str> {
    let mut args = vec![
        "clippy",
        "--workspace",
        "--locked",
        "--keep-going",
        "--target",
        TARGET,
        "--features",
        "blake3/pure",
    ];
    match variant {
        Variant::Default => {}
        Variant::AllFeatures => args.push("--all-features"),
    }
    args
}

/// The arguments for `cargo` for one clippy run. Pure, for testing.
pub fn clippy_args(variant: Variant) -> Vec<&'static str> {
    let mut args = workspace_args(variant);
    args.extend(["--all-targets", "--", "-D", "warnings"]);
    args
}

/// A `cargo` command for the workspace root, set up for the Windows target
/// from any host. Fails with the rustup hint unless the target's standard
/// library is installed.
pub fn cargo(root: &Path) -> Result<Command> {
    ensure_target(root)?;
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let mut command = Command::new(cargo);
    command
        .current_dir(root)
        // With BLAKE3_CI set, blake3's build script turns its "no MSVC
        // compiler" warning, expected here, into a failure.
        .env_remove("BLAKE3_CI");
    Ok(command)
}

/// What to tell someone whose toolchain lacks the target's standard library.
pub fn missing_target_hint() -> String {
    format!(
        "the {TARGET} standard library is not installed for this toolchain; \
         run `rustup target add {TARGET}` (rust-toolchain.toml lists it, so \
         `rustup toolchain install` adds it to a fresh toolchain)"
    )
}

/// Runs both clippy variants, even when the first fails, and reports each.
pub fn run(root: &Path) -> Result<()> {
    let mut failed = Vec::new();
    for variant in Variant::ALL {
        eprintln!("wincheck: clippy for {TARGET}, {}", variant.name());
        let status =
            cargo(root)?.args(clippy_args(variant)).status().context("running cargo clippy")?;
        if !status.success() {
            failed.push(variant.name());
        }
    }
    if failed.is_empty() {
        println!("wincheck: {TARGET} clean (type-checked and linted, not linked or tested)");
        return Ok(());
    }
    bail!("wincheck: clippy for {TARGET} failed ({})", failed.join(", "))
}

/// Fails with the rustup hint unless the target's standard library exists.
fn ensure_target(root: &Path) -> Result<()> {
    // The rustc cargo would use: $RUSTC, else the rust-toolchain.toml one.
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
    let out = Command::new(rustc)
        .args(["--print", "target-libdir", "--target", TARGET])
        .current_dir(root)
        .output()
        .context("running rustc --print target-libdir")?;
    ensure!(
        out.status.success(),
        "rustc --print target-libdir failed: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    let libdir = String::from_utf8_lossy(&out.stdout);
    ensure!(Path::new(libdir.trim()).is_dir(), missing_target_hint());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{TARGET, Variant, clippy_args, missing_target_hint, workspace_args};

    /// Arguments before and after the `--` that separates cargo's from clippy's.
    fn split(variant: Variant) -> (Vec<&'static str>, Vec<&'static str>) {
        let args = clippy_args(variant);
        let dash = args.iter().position(|a| *a == "--").expect("a `--`");
        (args[..dash].to_vec(), args[dash + 1..].to_vec())
    }

    #[test]
    fn both_variants_lint_the_whole_workspace_for_the_target() {
        for variant in Variant::ALL {
            let (cargo, clippy) = split(variant);
            assert_eq!(cargo.first(), Some(&"clippy"), "{variant:?}");
            for flag in ["--workspace", "--all-targets", "--locked", "--keep-going"] {
                assert!(cargo.contains(&flag), "{variant:?} lacks {flag}");
            }
            assert!(
                cargo.windows(2).any(|w| w == ["--target", TARGET]),
                "{variant:?} does not target {TARGET}"
            );
            assert!(!cargo.contains(&"--exclude"), "{variant:?} excludes a crate");
            assert_eq!(clippy, ["-D", "warnings"], "{variant:?}");
        }
    }

    #[test]
    fn only_the_diagnostics_variant_enables_all_features() {
        assert!(!split(Variant::Default).0.contains(&"--all-features"));
        assert!(split(Variant::AllFeatures).0.contains(&"--all-features"));
    }

    #[test]
    fn blake3_skips_its_assembly_only_through_the_command_line() {
        for variant in Variant::ALL {
            let (cargo, _) = split(variant);
            assert!(cargo.windows(2).any(|w| w == ["--features", "blake3/pure"]), "{variant:?}");
        }
    }

    #[test]
    fn clippy_runs_extend_the_shared_workspace_arguments() {
        for variant in Variant::ALL {
            let shared = workspace_args(variant);
            assert!(clippy_args(variant).starts_with(&shared), "{variant:?}");
            assert!(!shared.contains(&"--all-targets"), "{variant:?}");
            assert!(!shared.contains(&"--"), "{variant:?}");
        }
    }

    #[test]
    fn hint_names_the_rustup_command() {
        assert!(missing_target_hint().contains(&format!("rustup target add {TARGET}")));
    }
}
