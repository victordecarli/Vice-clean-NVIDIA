//! Keeps the paths a contract document cites honest: every `path` and
//! `path:line` written in inline code must name something that exists, so
//! renaming or deleting a cited file fails CI instead of leaving the
//! document stale.
//!
//! What counts as a citation (see AGENTS.md, "Citations"):
//!
//! - an inline code span outside fenced code blocks,
//! - made only of path characters (ASCII letters, digits, `_`, `-`, `.`),
//!   segments joined by `/`, at least one `/`, optionally a trailing `/` or
//!   a `:line` suffix.
//!
//! Every such span must resolve from the repository root; a path written
//! relative to some crate (`victory-gpu/src/...`) is reported like a missing
//! one, so citations cannot silently escape the check. Spans without a `/`
//! (bare file names such as `lib.rs`) and spans with glob or other
//! characters (`*.rs`, `// SAFETY:`) are not citations and are not checked.

use std::path::Path;

use anyhow::{Context, Result, bail};

/// The contract documents checked by default: the root one and the crate
/// ones it points to.
pub const CONTRACT_DOCS: [&str; 4] = [
    "AGENTS.md",
    "crates/victory-shader/AGENTS.md",
    "crates/victory-gpu/AGENTS.md",
    "crates/victory-hle/AGENTS.md",
];

/// One path cited by a document.
#[derive(Debug, PartialEq, Eq)]
pub struct Citation {
    /// 1-based line of the document the citation is on.
    pub doc_line: usize,
    /// The path as written, without any `:line` suffix.
    pub path: String,
    /// The cited line of `path`, if any.
    pub line: Option<usize>,
}

/// Checks the citations of each document (paths relative to `root`).
pub fn run(root: &Path, docs: &[String]) -> Result<()> {
    let (checked, problems) = find_problems(root, docs)?;
    if problems.is_empty() {
        println!("citations: {checked} paths ok in {}", docs.join(", "));
        return Ok(());
    }
    for p in &problems {
        eprintln!("CITATION: {p}");
    }
    bail!(
        "{} stale citation(s): cite paths from the repository root and update \
         the document in the same commit as the code (AGENTS.md, Citations)",
        problems.len()
    )
}

/// How many citations the documents hold, and one line per stale one.
pub fn find_problems(root: &Path, docs: &[String]) -> Result<(usize, Vec<String>)> {
    let mut problems = Vec::new();
    let mut checked = 0usize;
    for doc in docs {
        let text =
            std::fs::read_to_string(root.join(doc)).with_context(|| format!("reading {doc}"))?;
        for citation in extract(&text) {
            checked += 1;
            if let Some(problem) = check(root, &citation) {
                problems.push(format!(
                    "{doc}:{}: `{}`: {problem}",
                    citation.doc_line,
                    display(&citation)
                ));
            }
        }
    }
    Ok((checked, problems))
}

fn display(c: &Citation) -> String {
    match c.line {
        Some(line) => format!("{}:{line}", c.path),
        None => c.path.clone(),
    }
}

/// The citations in a Markdown document, in order. Pure, for testing.
pub fn extract(markdown: &str) -> Vec<Citation> {
    let mut found = Vec::new();
    let mut in_fence = false;
    for (index, line) in markdown.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        // Odd pieces between backticks are code spans. A span that crosses a
        // line break holds a newline, which is never a path character.
        for (piece, span) in line.split('`').enumerate() {
            if piece % 2 == 1
                && let Some((path, cited_line)) = parse(span)
            {
                found.push(Citation {
                    doc_line: index + 1,
                    path: path.to_owned(),
                    line: cited_line,
                });
            }
        }
    }
    found
}

/// `Some((path, line))` if `span` has the shape of a citation.
fn parse(span: &str) -> Option<(&str, Option<usize>)> {
    let (path, line) = match span.rsplit_once(':') {
        Some((path, digits)) => (path, Some(digits.parse::<usize>().ok()?)),
        None => (span, None),
    };
    let path_char = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/');
    let segments = path.trim_end_matches('/');
    let shaped = path.chars().all(path_char)
        && segments.contains('/')
        && segments.split('/').all(|s| !s.is_empty() && s != "." && s != "..");
    shaped.then_some((path, line))
}

/// What is wrong with one citation, if anything.
fn check(root: &Path, citation: &Citation) -> Option<String> {
    let path = root.join(&citation.path);
    let meta = match std::fs::metadata(&path) {
        Ok(meta) => meta,
        Err(e) => return Some(format!("not found from the repository root: {e}")),
    };
    if citation.path.ends_with('/') && !meta.is_dir() {
        return Some("written as a directory, but is a file".to_owned());
    }
    let line = citation.line?;
    if !meta.is_file() {
        return Some("a line of a directory".to_owned());
    }
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let lines = text.lines().count();
            (line == 0 || line > lines).then(|| format!("line {line}, but the file has {lines}"))
        }
        Err(e) => Some(format!("unreadable: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{CONTRACT_DOCS, Citation, check, extract};

    fn cite(doc_line: usize, path: &str, line: Option<usize>) -> Citation {
        Citation { doc_line, path: path.to_owned(), line }
    }

    /// Enforces the citations wherever tests run, so Windows CI checks them too.
    #[test]
    fn agents_md_citations_resolve() {
        super::run(&crate::workspace_root().unwrap(), &CONTRACT_DOCS.map(String::from)).unwrap();
    }

    #[test]
    fn extracts_paths_and_lines() {
        let doc = "See `crates/a/src/lib.rs` and `apps/b/src/c.rs:37`.\n\
                   | row | `docs/` and `xtask/src/` |\n";
        assert_eq!(
            extract(doc),
            [
                cite(1, "crates/a/src/lib.rs", None),
                cite(1, "apps/b/src/c.rs", Some(37)),
                cite(2, "xtask/src/", None),
            ]
        );
    }

    #[test]
    fn bare_names_globs_and_code_are_not_citations() {
        let doc = "`lib.rs` `Cargo.toml` `*.rs` `crates/*/src` `// SAFETY:` \
                   `unwrap`/`expect` `emit::host` `a/b c` `../x/y` `./x` `a//b` \
                   `x/y:` `x/y:3a` `foo(a, b)` `https://example.com/x`\n";
        assert_eq!(extract(doc), []);
    }

    #[test]
    fn skips_fenced_code() {
        let doc = "```sh\necho `crates/gone/x.rs`\n```\n`crates/here/y.rs`\n";
        assert_eq!(extract(doc), [cite(4, "crates/here/y.rs", None)]);
    }

    #[test]
    fn checks_against_the_tree() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let ok = |path: &str, line| check(root, &cite(1, path, line)).is_none();
        assert!(ok("xtask/src/citations.rs", None));
        assert!(ok("xtask/src/citations.rs", Some(1)));
        assert!(ok("xtask/src/", None));
        assert!(ok("xtask/src", None));
        assert!(!ok("xtask/src/gone.rs", None), "missing file");
        assert!(!ok("src/citations.rs", None), "not from the root");
        assert!(!ok("xtask/src/citations.rs", Some(0)), "line 0");
        assert!(!ok("xtask/src/citations.rs", Some(1_000_000)), "past the end");
        assert!(!ok("xtask/src", Some(1)), "line of a directory");
        assert!(!ok("xtask/Cargo.toml/", None), "file written as directory");
    }
}
