# victory-gpu

What is local to this crate. The root `AGENTS.md` still applies in full.

## How commands arrive

D3D12.X inlines packet writing into the title and calls the list's method
only when its put pointer reaches a limit. Our lists keep every limit at
zero, so each command is a call into a table method here. Title GPU addresses are CPU addresses (unified memory).

Every table method is title-reachable: on any failure it logs and returns
the API's failure code, or drops the draw. Shader translation failing at
pipeline creation is one such failure.

A draw that does not run on the host is counted in `DROPPED` in
`crates/victory-gpu/src/draw.rs` (read by `dropped_now` and
`take_dropped`); diagnostics builds also mark the traced draw and note its
pipeline (`dropped`). Why a draw did not run is logged where it is
decided, not recorded in one place.

## Objects and tables

Each object starts with a function-table pointer, bookkeeping bits and a
reference count (`object`); objects are static and never destroyed.
Tables come from `missing_table`, longer than any interface we know, so a
method a newer title adds logs and fails instead of jumping past the
table. Fill a slot by writing the `extern "system"` method and putting it
in that slot. Today the methods and the `object` helpers they pass
pointers to (`answer`, `is`, `hand_out`, `release_boxed`) are safe fns
that dereference raw pointers, under module-level
`not_unsafe_ptr_arg_deref` allows (see the root `AGENTS.md`, Pitfalls). A
new method that takes a pointer is `unsafe extern "system"` with a
`# Safety` section.

A method that is not written yet, or a case it skips, logs with enough
context that a run lists what to do next.

## Title memory on the host

The host GPU cannot read title memory. `shadow` keeps a host copy of each
buffer a draw uses and refreshes the ranges a submission reads from the
pages the title wrote (`watch`, fed by write watching that `victory-hle`
`xmem` sets up). Shader writes go back to title memory after the
submission. A new way for a draw to read a buffer needs its range added to
what `shadow` refreshes, or the draw reads the first copy forever.

## Linux

The crate compiles on Linux, but `draw`, `host`, `shadow`, `watch`,
`shader_cache` and the rest of the host side are `#[cfg(windows)]`. A
portable module (`list`, `resource`, `pipeline` ...) that calls into them
must be gated too, or Linux stops building (as `resource::view_format`
did). Helpers that only Windows code uses belong under `#[cfg(windows)]`:
they are why Linux clippy still excludes this crate.

## Pipelines

`crates/victory-gpu/src/pipeline/host.rs` holds a stage together with its translation from
construction (`Stage`), so nothing later has to unwrap a translation that
might be missing. Keep new pipeline state in that shape: build it fallibly
once, then carry the typed value. A host pipeline's root signature is not
built here: `root_signature` in `crates/victory-shader/src/emit/host.rs`
creates it, the same code the GPU tests use.
