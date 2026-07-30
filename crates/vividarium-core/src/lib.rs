//! Backend services and serializable contracts shared by Vividarium clients.
//!
//! Feature APIs are grouped by domain. Callers create a [`Database`] through
//! [`storage`] and pass it to functions in [`photos`], [`mapping`],
//! [`taxonomy`], [`naming`], or [`map`].

mod db;
pub mod error;
pub mod map;
pub mod mapping;
mod metadata;
pub mod models;
pub mod naming;
pub mod operations;
pub mod photos;
pub mod storage;
pub mod taxonomy;

pub use error::{CoreError, CoreResult};
pub use models::*;
pub use storage::Database;
