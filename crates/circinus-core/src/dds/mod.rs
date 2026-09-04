//! Texture optimisation: PNG → DDS the way RimWorld loads it, with validation, a manifest and
//! a way back. See `encode` for the pixel pipeline, `validate` for the checks every file
//! passes before it is put in place, and `job` for bulk conversion, re-checking and revert.

pub mod audit;
pub mod encode;
pub mod job;
pub mod validate;

pub use encode::{Encoded, Format, Options, Quality, PARAMS_VERSION};
pub use audit::{Finding, Problem};
pub use job::{Candidate, Entry, Outcome, Plan, Progress, Revalidation, Reverted, BACKUP_SUFFIX};
