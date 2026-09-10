//! circinus-core: everything Circinus knows about RimWorld mods, independent of any UI.
//!
//! Clean-room implementation. Interfaces (file formats, XML tags, JSON schemas, Steam
//! endpoints) are shared with other managers so mod lists and rule databases interoperate;
//! the code is original and MIT licensed.

pub mod about;
pub mod announce;
pub mod arrivals;
pub mod cache;
pub mod changes;
pub mod clr;
pub mod dds;
pub mod defs;
pub mod fsx;
pub mod game;
pub mod harmony;
pub mod import;
pub mod loadcost;
pub mod model;
pub mod modsconfig;
pub mod order;
pub mod paths;
pub mod playerlog;
pub mod rentry;
pub mod rules;
pub mod scan;
pub mod steam;
pub mod textures;
pub mod weight;
pub mod xmlutil;

pub use model::*;

/// Errors surfaced to the shell. Kept coarse on purpose: the UI shows the message.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("XML error in {path}: {msg}")]
    Xml { path: String, msg: String },
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("network error: {0}")]
    Net(#[from] reqwest::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
