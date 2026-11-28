---
name: verify
description: Picks this repository's checks for the current change and host, runs them, and reports each command with its result. Use before calling a change done, before a commit or PR, or when asked to verify or run CI locally.
---

# Verify

The commands live in the root `AGENTS.md` (Verify) and the CI workflow;
this skill picks which apply, runs them, and reports. Done when every
command picked has exited 0 with its output shown, or the report names
each command not run and why.

## 1. Scope

List what changed: `git diff --name-only main...HEAD` plus
`git status --short` (staged, unstaged and untracked). Map each path to
its crate: `crates/<name>/` is `<name>`, `apps/victory/` is `victory`,
`xtask/` is `xtask`. Note whether the change touches Windows-only code:
anything in `victory-hle` or `apps/victory`, or hunks under
`#[cfg(windows)]` elsewhere.

Only `.md` files changed: skip to step 4.

## 2. Fast loop

For each crate touched, until both are clean:

```sh
cargo clippy -p <crate> --all-targets --locked -- -D warnings
cargo test   -p <crate> --locked
```

Off Windows, `victory-hle` and `victory` do not build and Linux clippy
excludes `victory-gpu`: use `cargo check -p victory-gpu --all-targets`
there and leave the other two to `cargo xtask wincheck` in step 3.

## 3. Full set

Run the block for this host from `AGENTS.md`, Verify: the Windows block
on Windows; the Linux block plus `cargo xtask wincheck` elsewhere. The
fast loop does not replace it: a change in a lower crate breaks the
crates above it.

## 4. Repository checks

- The grep steps of the `portable` job in `.github/workflows/ci.yml`
  (test-module gating, decision-record citations), run in bash with
  `git grep --untracked` so files not yet committed are searched too. A
  match fails the step; exit 1 (no match) passes.
- `cargo xtask citations` and `cargo xtask policy`, if step 3 did not run
  them.
- `cargo fmt --all -- --check`, a step of the `portable` job: the whole
  tree is formatted, so any diff is this change's.

## 5. Report

A table: command, exit code, the line that shows the result (`test
result: ok. 13 passed`, `asset policy: 165 files ok`). Then a **Not
run** list with the reason for each.

When Windows-only code changed and Windows tests did not run on this
host, end with the sentence the commit or PR must carry: which Windows
commands were not run, and that `wincheck` type-checked and linted the
code instead.
