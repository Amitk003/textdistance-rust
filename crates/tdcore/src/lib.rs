#![forbid(unsafe_code)]
//! Pure Rust algorithm kernels for the textdistance port.
//!
//! This crate is the port. Every string distance and similarity algorithm
//! lives here as plain, unsafe-free Rust. Nothing in this crate touches the
//! Python runtime. Kernel modules are added family by family as they are
//! ported.

pub mod compression;
pub mod edit;
pub mod sequence;
pub mod simple;
pub mod token;
