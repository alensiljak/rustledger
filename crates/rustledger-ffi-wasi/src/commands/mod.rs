//! Command implementations for the FFI-WASI module.
//!
//! Commands are organized by functionality:
//! - `load` - Loading and parsing beancount files
//! - `query` - BQL query execution
//! - `format` - Formatting beancount source
//! - `validate` - Validation
//! - `entry` - Entry creation and manipulation
//! - `clamp` - Date range filtering
//! - `util` - Utility commands

pub mod clamp;
pub mod entry;
pub mod format;
pub mod load;
pub mod query;
pub mod util;
pub mod validate;
