//! # The Cobalt Async wrapper library
//!
//! This library provides a collection of helpful functions for working with async Rust.
//!
//! * [Changelog](https://github.com/harrison-ai/cobalt-async/blob/main/CHANGELOG.md)
//!
//! ### About harrison.ai
//!
//! This crate is maintained by the Data Engineering team at [harrison.ai](https://harrison.ai).
//!
//! At [harrison.ai](https://harrison.ai) our mission is to create AI-as-a-medical-device solutions through
//! ventures and ultimately improve the standard of healthcare for 1 million lives every day.
//!
//!
#![cfg_attr(docsrs, feature(doc_cfg))]
#[cfg(feature = "checksum")]
#[cfg_attr(docsrs, doc(cfg(feature = "checksum")))]
pub mod checksum;
mod chunker;
pub mod counter;
mod try_finally;

pub use chunker::{apply_chunker, try_apply_chunker, ChunkResult};
pub use try_finally::try_finally;
