//! The lint layer of `cargo xtask audit-metrics --lints`: counts clippy
//! makes for the Windows target, with `wincheck`'s setup, over library and
//! binary targets only.
//!
//! Leaving out `--all-targets` leaves out `#[cfg(test)]` code, test targets
//! and benches, so every count here is outside tests by construction (the
//! `unreachable` lint has no test exemption of its own). Both feature
//! variants run; a finding at the same place in both counts once.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use super::text::crate_of;
use crate::wincheck::{self, Variant};

/// Functions longer than this many lines count in `functions_over_100`:
/// clippy's own `too_many_lines` threshold (its default; `clippy.toml`
/// does not change it), stated here so the report can print it. Clippy
/// counts lines of code, not blank or comment lines.
pub const CLIPPY_FUNCTION: usize = 100;

/// Functions longer than this many lines count in `functions_over_300`.
pub const LONG_FUNCTION: usize = 300;

/// The lints this layer counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Lint {
    TooManyLines,
    Panic,
    Unreachable,
    UnwrapUsed,
    ExpectUsed,
}

impl Lint {
    const ALL: [Self; 5] =
        [Self::TooManyLines, Self::Panic, Self::Unreachable, Self::UnwrapUsed, Self::ExpectUsed];

    fn code(self) -> &'static str {
        match self {
            Self::TooManyLines => "clippy::too_many_lines",
            Self::Panic => "clippy::panic",
            Self::Unreachable => "clippy::unreachable",
            Self::UnwrapUsed => "clippy::unwrap_used",
            Self::ExpectUsed => "clippy::expect_used",
        }
    }
}

/// One warning clippy reported, at its primary span.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    pub lint: Lint,
    /// Repository-relative, `/`-separated.
    pub path: String,
    pub line: usize,
    pub column: usize,
    /// For `too_many_lines`, the function's length in clippy's count.
    pub size: Option<usize>,
}

/// A function over [`LONG_FUNCTION`] lines.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LongFunction {
    pub path: String,
    /// The line clippy points at (the function's signature).
    pub line: usize,
    /// Lines of code, in clippy's count.
    pub lines: usize,
}

/// Lint counts for one crate or for the workspace, all outside tests.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct LintCounts {
    /// Functions over 100 lines (`clippy::too_many_lines`), the ones over
    /// 300 included.
    pub functions_over_100: usize,
    /// Functions over 300 lines.
    pub functions_over_300: usize,
    /// Where the functions over 300 lines are, longest first.
    pub long_functions: Vec<LongFunction>,
    /// `panic!` calls (`clippy::panic`).
    pub panic: usize,
    /// `unreachable!` calls (`clippy::unreachable`).
    pub unreachable: usize,
    /// `.unwrap()` on `Option`/`Result` (`clippy::unwrap_used`).
    pub unwrap_used: usize,
    /// `.expect()` on `Option`/`Result` (`clippy::expect_used`).
    pub expect_used: usize,
}

impl LintCounts {
    fn count(&mut self, finding: &Finding) {
        match finding.lint {
            Lint::TooManyLines => {
                self.functions_over_100 += 1;
                if let Some(lines) = finding.size.filter(|&n| n > LONG_FUNCTION) {
                    self.functions_over_300 += 1;
                    self.long_functions.push(LongFunction {
                        path: finding.path.clone(),
                        line: finding.line,
                        lines,
                    });
                }
            }
            Lint::Panic => self.panic += 1,
            Lint::Unreachable => self.unreachable += 1,
            Lint::UnwrapUsed => self.unwrap_used += 1,
            Lint::ExpectUsed => self.expect_used += 1,
        }
    }

    /// Adds `other` into `self`, field by field.
    pub fn add(&mut self, other: &Self) {
        let Self {
            functions_over_100,
            functions_over_300,
            long_functions,
            panic,
            unreachable,
            unwrap_used,
            expect_used,
        } = other;
        self.functions_over_100 += functions_over_100;
        self.functions_over_300 += functions_over_300;
        self.long_functions.extend(long_functions.iter().cloned());
        self.long_functions.sort_by(|a, b| b.lines.cmp(&a.lines).then(a.path.cmp(&b.path)));
        self.panic += panic;
        self.unreachable += unreachable;
        self.unwrap_used += unwrap_used;
        self.expect_used += expect_used;
    }
}

/// What one lint run measured.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub crates: BTreeMap<String, LintCounts>,
    /// Findings whose file is in no workspace crate (a dependency's macro,
    /// for instance), counted so none disappears unreported.
    pub unattributed: usize,
}

/// Counts findings per crate. Pure, for testing.
pub fn tally<'a>(findings: impl IntoIterator<Item = &'a Finding>) -> Tally {
    let mut tally = Tally::default();
    for finding in findings {
        match crate_of(&finding.path) {
            Some(name) => tally.crates.entry(name.to_owned()).or_default().count(finding),
            None => tally.unattributed += 1,
        }
    }
    for counts in tally.crates.values_mut() {
        counts.long_functions.sort_by(|a, b| b.lines.cmp(&a.lines).then(a.path.cmp(&b.path)));
    }
    tally
}

#[derive(Deserialize)]
struct CargoMessage {
    reason: String,
    message: Option<Diagnostic>,
}

#[derive(Deserialize)]
struct Diagnostic {
    message: String,
    code: Option<Code>,
    spans: Vec<Span>,
}

#[derive(Deserialize)]
struct Code {
    code: String,
}

#[derive(Deserialize)]
struct Span {
    file_name: String,
    line_start: usize,
    column_start: usize,
    is_primary: bool,
}

/// The finding in one line of `cargo --message-format=json` output, if it
/// is a warning this layer counts. Pure, for testing.
pub fn parse(json_line: &str) -> Result<Option<Finding>, serde_json::Error> {
    let message: CargoMessage = serde_json::from_str(json_line)?;
    let Some(diagnostic) = message.message.filter(|_| message.reason == "compiler-message") else {
        return Ok(None);
    };
    let Some(code) = diagnostic.code else { return Ok(None) };
    let Some(lint) = Lint::ALL.into_iter().find(|l| l.code() == code.code) else {
        return Ok(None);
    };
    let Some(span) = diagnostic.spans.into_iter().find(|s| s.is_primary) else {
        return Ok(None);
    };
    let size = match lint {
        Lint::TooManyLines => function_size(&diagnostic.message),
        Lint::Panic | Lint::Unreachable | Lint::UnwrapUsed | Lint::ExpectUsed => None,
    };
    Ok(Some(Finding {
        lint,
        path: span.file_name.replace('\\', "/"),
        line: span.line_start,
        column: span.column_start,
        size,
    }))
}

/// `123` from clippy's "this function has too many lines (123/100)".
fn function_size(message: &str) -> Option<usize> {
    let (_, counts) = message.rsplit_once('(')?;
    let (size, _) = counts.split_once('/')?;
    size.parse().ok()
}

/// The `cargo` arguments for one lint run. Pure, for testing.
pub fn lint_args(variant: Variant) -> Vec<&'static str> {
    let mut args = wincheck::workspace_args(variant);
    args.push("--message-format=json");
    args.push("--");
    for lint in Lint::ALL {
        args.extend(["-W", lint.code()]);
    }
    args
}

/// Runs clippy for both feature variants and counts what it reports.
pub fn run(root: &Path) -> Result<Tally> {
    let mut findings = BTreeSet::new();
    let mut unparsed = 0usize;
    for variant in Variant::ALL {
        eprintln!("audit-metrics: clippy for {}, {}", wincheck::TARGET, variant.name());
        let output = wincheck::cargo(root)?
            .args(lint_args(variant))
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .output()
            .context("running cargo clippy")?;
        ensure!(
            output.status.success(),
            "cargo clippy failed ({}); the lint layer needs a workspace that compiles",
            variant.name()
        );
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            match parse(line) {
                Ok(Some(finding)) => {
                    findings.insert(finding);
                }
                Ok(None) => {}
                Err(e) => {
                    unparsed += 1;
                    eprintln!("audit-metrics: skipped a line cargo printed that is not JSON: {e}");
                }
            }
        }
    }
    ensure!(
        unparsed == 0,
        "{unparsed} line(s) of cargo output were not JSON; counts would be short"
    );
    Ok(tally(&findings))
}

#[cfg(test)]
mod tests {
    use super::{Finding, Lint, LintCounts, LongFunction, Variant, lint_args, parse, tally};
    use crate::wincheck::workspace_args;

    fn message(code: &str, text: &str, file: &str) -> String {
        format!(
            r#"{{"reason":"compiler-message","package_id":"x","message":{{"message":"{text}",
            "code":{{"code":"{code}","explanation":null}},"level":"warning",
            "spans":[{{"file_name":"other.rs","line_start":1,"column_start":1,"is_primary":false}},
            {{"file_name":"{file}","line_start":40,"column_start":5,"is_primary":true}}]}}}}"#
        )
        .replace('\n', "")
    }

    fn finding(lint: Lint, path: &str, line: usize, size: Option<usize>) -> Finding {
        Finding { lint, path: path.to_owned(), line, column: 1, size }
    }

    #[test]
    fn reads_the_primary_span_and_the_function_size() {
        let line = message(
            "clippy::too_many_lines",
            "this function has too many lines (412/100)",
            r"crates\\victory-shader\\src\\emit\\lower.rs",
        );
        assert_eq!(
            parse(&line).unwrap(),
            Some(Finding {
                lint: Lint::TooManyLines,
                path: "crates/victory-shader/src/emit/lower.rs".to_owned(),
                line: 40,
                column: 5,
                size: Some(412),
            })
        );
    }

    #[test]
    fn other_lints_and_other_messages_are_not_findings() {
        let other_lint = message("clippy::needless_return", "unneeded `return`", "xtask/src/a.rs");
        assert_eq!(parse(&other_lint).unwrap(), None);
        let artifact = r#"{"reason":"compiler-artifact","package_id":"x"}"#;
        assert_eq!(parse(artifact).unwrap(), None);
        let no_code =
            r#"{"reason":"compiler-message","message":{"message":"m","code":null,"spans":[]}}"#;
        assert_eq!(parse(no_code).unwrap(), None);
        assert!(parse("Compiling xtask").is_err(), "text is not silently a non-finding");
    }

    #[test]
    fn tally_splits_by_crate_and_keeps_long_functions() {
        let findings = [
            finding(Lint::TooManyLines, "crates/a/src/x.rs", 10, Some(150)),
            finding(Lint::TooManyLines, "crates/a/src/y.rs", 20, Some(301)),
            finding(Lint::TooManyLines, "crates/a/src/z.rs", 30, Some(900)),
            finding(Lint::Panic, "crates/a/src/x.rs", 11, None),
            finding(Lint::Unreachable, "apps/b/src/main.rs", 1, None),
            finding(Lint::UnwrapUsed, "xtask/src/main.rs", 2, None),
            finding(Lint::ExpectUsed, "xtask/src/main.rs", 3, None),
            finding(Lint::Panic, "C:/Users/x/.cargo/registry/src/dep/lib.rs", 1, None),
        ];
        let tally = tally(&findings);
        assert_eq!(tally.unattributed, 1);
        let a = &tally.crates["a"];
        assert_eq!((a.functions_over_100, a.functions_over_300, a.panic), (3, 2, 1));
        assert_eq!(
            a.long_functions,
            [
                LongFunction { path: "crates/a/src/z.rs".to_owned(), line: 30, lines: 900 },
                LongFunction { path: "crates/a/src/y.rs".to_owned(), line: 20, lines: 301 },
            ]
        );
        assert_eq!(tally.crates["b"].unreachable, 1);
        assert_eq!((tally.crates["xtask"].unwrap_used, tally.crates["xtask"].expect_used), (1, 1));
    }

    #[test]
    fn add_sums_every_field() {
        let long = LongFunction { path: "p".to_owned(), line: 1, lines: 400 };
        let one = LintCounts {
            functions_over_100: 1,
            functions_over_300: 1,
            long_functions: vec![long.clone()],
            panic: 1,
            unreachable: 1,
            unwrap_used: 1,
            expect_used: 1,
        };
        let mut sum = one.clone();
        sum.add(&one);
        assert_eq!(
            sum,
            LintCounts {
                functions_over_100: 2,
                functions_over_300: 2,
                long_functions: vec![long.clone(), long],
                panic: 2,
                unreachable: 2,
                unwrap_used: 2,
                expect_used: 2,
            }
        );
    }

    #[test]
    fn lint_runs_skip_test_code_and_only_warn() {
        for variant in Variant::ALL {
            let args = lint_args(variant);
            assert!(args.starts_with(&workspace_args(variant)), "{variant:?}");
            assert!(!args.contains(&"--all-targets"), "{variant:?}: tests would be counted");
            assert!(!args.contains(&"-D"), "{variant:?}: a warning would stop the count");
            for lint in Lint::ALL {
                assert!(args.windows(2).any(|w| w == ["-W", lint.code()]), "{lint:?}");
            }
        }
    }
}
