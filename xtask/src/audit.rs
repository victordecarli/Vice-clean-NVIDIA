//! `cargo xtask audit-metrics`: measures every workspace crate so that
//! reports about the code base come from a command, not from numbers typed
//! by hand. It measures; it gates nothing and is not part of CI.
//!
//! Two layers, and every number in the output says which one it came from
//! by where it sits:
//!
//! - `text` (always): counts made by reading the tracked `.rs` files of the
//!   working tree ([`text`]). Cheap, and the same method at every commit.
//! - `lints` (with `--lints`): clippy's counts for the Windows target, with
//!   `wincheck`'s setup, outside tests ([`lints`]). Slower: it compiles the
//!   workspace twice. `null` when not run.
//!
//! `--json` prints a [`Report`]; its fields, documented below, are the
//! contract with whatever reads it. Raise [`SCHEMA`] when a field changes
//! meaning or goes away.

mod grade;
mod lints;
mod text;

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;

use self::grade::Indices;
use self::lints::LintCounts;
use self::text::TextCounts;
use crate::citations;

/// The version of the [`Report`] format.
pub const SCHEMA: u32 = 1;

/// Files longer than this are listed (AGENTS.md hard rule 9).
pub const BIG_FILE: usize = 1_200;

/// The whole measurement.
#[derive(Debug, Serialize)]
pub struct Report {
    /// [`SCHEMA`] at the time of measuring.
    pub schema: u32,
    /// The commit measured (`git rev-parse HEAD`).
    pub commit: String,
    /// Its committer date, ISO 8601 (`git show -s --format=%cI`).
    pub commit_date: String,
    /// Whether tracked files differed from `commit`: the counts are always
    /// of the working tree.
    pub dirty: bool,
    /// The layers that ran: `"text"`, and `"lints"` with `--lints`.
    pub layers: Vec<&'static str>,
    /// Checks over the whole repository.
    pub repo: RepoChecks,
    /// One entry per workspace crate, by name.
    pub crates: Vec<CrateReport>,
    /// Sums over `crates`.
    pub totals: Totals,
    /// The thresholds the indices were graded with.
    pub rules: grade::Thresholds,
}

/// Checks that hold for the repository as a whole, not per crate.
#[derive(Debug, Serialize)]
pub struct RepoChecks {
    /// Citations `cargo xtask citations` checked in the contract documents.
    pub citations_checked: usize,
    /// Its findings, one line each; empty when it passes.
    pub citation_problems: Vec<String>,
    /// `.rs` and `.md` lines citing a numbered decision record (the
    /// `portable` CI job fails on any); empty when it passes.
    pub decision_records: Vec<Location>,
    /// Tracked files that could not be read as text, so are not counted.
    pub unreadable: Vec<String>,
}

/// A line of a file.
#[derive(Debug, Serialize)]
pub struct Location {
    /// Repository-relative path.
    pub path: String,
    /// 1-based line.
    pub line: usize,
}

/// A file and its length.
#[derive(Clone, Debug, Default, Serialize)]
pub struct FileLines {
    /// Repository-relative path.
    pub path: String,
    /// Its lines, as [`TextCounts::lines`] counts them.
    pub lines: usize,
}

/// One workspace crate.
#[derive(Debug, Serialize)]
pub struct CrateReport {
    /// The crate's directory name (`victory-gpu`, `victory` for the app,
    /// `xtask`).
    pub name: String,
    /// Its directory, repository-relative.
    pub root: String,
    /// The text layer.
    pub text: TextLayer,
    /// The lint layer, or `null` without `--lints`.
    pub lints: Option<LintCounts>,
    /// Risk levels computed from the layers above by the rules in
    /// xtask/src/audit/grade.rs, each with its reasons.
    pub indices: Indices,
}

/// A crate's text layer, before the lint layer and the grading.
struct Scanned {
    name: String,
    root: String,
    text: TextLayer,
}

/// What the text layer measured for one crate.
#[derive(Debug, Default, Serialize)]
pub struct TextLayer {
    /// The counts, as fields of this object.
    #[serde(flatten)]
    pub counts: TextCounts,
    /// `.rs` files over [`BIG_FILE`] lines, longest first.
    pub files_over_1200: Vec<FileLines>,
    /// The crate's longest `.rs` file.
    pub largest_file: FileLines,
    /// `.rs` and `.md` lines of the crate calling something future work
    /// (see `text::future_work_markers`), each to be read by a person.
    pub future_work_markers: Vec<Marker>,
}

/// A line marking something as future work.
#[derive(Debug, Serialize)]
pub struct Marker {
    /// Repository-relative path.
    pub path: String,
    /// 1-based line.
    pub line: usize,
    /// The line, trimmed and cut at 160 characters.
    pub text: String,
}

/// Sums over every crate.
#[derive(Debug, Serialize)]
pub struct Totals {
    /// The text layer's counts, summed.
    pub text: TextCounts,
    /// `.rs` files over [`BIG_FILE`] lines, in all crates.
    pub files_over_1200: usize,
    /// The lint layer's counts, summed, or `null` without `--lints`.
    pub lints: Option<LintCounts>,
    /// Lint findings in no workspace crate (a dependency's macro, for
    /// instance), or `null` without `--lints`.
    pub unattributed_lint_findings: Option<usize>,
}

/// Measures the workspace at `root` and prints the result.
pub fn run(root: &Path, json: bool, with_lints: bool) -> Result<()> {
    let report = measure(root, with_lints)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_table(&report);
    }
    Ok(())
}

fn measure(root: &Path, with_lints: bool) -> Result<Report> {
    let commit = git(root, &["rev-parse", "HEAD"])?.trim().to_owned();
    let commit_date = git(root, &["show", "-s", "--format=%cI", "HEAD"])?.trim().to_owned();
    let dirty = !git(root, &["status", "--porcelain", "--untracked-files=no"])?.trim().is_empty();
    let (scanned, mut repo) = scan_files(root)?;
    let (citations_checked, citation_problems) =
        citations::find_problems(root, &citations::CONTRACT_DOCS.map(String::from))?;
    repo.citations_checked = citations_checked;
    repo.citation_problems = citation_problems;

    let mut layers = vec!["text"];
    let mut unattributed = None;
    let mut by_crate = None;
    if with_lints {
        layers.push("lints");
        let tally = lints::run(root)?;
        unattributed = Some(tally.unattributed);
        by_crate = Some(tally.crates);
    }
    let mut crates = Vec::new();
    for Scanned { name, root, text } in scanned {
        let lints = by_crate.as_mut().map(|c| c.remove(&name).unwrap_or_default());
        let indices = grade::grade(&text, lints.as_ref(), &repo);
        crates.push(CrateReport { name, root, text, lints, indices });
    }
    // Every finding's crate comes from a tracked file, so none is left.
    if let Some(left) = by_crate.filter(|c| !c.is_empty()) {
        bail!("lint findings in crates with no tracked file: {:?}", left.keys());
    }
    let totals = totals(&crates, unattributed);
    Ok(Report {
        schema: SCHEMA,
        commit,
        commit_date,
        dirty,
        layers,
        repo,
        crates,
        totals,
        rules: grade::THRESHOLDS,
    })
}

/// Reads every tracked file once: text counts and markers per crate,
/// decision-record citations everywhere.
fn scan_files(root: &Path) -> Result<(Vec<Scanned>, RepoChecks)> {
    let listing = git(root, &["ls-files", "-z"])?;
    let mut crates: BTreeMap<String, Scanned> = BTreeMap::new();
    let mut repo = RepoChecks {
        citations_checked: 0,
        citation_problems: Vec::new(),
        decision_records: Vec::new(),
        unreadable: Vec::new(),
    };
    for path in listing.split('\0').filter(|p| p.ends_with(".rs") || p.ends_with(".md")) {
        let content = match std::fs::read_to_string(root.join(path)) {
            Ok(content) => content,
            Err(e) => {
                eprintln!("audit-metrics: not counted, cannot read {path}: {e}");
                repo.unreadable.push(path.to_owned());
                continue;
            }
        };
        for line in text::decision_record_lines(&content) {
            repo.decision_records.push(Location { path: path.to_owned(), line });
        }
        let Some(name) = text::crate_of(path) else { continue };
        let krate = crates.entry(name.to_owned()).or_insert_with(|| Scanned {
            name: name.to_owned(),
            root: crate_root(path, name).to_owned(),
            text: TextLayer::default(),
        });
        add_file(&mut krate.text, path, &content);
    }
    for krate in crates.values_mut() {
        krate.text.files_over_1200.sort_by_key(|f| std::cmp::Reverse(f.lines));
    }
    Ok((crates.into_values().collect(), repo))
}

/// `crates/victory-gpu` for `crates/victory-gpu/src/list.rs`.
fn crate_root<'a>(path: &'a str, name: &str) -> &'a str {
    path.find(name).map_or(path, |at| &path[..at + name.len()])
}

fn add_file(layer: &mut TextLayer, path: &str, content: &str) {
    for (line, text) in text::future_work_markers(content) {
        layer.future_work_markers.push(Marker { path: path.to_owned(), line, text });
    }
    if !path.ends_with(".rs") {
        return;
    }
    let counts = TextCounts::of_rust(content);
    layer.counts.add(counts);
    let file = FileLines { path: path.to_owned(), lines: counts.lines };
    if file.lines > BIG_FILE {
        layer.files_over_1200.push(file.clone());
    }
    if file.lines > layer.largest_file.lines {
        layer.largest_file = file;
    }
}

fn totals(crates: &[CrateReport], unattributed_lint_findings: Option<usize>) -> Totals {
    let mut text = TextCounts::default();
    let mut lints: Option<LintCounts> = None;
    let mut files_over_1200 = 0;
    for krate in crates {
        text.add(krate.text.counts);
        files_over_1200 += krate.text.files_over_1200.len();
        if let Some(counts) = &krate.lints {
            lints.get_or_insert_with(LintCounts::default).add(counts);
        }
    }
    Totals { text, files_over_1200, lints, unattributed_lint_findings }
}

pub(crate) fn git(root: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .with_context(|| format!("running git {}", args.join(" ")))?;
    ensure!(
        out.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr).trim()
    );
    String::from_utf8(out.stdout).context("git printed something that is not UTF-8")
}

fn print_table(report: &Report) {
    let dirty = if report.dirty { " (working tree differs)" } else { "" };
    println!("audit-metrics at {}{dirty}, layers: {}", report.commit, report.layers.join(", "));
    println!(
        "{:<18} {:>7} {:>6} {:>7} {:>7} {:>5} {:>6}  largest file",
        "crate", "lines", "tests", "unsafe", "SAFETY", "raw", ">1200"
    );
    for krate in &report.crates {
        let t = &krate.text;
        println!(
            "{:<18} {:>7} {:>6} {:>7} {:>7} {:>5} {:>6}  {} ({})",
            krate.name,
            t.counts.lines,
            t.counts.tests,
            t.counts.unsafe_blocks,
            t.counts.safety_comments,
            t.counts.raw_pointer_allows,
            t.files_over_1200.len(),
            t.largest_file.path,
            t.largest_file.lines
        );
    }
    println!(
        "risk indices (rules in xtask/src/audit/grade.rs; * = lower bound, run with --lints):"
    );
    for krate in &report.crates {
        let i = &krate.indices;
        let partial = if i.code_smell.complete { "" } else { "*" };
        println!(
            "{:<18} tests {:?}, code smell {:?}{partial}, hallucination {:?}",
            krate.name, i.tests.level, i.code_smell.level, i.hallucination.level
        );
    }
    if let Some(l) = &report.totals.lints {
        println!(
            "lints, outside tests: fn>100 {}, fn>300 {}, panic {}, unreachable {}, unwrap {}, expect {}",
            l.functions_over_100,
            l.functions_over_300,
            l.panic,
            l.unreachable,
            l.unwrap_used,
            l.expect_used
        );
    }
    let repo = &report.repo;
    println!(
        "citations: {} checked, {} stale; decision-record citations: {}",
        repo.citations_checked,
        repo.citation_problems.len(),
        repo.decision_records.len()
    );
}

#[cfg(test)]
mod tests {
    use super::{TextLayer, add_file, crate_root};

    #[test]
    fn crate_roots_are_the_directory_of_the_crate() {
        assert_eq!(
            crate_root("crates/victory-gpu/src/list.rs", "victory-gpu"),
            "crates/victory-gpu"
        );
        assert_eq!(crate_root("apps/victory/src/main.rs", "victory"), "apps/victory");
        assert_eq!(crate_root("xtask/src/main.rs", "xtask"), "xtask");
    }

    #[test]
    fn files_add_up_and_the_longest_is_kept() {
        let mut layer = TextLayer::default();
        add_file(&mut layer, "crates/a/src/small.rs", "a\n");
        add_file(&mut layer, "crates/a/src/big.rs", &"x\n".repeat(1_201));
        add_file(&mut layer, "crates/a/README.md", &"y\n".repeat(5_000));
        assert_eq!((layer.counts.files, layer.counts.lines), (2, 1_202));
        assert_eq!(layer.largest_file.path, "crates/a/src/big.rs");
        assert_eq!(layer.files_over_1200.len(), 1, "Markdown is not source");
    }
}
