// SPDX-License-Identifier: GPL-3.0-only
//! Conservative text adapters. Host policy is fixed outside MCP.
mod browser;
mod human_tui;
pub use browser::*;
pub use human_tui::*;
mod host;
mod mcp;
mod safe_text;
mod tui;
pub mod workspace;
pub use host::*;
pub use mcp::{BoundedReader, MAX_STDIO_FRAME_BYTES, McpAdapter, run_mcp};
pub use safe_text::inert_text;
pub use tui::{UiState, render, run_tui};
