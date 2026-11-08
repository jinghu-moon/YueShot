//! YueShot-owned file persistence for screenshot artifacts and source-format masters.

pub mod artifact_store;
pub mod capture_history;
pub mod error;
pub mod pixel_master;

pub use error::StorageError;
