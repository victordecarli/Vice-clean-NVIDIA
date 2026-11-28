//! `translate --json`: a run's counts per kind of program, in the form
//! `cargo xtask progress` reads. Aggregate counts only, never a shader's
//! name, hash or an example, so the file may be committed
//! (`docs/asset-policy.md`).

use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

use super::Tally;

/// The version of the [`Summary`] format; `cargo xtask progress` refuses
/// any other. Raise it when a field changes meaning or goes away.
pub(super) const SCHEMA: u32 = 1;

#[derive(Debug, PartialEq, Serialize)]
pub(super) struct Summary {
    pub(super) schema: u32,
    /// Whether every file given was read: with one that was not, every
    /// count is a lower bound.
    pub(super) complete: bool,
    /// The widest wave the translation took the host to run
    /// (`VICTORY_HOST_WAVE`; not set, the console's 64). At 32, console
    /// wave64 compute that cannot run in waves of 32 is refused, so it is
    /// not among the validated.
    pub(super) host_lanes: u32,
    /// Vertex, pixel and the other graphics stages.
    pub(super) vertex_pixel: Kind,
    pub(super) compute: Kind,
    pub(super) mesh: Kind,
}

#[derive(Debug, PartialEq, Serialize)]
pub(super) struct Kind {
    /// Programs of this kind found, duplicates skipped.
    pub(super) found: usize,
    /// Of those, the ones emitted as DXIL that the validator accepted:
    /// a shader counts as translated only then.
    pub(super) validated: usize,
}

impl Summary {
    /// `t`'s summary, `unread` entries not read, translated for a host of
    /// `host_wave` lanes (0: not known, translated as for the console).
    pub(super) fn of(t: &Tally, unread: usize, host_wave: u32) -> Self {
        Self {
            schema: SCHEMA,
            complete: unread == 0,
            host_lanes: if host_wave == 0 { 64 } else { host_wave },
            vertex_pixel: Kind { found: t.found, validated: t.validated },
            compute: Kind { found: t.cs_found, validated: t.cs_validated },
            mesh: Kind { found: t.ms_found, validated: t.ms_validated },
        }
    }
}

/// Writes `t`'s summary to `path` as pretty JSON.
pub(super) fn write(path: &Path, t: &Tally, unread: usize) -> Result<()> {
    let [host_wave, ..] = victory_shader::emit::host_settings();
    let text = serde_json::to_string_pretty(&Summary::of(t, unread, host_wave))?;
    std::fs::write(path, text + "\n")
        .with_context(|| format!("writing the summary to {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_counts_what_the_validator_accepted() {
        let t = Tally {
            found: 10,
            validated: 6,
            cs_found: 5,
            cs_validated: 4,
            ms_found: 3,
            ms_validated: 3,
            // Stage-internal counts are not part of the summary.
            emit_programs: 8,
            emitted: 7,
            ..Tally::default()
        };
        assert_eq!(
            Summary::of(&t, 0, 32),
            Summary {
                schema: SCHEMA,
                complete: true,
                host_lanes: 32,
                vertex_pixel: Kind { found: 10, validated: 6 },
                compute: Kind { found: 5, validated: 4 },
                mesh: Kind { found: 3, validated: 3 },
            }
        );
    }

    #[test]
    fn an_unread_file_marks_it_incomplete() {
        assert!(!Summary::of(&Tally::default(), 1, 32).complete);
    }

    #[test]
    fn an_unknown_host_is_the_consoles_64_lanes() {
        assert_eq!(Summary::of(&Tally::default(), 0, 0).host_lanes, 64);
    }

    #[test]
    fn the_file_holds_counts_and_no_examples() {
        let mut t = Tally { found: 2, validated: 1, ..Tally::default() };
        t.fail("emit: x".into(), || "shaders.pak@0x10 Ps".into());
        let text = serde_json::to_string_pretty(&Summary::of(&t, 0, 64)).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["vertex_pixel"]["found"], 2);
        assert_eq!(json["schema"], SCHEMA);
        assert!(!text.contains("shaders.pak"), "{text}");
    }
}
