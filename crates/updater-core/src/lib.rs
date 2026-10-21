//! Core logic of GitHub Release Updater, kept free of any GUI dependency so it
//! can be unit-tested on any machine.

pub mod assets;
pub mod b2;
pub mod backup;
pub mod check;
pub mod checksum;
pub mod config;
pub mod detect;
pub mod download;
pub mod error;
pub mod github;
pub mod installer;
pub mod repo;
pub mod version;

pub use error::{Error, Result};
