//! xlsx2csv-rs — Rust port of the `xlsx2csv` CLI (MIT, upstream 0.8.6).
//!
//! Faithful port target: byte-identical CSV for the supported option subset.
//! Upstream source is the only authority; where upstream behaves oddly, this
//! crate copies the behaviour and the oddity is written down in
//! `docs/UPSTREAM-QUIRKS.md`.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod cli;
pub mod csvw;
pub mod dates;
pub mod encoding;
pub mod errors;
pub mod fmtval;
pub mod ooxml;
pub mod pattern;
pub mod port;
pub mod sheet;
pub mod sst;
pub mod styles;

/// Upstream release this port tracks.
pub const UPSTREAM_VERSION: &str = "0.8.6";
/// Upstream commit this port tracks (anchor for byte-parity goldens).
pub const UPSTREAM_COMMIT: &str = "ec1883bb438ee4baf05953695009b9b6e9cad966";

/// This crate's version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
