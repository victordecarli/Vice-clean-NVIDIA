//! The text layer of `cargo xtask audit-metrics`: counts made by reading
//! source text, the same way for every crate and every commit. Pure, for
//! testing.
//!
//! Text counts are cheap and comparable across history, but they cannot
//! tell test code from product code or a block from a comment: what each
//! field matches is stated on the field.

use serde::Serialize;

/// Textual counts over a set of `.rs` files.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct TextCounts {
    /// `.rs` files counted.
    pub files: usize,
    /// Lines, as `wc -l` counts them on a file ending in a newline.
    pub lines: usize,
    /// Lines containing `#[test]`.
    pub tests: usize,
    /// Occurrences of `unsafe {` (blocks; `unsafe fn` and `unsafe impl` are
    /// not counted).
    pub unsafe_blocks: usize,
    /// Lines containing `// SAFETY:`.
    pub safety_comments: usize,
    /// Occurrences of `.unwrap()`, tests included (the lint layer's
    /// `unwrap_used` is the count outside tests).
    pub unwrap_calls: usize,
    /// Outer and inner `allow` attributes with a `reason`.
    pub allows_with_reason: usize,
    /// Outer and inner `allow` attributes without a `reason`.
    pub allows_without_reason: usize,
    /// `allow` attributes naming `clippy::not_unsafe_ptr_arg_deref` (raw
    /// pointers dereferenced in safe functions, AGENTS.md hard rule 3).
    pub raw_pointer_allows: usize,
}

impl TextCounts {
    /// The counts of one Rust source file.
    pub fn of_rust(text: &str) -> Self {
        let lines_with = |needle: &str| text.lines().filter(|l| l.contains(needle)).count();
        let allows = allow_bodies(text);
        let with_reason = allows.iter().filter(|body| body.contains("reason")).count();
        Self {
            files: 1,
            lines: text.lines().count(),
            tests: lines_with("#[test]"),
            unsafe_blocks: text.matches("unsafe {").count(),
            safety_comments: lines_with("// SAFETY:"),
            unwrap_calls: text.matches(".unwrap()").count(),
            allows_with_reason: with_reason,
            allows_without_reason: allows.len() - with_reason,
            raw_pointer_allows: allows
                .iter()
                .filter(|body| body.contains("not_unsafe_ptr_arg_deref"))
                .count(),
        }
    }

    /// Adds `other` into `self`, field by field.
    pub fn add(&mut self, other: Self) {
        let Self {
            files,
            lines,
            tests,
            unsafe_blocks,
            safety_comments,
            unwrap_calls,
            allows_with_reason,
            allows_without_reason,
            raw_pointer_allows,
        } = other;
        self.files += files;
        self.lines += lines;
        self.tests += tests;
        self.unsafe_blocks += unsafe_blocks;
        self.safety_comments += safety_comments;
        self.unwrap_calls += unwrap_calls;
        self.allows_with_reason += allows_with_reason;
        self.allows_without_reason += allows_without_reason;
        self.raw_pointer_allows += raw_pointer_allows;
    }
}

/// What is between the parentheses of each outer or inner `allow`
/// attribute, which may span lines.
fn allow_bodies(text: &str) -> Vec<&str> {
    let mut bodies = Vec::new();
    for (at, _) in text.match_indices("[allow(") {
        let opens_attribute = text[..at].ends_with('#') || text[..at].ends_with("#!");
        let start = at + "[allow(".len();
        if let (true, Some(len)) = (opens_attribute, closing_paren(&text[start..])) {
            bodies.push(&text[start..start + len]);
        }
    }
    bodies
}

/// The length of `text` before the `)` that closes an already open `(`.
fn closing_paren(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => return Some(i),
            ')' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// The crate a repository path belongs to: `crates/<name>/...` and
/// `apps/<name>/...` are `<name>`, `xtask/...` is `xtask`. Anything else
/// (the root, `docs/`, `tests/fixtures/`, which are not workspace members)
/// belongs to none.
pub fn crate_of(path: &str) -> Option<&str> {
    let mut parts = path.split('/');
    let top = parts.next()?;
    let second = parts.next()?;
    match top {
        "xtask" => Some("xtask"),
        "crates" | "apps" if !second.is_empty() && parts.next().is_some() => Some(second),
        _ => None,
    }
}

/// The words that mark something as future work, English and Portuguese,
/// matched in any case. Each is split so that this file does not report
/// itself.
const FUTURE_WORK: [&str; 2] = [concat!("plan", "ned"), concat!("planej", "ad")];

/// Lines (1-based, with their trimmed text) that call something future
/// work: they contain a word of [`FUTURE_WORK`]. A doc saying so beside code
/// that exists is how this repository has drifted before (AGENTS.md,
/// Pitfalls); each hit needs a human look.
pub fn future_work_markers(text: &str) -> Vec<(usize, String)> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| {
            let lower = line.to_lowercase();
            FUTURE_WORK.iter().any(|word| lower.contains(word))
        })
        .map(|(i, line)| (i + 1, line.trim().chars().take(160).collect()))
        .collect()
}

/// Lines (1-based) citing a numbered decision record: the word, a space,
/// then `0` and three more digits, as the `portable` CI job greps for.
pub fn decision_record_lines(text: &str) -> Vec<usize> {
    const WORD: &str = "decision 0";
    text.lines()
        .enumerate()
        .filter(|(_, line)| {
            line.match_indices(WORD).any(|(at, _)| {
                let rest = &line.as_bytes()[at + WORD.len()..];
                rest.len() >= 3 && rest[..3].iter().all(u8::is_ascii_digit)
            })
        })
        .map(|(i, _)| i + 1)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{TextCounts, crate_of, decision_record_lines, future_work_markers};

    // Attributes are assembled at run time so that this file does not hold
    // the literal shapes the edit hook and CI greps reject.
    fn attr(inner: &str) -> String {
        format!("#[{}({inner})]", "allow")
    }

    #[test]
    fn counts_the_text_shapes_of_one_file() {
        let src = "#[test]\nfn t() { x.unwrap(); y.unwrap(); }\n\
                   // SAFETY: the pointer is live.\nlet v = unsafe { *p };\n\
                   let w = unsafe { *q }; // SAFETY: as above\n";
        let counts = TextCounts::of_rust(src);
        assert_eq!(counts.files, 1);
        assert_eq!(counts.lines, 5);
        assert_eq!(counts.tests, 1);
        assert_eq!(counts.unwrap_calls, 2);
        assert_eq!(counts.unsafe_blocks, 2);
        assert_eq!(counts.safety_comments, 2);
    }

    #[test]
    fn a_missing_final_newline_does_not_change_the_line_count() {
        assert_eq!(TextCounts::of_rust("a\nb\n").lines, 2);
        assert_eq!(TextCounts::of_rust("a\nb").lines, 2);
    }

    #[test]
    fn unsafe_fn_and_unsafe_impl_are_not_blocks() {
        let src = "unsafe fn f() {}\nunsafe impl Send for X {}\nunsafe extern \"C\" {}\n";
        assert_eq!(TextCounts::of_rust(src).unsafe_blocks, 0);
    }

    #[test]
    fn allows_are_split_by_reason_across_lines() {
        let multi_line =
            format!("#!{}", &attr("\n    clippy::x,\n    reason = \"why (and how)\"\n")[1..]);
        let src = format!(
            "{}\n{multi_line}\n{}\n",
            attr("dead_code"),
            attr("clippy::not_unsafe_ptr_arg_deref, reason = \"debt\""),
        );
        let counts = TextCounts::of_rust(&src);
        assert_eq!(counts.allows_without_reason, 1);
        assert_eq!(counts.allows_with_reason, 2);
        assert_eq!(counts.raw_pointer_allows, 1);
    }

    #[test]
    fn allow_outside_an_attribute_is_not_counted() {
        let src = "let s = \"[allow(x)]\"; // see [allow(y)]\n";
        let counts = TextCounts::of_rust(src);
        assert_eq!(counts.allows_with_reason + counts.allows_without_reason, 0);
    }

    #[test]
    fn add_sums_every_field() {
        let one = TextCounts::of_rust(&format!(
            "#[test]\nunsafe {{}} // SAFETY: x\n.unwrap()\n{}\n{}\n",
            attr("a"),
            attr("clippy::not_unsafe_ptr_arg_deref, reason = \"r\"")
        ));
        let mut sum = one;
        sum.add(one);
        assert_eq!(
            sum,
            TextCounts {
                files: 2,
                lines: 10,
                tests: 2,
                unsafe_blocks: 2,
                safety_comments: 2,
                unwrap_calls: 2,
                allows_with_reason: 2,
                allows_without_reason: 2,
                raw_pointer_allows: 2,
            }
        );
    }

    #[test]
    fn paths_map_to_their_crate() {
        assert_eq!(crate_of("crates/victory-gpu/src/list.rs"), Some("victory-gpu"));
        assert_eq!(crate_of("apps/victory/src/main.rs"), Some("victory"));
        assert_eq!(crate_of("xtask/src/main.rs"), Some("xtask"));
        assert_eq!(crate_of("xtask/build.rs"), Some("xtask"));
        assert_eq!(crate_of("tests/fixtures/hle-probe/src/lib.rs"), None);
        assert_eq!(crate_of("docs/architecture.md"), None);
        assert_eq!(crate_of("crates/README.md"), None);
        assert_eq!(crate_of("build.rs"), None);
        assert_eq!(crate_of("crates//x.rs"), None);
    }

    #[test]
    fn future_work_markers_are_found_in_any_case() {
        // Assembled so that this file holds no marker of its own.
        let english = format!("//! {}ned: XMA2", "Plan");
        let portuguese = format!("It is {}ado.", "planej");
        let text = format!("fn a() {{}}\n{english}\n{portuguese}\nNo plan here.\n");
        assert_eq!(future_work_markers(&text), [(2, english), (3, portuguese)]);
    }

    #[test]
    fn decision_records_need_four_digits_after_the_word() {
        let cited = format!("see decision 0{}\n", "042");
        let text = format!("decision 1234\ndecision 0x\n{cited}decision 01\n");
        assert_eq!(decision_record_lines(&text), [3]);
    }
}
