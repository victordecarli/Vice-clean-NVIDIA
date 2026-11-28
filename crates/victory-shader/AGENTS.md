# victory-shader

What is local to this crate. The root `AGENTS.md` still applies in full.

## Stages

`translate` in `emit` runs the whole pipeline:

```text
dxbc / dxil   container, XDAT table, zlib-compressed GFX10 code
ir::build     GFX10 decode (gfx10, native) to SSA
structure     EXEC and lane masks become per-lane booleans; unproven EXEC stays a per-lane guard
resource      where each descriptor and address comes from
io            inputs and outputs raised from the native layout
emit          DXIL in a PC container (SM 6.6) plus the HostShader binding contract
```

`emit`'s module doc is the binding contract `victory-gpu` relies on (user
data, descriptor heaps, static samplers, root CBVs/SRVs, vertex fetch).
Change it in the same commit as the code it describes.

## Purity

No I/O outside `emit::host` (Windows, test or feature `host`; reads
`VICTORY_WARP`) and `emit::validator` (Windows; reads `VICTORY_DXIL`).
Code that needs D3D12, DXC or the validator goes in those two modules;
everything else builds and tests on Linux.

The one exception is environment switches: about thirty `VICTORY_*`
variables are read in `crates/victory-shader/src/emit/mod.rs` and
`crates/victory-shader/src/emit/lower.rs` (translation options and
`..._WHY` explanations), each once through a `OnceLock`. A new switch
follows that shape; a translation must not depend on anything else of
the process.

## Failure is a value

Translation runs inside `victory-gpu` pipeline creation, on the title's
call, so every failure is a `ShaderError`:

- `Unsupported { offset, what }`: valid code we do not translate yet.
- `Isa`, `Malformed`, `NotDxbc`: bad input.
- `Internal`: we broke our own invariant. The bitcode builder records such
  breaks on the `Module` and `write` returns them; follow that shape.

Inside `lower`, a value the emitter cannot express is `Missing` with a
reason. It fails the program only when something with an effect (an
export, a branch) needs it, so an unused unsupported value costs nothing.

## lower.rs

`crates/victory-shader/src/emit/lower.rs` holds all of the lowering in
one file of about 13,400 lines: the `Lower` struct, the dispatch of each
instruction (`inst`) and every instruction's lowering. Splitting it into
modules of one kind each is planned; until then, put a new lowering in a
method of its own beside those of its kind (the `valu_*`, `image_*` and
`buffer_*` methods), not inline in the dispatch. An `unreachable!` you
touch there becomes a `let ... else` returning the lowering's error.

## Tests

- Portable unit tests sit beside the code under `#[cfg(test)]`.
- The `*_tests` modules of `emit` (`diff_tests`, `valu_tests`,
  `waterfall_tests`, `wave64_tests` and the others) run translated
  shaders on the host GPU: Windows only, gated `#[cfg(test)]` plus
  `#[cfg(windows)]`.
- `diff_tests` compile each HLSL case with the console and the PC
  compiler and compare the two. They skip unless
  `VICTORY_CONSOLE_DXC` names the console compiler; `VICTORY_PC_DXC`
  overrides the PC one (default: the `dxc.exe` beside the validator),
  `VICTORY_DIFF_ONLY` filters cases, `VICTORY_EMIT_DUMP` names a directory
  for emitted modules, `VICTORY_WARP=1` runs on WARP.

## HostShader is persisted

`victory-gpu`'s cache (`encode` and `decode` in
`crates/victory-gpu/src/shader_cache.rs`) stores `HostShader` in an
encoding of its own, field by field. It reads the fields by name, so a
field added to `HostShader` is silently not stored (hard rule 8): add it
to both functions in the same commit, and change the format's `MAGIC` so
old entries are translated again.
