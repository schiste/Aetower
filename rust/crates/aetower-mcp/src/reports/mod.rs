//! Report builders, grouped by domain.
//!
//! Each submodule contains the `build_*` functions backing a related group
//! of MCP tools.

pub(crate) mod diagnostics;
pub(crate) mod export;
pub(crate) mod filter;
pub(crate) mod history;
pub(crate) mod persistence;
pub(crate) mod process;
pub(crate) mod runtime;
pub(crate) mod snapshot;
