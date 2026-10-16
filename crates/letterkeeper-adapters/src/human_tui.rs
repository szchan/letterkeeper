// SPDX-License-Identifier: GPL-3.0-only
//! Human-only workbench; no instance of this capability enters MCP.
use crate::tui::TerminalGuard;
use crate::{workspace::*, *};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use letterkeeper_core::{Draft, DraftInput as CoreInput, SendDiagnostic};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use std::{io, sync::Arc};
use std::{path::PathBuf, sync::mpsc};
use tui_textarea::TextArea;

mod background;
mod drawing;
mod editor;
mod input;
mod review;
mod session;
mod state;
pub use background::*;
pub use drawing::*;
pub use editor::*;
use review::dangerous;
pub use review::*;
pub use session::*;
pub use state::*;
