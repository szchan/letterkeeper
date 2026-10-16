// SPDX-License-Identifier: GPL-3.0-only
//! Resource-free parsing. Production entry points must use the isolated worker.
mod composition;
mod html;
mod mime;
mod types;
pub use composition::*;
use mime::is_attachment;
pub use mime::{parse_message, sanitize_text};
pub use types::*;
