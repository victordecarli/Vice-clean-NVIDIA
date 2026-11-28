# victory-hle

What is local to this crate. The root `AGENTS.md` still applies in full.

## Adding an export

Write an `extern "system"` function named exactly like the import, then
list it with `hle_export!` in its module's `module()`. A new module is
added to `all()` in `crates/victory-hle/src/modules/mod.rs`, under `#[cfg(windows)]` there if
the module is. `victory census` over a title shows what is still missing.

Resolution order for every import: an export here, then the host if
`HostPolicy` passes that DLL through, then the loader's unimplemented
stub. A DLL that is passed through can still be reached by
`LoadLibrary`/`GetProcAddress`, so an override has to cover that path
too: `kernel`'s `GetProcAddress` serves our modules' handles from their
exports and checks `host_override` before asking the host for a real one.

## Calling the host

Host exports are reached through `host!` in
`crates/victory-hle/src/modules/files.rs`, which panics when the host
lacks the export: a panic on a title-reachable path (hard rule 5). Turning
it into a macro that yields `None`, so each caller returns its API's
failure, is planned; new code does not add calls to `host!` meanwhile
without saying so in the PR.
Host pass-through loads only from System32; `crates/victory-hle/tests/no_dll_planting.rs`
guards that.

## Interfaces

Runtime interfaces (`xgameruntime` and its neighbours) are static
`Object`s with vtables, listed in each module's `INTERFACES` by provider
GUID and interface id. Write results through `put`, read strings through
`text`; both handle the title's raw pointers in one place.

## Policy built in

- Offline by design: `net` lets the title reach loopback only, and vets
  every Winsock and WinHTTP entry point a title can import or look up.
- Decoding belongs to `victory-formats`, GPU objects to `victory-gpu`
  (`d3d12x` sets up virtual memory, creates the device and serializes
  root signatures; everything the device creates lives there).

## Builds

The crate does not compile on Linux. Check it with `cargo xtask wincheck`
and say in the PR when Windows tests have not run. The modules marked
`#[cfg(windows)]` in `crates/victory-hle/src/modules/mod.rs` are Windows-only; a portable module
that uses one (as `d3d12x`, `xgame`, `xgameui` do today) is what keeps
Linux from building, so new code gates the caller or gives the item a
non-Windows body.

## FFI debt

27 modules here carry a module-level `not_unsafe_ptr_arg_deref` allow:
their exports are safe fns that dereference the title's pointers, and so
are `put`, `put_string` and `text` in
`crates/victory-hle/src/modules/xgameruntime.rs`. Removing them, one
module per change, is planned. New code may not add an allow (hard
rule 3): a new function that takes a title pointer is
`unsafe extern "system"` with a `# Safety` section, and the contract
belongs on the function the title calls, which passes it on in an
`unsafe` block.
