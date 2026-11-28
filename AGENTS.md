# AGENTS.md

The contract for anyone changing this repository, coding agents and humans
alike. VIctory runs an Xbox Series title's x86-64 code natively on Windows:
we load its image, implement the console APIs it imports, and translate its
GPU work and shaders to host D3D12. Read `README.md`,
`docs/architecture.md` and `docs/asset-policy.md` before your first change.

Merges here are fast and review of intent cannot be assumed. CI and the
rules below are the filter, so they are strict on purpose.

## Project map

Before changing `crates/victory-shader/`, `crates/victory-gpu/` or
`crates/victory-hle/`, read that crate's `AGENTS.md`: how its code is laid
out and the gotchas local to it. This file still applies there.

| Crate | Owns | Must not contain | Builds off Windows |
|-------|------|------------------|--------------------|
| `victory-core` | config, `ensure_outside_repo`, logging setup | title-specific logic | yes |
| `victory-pe` | read-only PE32+ and import-library parsing | I/O, `unsafe`, mapping | yes (pure) |
| `victory-formats` | texture tiling/detiling, BCPack, XMA2 | I/O, logging, host APIs | yes (pure) |
| `victory-shader` | GFX10 → SSA IR → structured → DXIL | host APIs outside `emit::host`/`emit::validator` | yes (pure apart from those and its `VICTORY_*` switches) |
| `victory-pipeline` | `Converter` trait, content-addressed cache | format knowledge | yes |
| `victory-game` | GTA VI formats and build identities | anything from the game (asset policy) | yes (pure) |
| `victory-loader` | prepare, map, bind, start the title image; crash reports | API implementations | compiles; most of it is `cfg(windows)` |
| `victory-gpu` | D3D12.X objects, command lists, GPU memory → host D3D12 | shader translation logic | compiles, with dead-code warnings |
| `victory-hle` | console API implementations called by title code | format decoders (call `victory-formats`) | **no** (see Pitfalls) |
| `apps/victory` | CLI: play, run, translate, census and the cache and download commands | library logic that other crates need | no (depends on `victory-hle`) |
| `xtask` | asset-policy check, citation check, Windows-target clippy (`wincheck`), code metrics (`audit-metrics`), git hooks | product code | yes |

Dependency direction (enforced by review, keep it acyclic and downward):

- Pure crates (`victory-pe`, `victory-formats`, `victory-game`, and
  `victory-shader` outside `emit::host`/`emit::validator`) depend on no
  internal crate, do no file, network or process I/O, and must build and
  pass their tests on Linux. Tests that need real files read their paths
  from environment variables and skip when unset.
- `victory-gpu` → `victory-shader`, `victory-pipeline`, `victory-formats`.
- `victory-hle` → `victory-gpu`, `victory-loader`, `victory-pe`, `victory-formats`.
- `victory-loader` → `victory-core`, `victory-pe`; `victory-pipeline` → `victory-core`.
- Only `apps/victory` depends on `victory-hle`. Nothing depends on the app.
- Windows-only code is gated with `#[cfg(windows)]` and Windows-only
  dependencies go under `[target.'cfg(windows)'.dependencies]`. A
  portable item must never call a `cfg(windows)` one (that is how
  `victory-gpu` stopped building on Linux; see Pitfalls).

## Run logs: where to look first

To debug a run, read its log before reading code.

- `victory play` (and `victory` started with no command) writes one log
  per start, `victory-nvidia-play-<local time>-<pid>.log` under
  `%LOCALAPPDATA%\victory\logs` (the ten newest are kept), and prints its
  path at start. Its first lines are the console model, the switches and
  the mods of that start; every later line is what the console window
  showed, without colours (`log_file` and the setup after it in
  `apps/victory/src/play.rs`).
- Any other command writes a file only when `VICTORY_LOG_FILE=<path>` is
  set (`crates/victory-core/src/logging.rs`); `victory play` sets it for
  the title process it starts.
- Filter with `VICTORY_LOG` (or `RUST_LOG`). Debug and trace lines exist
  only in builds with the `diagnostics` feature.
- A log holds names and hashes from the title, so it is game-derived data
  (hard rule 1): never copy one into the repository.

## Verify

Linux (what the `portable` CI job runs; `victory-hle` and `apps/victory`
do not compile there yet):

```sh
cargo check  --workspace --all-targets --locked --exclude victory-hle --exclude victory
cargo clippy --workspace --all-targets --locked --exclude victory-hle --exclude victory --exclude victory-gpu -- -D warnings
cargo test   --workspace --locked --exclude victory-hle --exclude victory
cargo fmt --all -- --check
cargo xtask citations                    # also part of cargo test
cargo xtask wincheck                     # Windows target: type-check and lint, no tests
```

`cargo xtask wincheck` runs the two Windows clippy commands below with
`--target x86_64-pc-windows-msvc`, from any host, so it covers what the
Linux commands cannot: `cfg(windows)` code, `victory-hle` and
`apps/victory`. It needs that target's standard library (`rustup target
add x86_64-pc-windows-msvc`; a toolchain installed from
`rust-toolchain.toml` has it). It does not link or run anything, and it
enables `blake3`'s `pure` feature on its command line only, because
`blake3`'s assembly needs MSVC tools that other hosts lack. Tests and the
real Windows build still run only on Windows.

Windows (what the `check` CI job runs; the only place all code compiles):

```powershell
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked          # includes the asset-policy check
cargo xtask policy                       # also run by the pre-commit hook
```

If you can only run Linux, run `cargo xtask wincheck`: Windows code then
type-checks and lints, but its tests have not run. Say so in the commit or
PR, and do not change what `cfg(windows)` code does beyond what you can
prove by reading and by `wincheck`. Never use `#[expect(...)]` in code you
could not compile for every configuration CI builds: an unfulfilled
expectation is itself a warning, and CI runs `-D warnings`.

`cargo xtask audit-metrics [--json] [--lints]` measures and verifies
nothing: it is not in CI and none of its numbers gate a change. It prints
per-crate counts (text counts always; with `--lints`, clippy's counts
outside tests, using `wincheck`'s setup), and grades each crate by the
written rules in `xtask/src/audit/grade.rs`, giving a reason for each
level. The JSON format is documented in `xtask/src/audit.rs`.

Claude Code sessions run `.claude/hooks/check_edit.ps1` before every file
edit and shell command (configured in `.claude/settings.json`; needs
`pwsh`). It rejects what CI would fail later: `#[expect(...)]`,
`cfg(all(test, ...))`, `#[allow]` without `reason`, new
`not_unsafe_ptr_arg_deref` allows (hard rule 3), decision records cited
in the old numbered form (see Citations), and `git commit` with
`--no-verify` or `-n`. It checks only the text being written, so it
shortens the loop; it replaces none of the commands above. After each
edit of a `.rs` file, `.claude/hooks/rustfmt_edit.ps1` runs rustfmt on
that file alone if it was already formatted at `HEAD` (the whole tree
is, and CI checks it).

## Hard rules

1. **Asset policy.** Nothing from the game or any other proprietary title,
   in any form (`docs/asset-policy.md`). Tests use synthetic data our code
   generates, or paths from environment variables.
2. **`unsafe`.** Every `unsafe` block has a `// SAFETY:` comment stating the
   invariant that makes it sound, not what the block does. Every
   `unsafe fn` has a `# Safety` section.
3. **No raw-pointer dereference in a safe fn.** Make it `unsafe fn` (an
   `unsafe extern "system" fn` can still sit in a function table) or take a
   validated type. New code may not add `not_unsafe_ptr_arg_deref` allows.
4. **No `#[allow]` without `reason = "..."`** (enforced by
   `clippy::allow_attributes_without_reason`). The reason says why the
   code is right as it is, not which lint it silences.
5. **No panics on paths title code reaches.** Everything called through
   FFI (`victory-hle` exports, `victory-gpu` table methods, and what they
   call, including shader translation at pipeline creation) returns the
   API's failure code instead of `unwrap`/`expect`/`panic!`/indexing that
   can fail. A panic in an `extern "system" fn` aborts the title.
   `unwrap`/`expect` are denied outside tests everywhere
   (`clippy::unwrap_used`, `expect_used`): return the error, or restructure
   (`let ... else`, a typed value) so there is nothing to unwrap.
6. **No swallowed errors.** No `let _ = fallible()`, bare `.ok()`, or
   `filter_map(Result::ok)` without a `tracing` line saying what was
   skipped. Propagate with `?` where the caller can act on it.
7. **No `_ =>` over enums we own** when the arm means "the remaining
   cases". List the variants so adding one is a compile error.
8. **Aggregates are destructured without `..`** in merge/sum/compare code,
   so a new field cannot be silently dropped.
9. **Size.** New functions stay under ~100 lines and new files under
   ~1,200; split by responsibility before you cross either.
10. **Docs match code in the same commit.** Do not call implemented things
    "planned", or planned things done. Cite only documents that exist in
    the repository. Decision records are ADRs in the docs/adr directory,
    created when the first decision is recorded (`docs/agents/domain.md`);
    until then there are none to cite. Paths cited here are checked; see
    Citations.
11. **Lints.** Libraries log through `tracing`, never `println!`/`eprintln!`;
    no `dbg!`, `todo!`, `unimplemented!`, `std::process::exit`,
    `mem::forget`, `result.ok();`, or `unwrap`/`expect` outside tests (all
    enforced). Lints are configured
    once, in the root `Cargo.toml`; every crate has `[lints] workspace = true`.

## Citations

`cargo xtask citations` (a CI step, and the `agents_md_citations_resolve`
test) fails when a path this file or a crate `AGENTS.md` it points to
cites stops existing, so a rename or deletion has to update the contract
in the same commit. The documents are listed in `CONTRACT_DOCS`
(`xtask/src/citations.rs`); a new crate `AGENTS.md` is added there. What it
checks:

- An inline code span (outside fenced blocks) made only of letters,
  digits, `_`, `-`, `.` and at least one `/` is a citation, optionally
  ending in `/` (must be a directory) or `:line` (must be within the file).
- Citations resolve from the repository root. A path written relative to
  a crate is reported as missing, so write
  `crates/victory-gpu/src/pipeline/host.rs`, not the part after `crates/`.
- Bare file names without a `/` (such as `lib.rs` or `Cargo.toml`) are not
  checked: they name a file in the sentence's context, and many are
  ambiguous across crates. Write the full path when the file matters.
- Globs, and spans with spaces or other characters (`*.rs`,
  `// SAFETY:`), are not citations and are not checked.
- To mention a path that must not exist, say it in words, not in a code
  span.
- An ADR is cited as `ADR-NNNN` or by its path. The `portable` job fails
  on any `.rs` or `.md` line citing a decision record in the old form (the
  word "decision", a space, then a four-digit number starting with 0),
  which pointed at records the repository never held. Cite an existing
  ADR, or put the reason itself in the comment.

## Pitfalls seen in this repository

"Still here" marks a case the code holds today: fix it when you touch
that code, and update this row in the same commit.

| Symptom | Real example | Do instead |
|---------|--------------|------------|
| Portable code calls a `cfg(windows)` item; Windows CI passes, every other host fails to build | `victory-gpu` `resource::view_format` called `host_resource`, `ValueWait::holds` called `texture` and `host`, and `victory-loader` declared `crash` for every target (all fixed by gating them); still here: `victory-hle` does not build off Windows | Gate the caller too, or give the item a non-Windows body. Run the Linux commands above. |
| Tests assume the test binary is a PE image | `victory-loader` `prepare.rs` tests and four `victory-pe` `image.rs` tests reading `current_exe()`, failing the `portable` job on its first run (fixed: gated `cfg(windows)`) | Gate with `#[cfg(windows)]` and a comment saying why, or build a synthetic image (`synthetic_image` in `crates/victory-pe/src/image.rs`). |
| Safe signature, raw-pointer dereference inside | still here: 41 module-level `not_unsafe_ptr_arg_deref` allows over FFI code in `victory-gpu` and `crates/victory-hle/src/modules` | `unsafe extern "system" fn` with a `# Safety` section; a caller passes its own contract on in an `unsafe` block. Remove one module's allow per change (`/ratchet-ffi`). |
| Swallowed I/O errors | still here: `apps/victory/src/translate.rs` drops dump writes with `let _ = std::fs::write(...)` (three places); `.filter_map(Result::ok)` over `walkdir` in `apps/victory/src/translate.rs`, `apps/victory/src/shader.rs`, `apps/victory/src/replay.rs` and the non-git fallback of `xtask/src/policy.rs` | `if let Err(e) = ... { tracing::warn!(...) }`; count and report skipped entries. |
| Hand-written field-by-field merge | still here: `Tally::merge` in `apps/victory/src/translate.rs` adds each field by name, so a new field can be left out of the sum; `encode` in `crates/victory-gpu/src/shader_cache.rs` writes `HostShader` the same way | `let Tally { a, b, c } = other;` naming every field, never `..`, so a new field fails to compile until merged. |
| `_ =>` standing for "vertex/pixel" | still here: `counter()` in `apps/victory/src/translate.rs` treats every `HwStage` (8 variants) other than `Cs`/`Ms` as one group | Name the variants: `HwStage::Ls \| HwStage::Hs \| ... =>`. |
| Panic reachable from title code | `crates/victory-shader/src/emit/bitcode.rs` `panic!`ed on internal invariants, reached from `victory-gpu` pipeline creation (fixed: builder and writer invariants are recorded on the `Module` and reported by `write` as `ShaderError::Internal`); `Stage::translated` in `crates/victory-gpu/src/pipeline/host.rs` used `expect` (fixed: a `Stage` now holds its translation from construction); still here: `host!` in `crates/victory-hle/src/modules/files.rs` panics if a host export is missing | Return `Result` and map it to the API's failure (`E_FAIL`, a dropped draw) with a log line. |
| Giant functions and files | still here: `crates/victory-shader/src/emit/lower.rs` is about 13,400 lines, and clippy counts 107 functions over 100 lines outside tests (see the lint ratchet) | New logic goes in a new, focused module; extract before extending. |
| Docs promise what the code does not do (or deny what it does) | `crates/victory-formats/src/lib.rs` called BCPack and XMA2 "Planned" beside the modules implementing them; module docs cited numbered decision records the repository never held (both fixed) | Update the doc in the same commit as the code; cite only what exists. |
| A Windows-only test helper prints, Linux CI is green and Windows clippy fails | a test module gated `#[cfg(all(test, windows))]`: clippy only exempts code under a plain `#[cfg(test)]`, so its `eprintln!` fails Windows clippy | Gate test modules with `#[cfg(test)]` and `#[cfg(windows)]` as two attributes, never `cfg(all(test, ...))` (the `portable` CI job rejects it). |
| Unverifiable lint suppression | `#[allow(dead_code)] // unused in test builds` | `#[allow(dead_code, reason = "...")]`; never `#[expect]` on code CI compiles differently per cfg. |
| Counting one shader per file or container forgets the permutations and entry points it holds: the port looks complete until the title uses a combination left out | still here: `apps/victory/src/translate.rs` counts every native program of a container by stage, but `translate` in `crates/victory-shader/src/emit/mod.rs` (what a run creates pipelines from) takes only the first vertex, pixel, compute or mesh program, and the vertex/pixel count also holds LS, HS, ES and GS, which are not translated | Count entry points, each with its stage; translate and measure the same set of programs. |

## Known doc drift (to fix, not to copy)

None known. When you find a doc that contradicts the code, fix it in the
same change, or list it here with the file and what is wrong.

## Lint ratchet

Lints deliberately not enabled yet. Counts are from Windows clippy over
the whole workspace on 2026-11-27, deduplicated by place; "outside tests"
is the library and binary targets alone. Enable one only with a commit
that makes it pass, verified on Windows CI.

| Lint | Why not yet | Enable when |
|------|-------------|-------------|
| `not_unsafe_ptr_arg_deref` (deny) | 41 module-level allows (see Pitfalls) | the last allow is removed; then no allow is left to forbid |
| `clippy::let_underscore_must_use`, `let_underscore_drop` | 132 / 103 hits outside tests (163 / 126 in all) | the hits outside tests fixed, with tests exempt or fixed |
| `clippy::panic`, `unreachable` | `panic!`: 9 outside tests, 114 in tests (needs `allow-panic-in-tests` in `clippy.toml`); `unreachable!`: 47 outside tests, 58 in all (the lint has no test exemption) | `panic`: the 9 fixed, add the `clippy.toml` key and enable; `unreachable`: the remaining sites turned into `let ... else` returning their caller's error |
| `clippy::too_many_lines` | 107 hits outside tests (153 in all). `cognitive_complexity` is on, at the `clippy.toml` threshold of 50 | a `clippy.toml` threshold that passes today, then lowered step by step, as for `cognitive_complexity` |
| `clippy::wildcard_enum_match_arm` | 212 hits outside tests, and it also fires on foreign and `#[non_exhaustive]` enums, so it would force noise | rule 7 is enforced by review; reconsider with a crate-local `warn` in `apps/victory` and `victory-shader` |
| `clippy::allow_attributes` (prefer `#[expect]`) | not viable: several allows are true only on some cfgs (Windows-only code is dead on Linux, some modules dead in test builds), where `expect` would itself warn | not planned |
| `missing_docs`, `unreachable_pub`, `unused_qualifications`, `missing_debug_implementations` | 850 / 297 / 180 / 50 hits outside tests | per crate |
| `clippy::multiple_unsafe_ops_per_block`, `large_stack_arrays` | 343 / 3 hits outside tests | one crate per change, after the `not_unsafe_ptr_arg_deref` allows are gone |
