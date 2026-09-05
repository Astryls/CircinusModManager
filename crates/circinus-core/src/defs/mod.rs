//! Def flattening: what the game ends up with after every active mod's Defs are merged, every
//! PatchOperation has run, and inheritance has been resolved — and, for each value, which mod
//! put it there.
//!
//! RimWorld builds one XML document from every active mod's `Defs/` folders in load order, runs
//! each mod's `Patches/` against that document (also in load order), then resolves
//! `ParentName`/`Name` inheritance. Circinus does the same thing over its own arena, recording
//! an origin on every node it touches, so "who wins this value" has an answer rather than a
//! guess.

pub mod flatten;
pub mod tree;
pub mod xpath;

pub use flatten::{flatten, Flattened, Flattener, Origin, Report};
