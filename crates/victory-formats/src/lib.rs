//! Platform-level data formats, shared by every Xbox Series title.
//!
//! Each module is pure (bytes in, bytes out, no I/O): [`texture`] (the
//! GPU's tiled texture layouts and copies to and from linear rows),
//! [`bcpack`] (BCPack block streams, as DirectStorage unpacks them) and
//! [`xma`] (XMA2 audio). Opus audio and the zlib streams of hardware
//! decompression are decoded in `victory-hle`, with the `opus-pure` and
//! `zlib-rs` crates.

pub mod bcpack;
pub mod texture;
pub mod xma;
