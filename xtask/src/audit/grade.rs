//! Risk indices: a pure function of one crate's measurements that returns
//! a level and the reason for it. They are heuristics over proxies, not a
//! judgement of the code; each reason names the number that set the level.
//!
//! The thresholds were chosen by the project owner on 2026-11-24. Only the
//! indices with a measurable proxy exist: there is none for "vibe coding"
//! or "spaghetti", so they are not graded.

use serde::Serialize;

use super::lints::LintCounts;
use super::text::TextCounts;
use super::{RepoChecks, TextLayer};

/// Fewer `#[test]` per 1,000 lines than this is a high risk.
pub const TESTS_HIGH_BELOW: usize = 3;
/// More `#[test]` per 1,000 lines than this is a low risk.
pub const TESTS_LOW_ABOVE: usize = 6;
/// A file longer than this is a high risk; one over `BIG_FILE` is medium.
pub const FILE_HIGH: usize = 3_000;

/// The thresholds the rules use, in the report so that a reader does not
/// have to copy them from here.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Thresholds {
    /// Tests index: high below this many `#[test]` per 1,000 lines.
    pub tests_high_below_per_1000_lines: usize,
    /// Tests index: low above this many `#[test]` per 1,000 lines.
    pub tests_low_above_per_1000_lines: usize,
    /// Code smell: a file over this many lines is medium.
    pub file_medium_over: usize,
    /// Code smell: a file over this many lines is high.
    pub file_high_over: usize,
    /// Code smell: a function over this many lines is medium.
    pub function_medium_over: usize,
    /// Code smell: a function over this many lines is high.
    pub function_high_over: usize,
}

/// The thresholds in force.
pub const THRESHOLDS: Thresholds = Thresholds {
    tests_high_below_per_1000_lines: TESTS_HIGH_BELOW,
    tests_low_above_per_1000_lines: TESTS_LOW_ABOVE,
    file_medium_over: super::BIG_FILE,
    file_high_over: FILE_HIGH,
    function_medium_over: super::lints::CLIPPY_FUNCTION,
    function_high_over: super::lints::LONG_FUNCTION,
};

/// A risk level, ordered from low to high.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Low,
    Medium,
    High,
}

/// One index of one crate.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Index {
    /// The highest level any rule gave.
    pub level: Level,
    /// One sentence per measured input that a rule looked at, naming its
    /// value.
    pub reasons: Vec<String>,
    /// `false` when an input was not measured (the lint layer did not
    /// run): the level is then a lower bound.
    pub complete: bool,
}

/// The indices of one crate.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Indices {
    /// `#[test]` per 1,000 lines (text layer).
    pub tests: Index,
    /// Long files, long functions and raw-pointer allows.
    pub code_smell: Index,
    /// Documents promising what the code does not do: stale citations and
    /// decision-record citations (repository-wide), future-work markers
    /// (per crate).
    pub hallucination: Index,
}

/// Grades one crate.
pub fn grade(text: &TextLayer, lints: Option<&LintCounts>, repo: &RepoChecks) -> Indices {
    Indices {
        tests: tests(&text.counts),
        code_smell: code_smell(text, lints),
        hallucination: hallucination(repo, text.future_work_markers.len()),
    }
}

/// Collects the reasons and keeps the highest level among them.
struct Rules {
    level: Level,
    reasons: Vec<String>,
}

impl Rules {
    fn new() -> Self {
        Self { level: Level::Low, reasons: Vec::new() }
    }

    fn found(&mut self, level: Level, reason: String) {
        self.level = self.level.max(level);
        self.reasons.push(reason);
    }

    fn done(self, complete: bool) -> Index {
        Index { level: self.level, reasons: self.reasons, complete }
    }
}

/// High below [`TESTS_HIGH_BELOW`] `#[test]` per 1,000 lines, low above
/// [`TESTS_LOW_ABOVE`], medium between them, edges included.
fn tests(counts: &TextCounts) -> Index {
    let per_mille = counts.tests * 1_000;
    let level = if per_mille < TESTS_HIGH_BELOW * counts.lines {
        Level::High
    } else if per_mille > TESTS_LOW_ABOVE * counts.lines {
        Level::Low
    } else {
        Level::Medium
    };
    // One decimal, rounded down, without floating point.
    let tenths = (counts.tests * 10_000).checked_div(counts.lines).unwrap_or(0);
    let reason = format!(
        "{}.{} #[test] per 1,000 lines ({} in {}); high below {TESTS_HIGH_BELOW}, \
         low above {TESTS_LOW_ABOVE}",
        tenths / 10,
        tenths % 10,
        counts.tests,
        counts.lines
    );
    let mut rules = Rules::new();
    rules.found(level, reason);
    rules.done(true)
}

/// High with a file over [`FILE_HIGH`] lines or a function over 300;
/// medium with a file over `BIG_FILE`, a function over 100 or a
/// raw-pointer allow; low otherwise.
fn code_smell(text: &TextLayer, lints: Option<&LintCounts>) -> Index {
    let mut rules = Rules::new();
    for file in text.files_over_1200.iter().filter(|f| f.lines > FILE_HIGH) {
        rules.found(
            Level::High,
            format!("file over {FILE_HIGH} lines: {} ({})", file.path, file.lines),
        );
    }
    let medium_files = text.files_over_1200.iter().filter(|f| f.lines <= FILE_HIGH).count();
    if medium_files > 0 {
        rules
            .found(Level::Medium, format!("{medium_files} file(s) over {} lines", super::BIG_FILE));
    }
    let allows = text.counts.raw_pointer_allows;
    if allows > 0 {
        rules.found(Level::Medium, format!("{allows} not_unsafe_ptr_arg_deref allow(s)"));
    }
    let Some(lints) = lints else {
        if rules.reasons.is_empty() {
            rules
                .reasons
                .push(format!("no file over {} lines, no raw-pointer allow", super::BIG_FILE));
        }
        rules.reasons.push("function lengths not measured: run with --lints".to_owned());
        return rules.done(false);
    };
    if let Some(longest) = lints.long_functions.first() {
        rules.found(
            Level::High,
            format!(
                "{} function(s) over {} lines, longest {}:{} ({})",
                lints.functions_over_300,
                super::lints::LONG_FUNCTION,
                longest.path,
                longest.line,
                longest.lines
            ),
        );
    }
    let medium_functions = lints.functions_over_100 - lints.functions_over_300;
    if medium_functions > 0 {
        rules.found(
            Level::Medium,
            format!(
                "{medium_functions} function(s) over {} lines but not over {}",
                super::lints::CLIPPY_FUNCTION,
                super::lints::LONG_FUNCTION
            ),
        );
    }
    if rules.reasons.is_empty() {
        rules.reasons.push(format!(
            "no file over {} lines, no function over {} lines, no raw-pointer allow",
            super::BIG_FILE,
            super::lints::CLIPPY_FUNCTION
        ));
    }
    rules.done(true)
}

/// High when the repository's citation or decision-record check fails
/// (both are repository-wide, so every crate shares them); medium when the
/// crate holds future-work markers; low otherwise.
fn hallucination(repo: &RepoChecks, markers: usize) -> Index {
    let mut rules = Rules::new();
    let stale = repo.citation_problems.len();
    if stale > 0 {
        rules.found(
            Level::High,
            format!("{stale} stale citation(s) in the contract documents (repository-wide)"),
        );
    }
    let decisions = repo.decision_records.len();
    if decisions > 0 {
        rules.found(
            Level::High,
            format!("{decisions} decision-record citation(s) (repository-wide)"),
        );
    }
    if markers > 0 {
        rules.found(
            Level::Medium,
            format!("{markers} line(s) calling something future work, to be read by a person"),
        );
    }
    if rules.reasons.is_empty() {
        rules
            .reasons
            .push("citations and decision-record checks pass; no future-work marker".to_owned());
    }
    rules.done(true)
}

#[cfg(test)]
mod tests {
    use super::super::lints::{LintCounts, LongFunction};
    use super::super::text::TextCounts;
    use super::super::{FileLines, Location, Marker, RepoChecks, TextLayer};
    use super::{Level, code_smell, hallucination, tests};

    fn density(tests_count: usize, lines: usize) -> Level {
        tests(&TextCounts { tests: tests_count, lines, ..TextCounts::default() }).level
    }

    fn file(path: &str, lines: usize) -> FileLines {
        FileLines { path: path.to_owned(), lines }
    }

    fn layer(big_files: &[usize], raw_pointer_allows: usize) -> TextLayer {
        TextLayer {
            counts: TextCounts { raw_pointer_allows, ..TextCounts::default() },
            files_over_1200: big_files.iter().map(|&n| file("crates/a/src/f.rs", n)).collect(),
            ..TextLayer::default()
        }
    }

    fn functions(over_100: usize, longest: &[usize]) -> LintCounts {
        LintCounts {
            functions_over_100: over_100,
            functions_over_300: longest.len(),
            long_functions: longest
                .iter()
                .map(|&lines| LongFunction { path: "crates/a/src/f.rs".to_owned(), line: 9, lines })
                .collect(),
            ..LintCounts::default()
        }
    }

    fn repo(stale: usize, decisions: usize) -> RepoChecks {
        RepoChecks {
            citations_checked: 10,
            citation_problems: vec!["AGENTS.md:1: `x/y`: not found".to_owned(); stale],
            decision_records: (0..decisions)
                .map(|line| Location { path: "docs/x.md".to_owned(), line })
                .collect(),
            unreadable: Vec::new(),
        }
    }

    #[test]
    fn test_density_bands_include_their_edges_in_medium() {
        assert_eq!(density(29, 10_000), Level::High, "2.9 per kLoC");
        assert_eq!(density(30, 10_000), Level::Medium, "3.0 per kLoC");
        assert_eq!(density(60, 10_000), Level::Medium, "6.0 per kLoC");
        assert_eq!(density(61, 10_000), Level::Low, "6.1 per kLoC");
    }

    #[test]
    fn test_density_reason_names_the_value() {
        let index = tests(&TextCounts { tests: 55, lines: 15_618, ..TextCounts::default() });
        assert_eq!(index.level, Level::Medium);
        assert!(index.reasons[0].contains("3.5"), "{:?}", index.reasons);
        assert!(index.complete);
    }

    #[test]
    fn code_smell_is_low_only_without_any_trigger() {
        let index = code_smell(&layer(&[], 0), Some(&functions(0, &[])));
        assert_eq!(index.level, Level::Low);
        assert!(index.complete);
        assert_eq!(index.reasons.len(), 1, "one sentence saying nothing triggered");
    }

    #[test]
    fn each_medium_trigger_alone_gives_medium() {
        let none = functions(0, &[]);
        assert_eq!(code_smell(&layer(&[1_201], 0), Some(&none)).level, Level::Medium);
        assert_eq!(code_smell(&layer(&[3_000], 0), Some(&none)).level, Level::Medium);
        assert_eq!(code_smell(&layer(&[], 1), Some(&none)).level, Level::Medium);
        assert_eq!(code_smell(&layer(&[], 0), Some(&functions(1, &[]))).level, Level::Medium);
    }

    #[test]
    fn each_high_trigger_alone_gives_high() {
        let none = functions(0, &[]);
        let index = code_smell(&layer(&[3_001], 0), Some(&none));
        assert_eq!(index.level, Level::High);
        assert!(index.reasons.iter().any(|r| r.contains("crates/a/src/f.rs")), "{index:?}");
        let index = code_smell(&layer(&[], 0), Some(&functions(1, &[301])));
        assert_eq!(index.level, Level::High);
        assert!(index.reasons.iter().any(|r| r.contains("crates/a/src/f.rs:9")), "{index:?}");
    }

    #[test]
    fn without_the_lint_layer_code_smell_is_a_lower_bound() {
        let index = code_smell(&layer(&[], 0), None);
        assert_eq!(index.level, Level::Low);
        assert!(!index.complete);
        assert!(index.reasons.iter().any(|r| r.contains("--lints")), "{index:?}");
        assert_eq!(code_smell(&layer(&[], 2), None).level, Level::Medium);
    }

    #[test]
    fn repository_checks_failing_make_hallucination_high() {
        assert_eq!(hallucination(&repo(1, 0), 0).level, Level::High);
        assert_eq!(hallucination(&repo(0, 1), 0).level, Level::High);
        assert_eq!(hallucination(&repo(1, 1), 3).level, Level::High);
    }

    #[test]
    fn markers_alone_make_hallucination_medium() {
        assert_eq!(hallucination(&repo(0, 0), 0).level, Level::Low);
        let index = hallucination(&repo(0, 0), 2);
        assert_eq!(index.level, Level::Medium);
        assert!(index.reasons.iter().any(|r| r.contains('2')), "{index:?}");
    }

    #[test]
    fn grade_reads_markers_from_the_text_layer() {
        let mut text = layer(&[], 0);
        text.future_work_markers.push(Marker {
            path: "crates/a/src/lib.rs".to_owned(),
            line: 1,
            text: String::new(),
        });
        let indices = super::grade(&text, Some(&functions(0, &[])), &repo(0, 0));
        assert_eq!(indices.hallucination.level, Level::Medium);
    }
}
