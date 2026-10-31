//! A Z-machine interpreter core for story files of versions 1 to 3.
//!
//! The VM has no I/O of its own. [`Vm::run`] executes until the story needs
//! something from the outside world and returns an [`Event`]; output collects
//! in [`Vm::take_output`] for the front end to display.

mod decode;
mod dictionary;
mod exec;
pub mod header;
mod memory;
mod objects;
mod quetzal;
mod rng;
mod text;

pub use exec::Vm;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ZError {
    #[error("unsupported story file version {0} (only versions 1-3 are supported)")]
    UnsupportedVersion(u8),
    #[error("story file is too small or corrupt")]
    BadStory,
    #[error("illegal opcode {0:#04x} at {1:#07x}")]
    IllegalOpcode(u8, usize),
    #[error("memory access out of range at {0:#07x}")]
    BadAddress(usize),
    #[error("write to read-only memory at {0:#07x}")]
    ReadOnlyWrite(usize),
    #[error("routine has no local variable {0}")]
    BadVariable(u8),
    #[error("stack underflow")]
    StackUnderflow,
    #[error("stack overflow")]
    StackOverflow,
    #[error("return from main routine")]
    ReturnFromMain,
    #[error("division by zero")]
    DivisionByZero,
    #[error("invalid object {0}")]
    BadObject(u16),
    #[error("object {0} has no property {1}")]
    NoProperty(u16, u16),
    #[error("save file error: {0}")]
    BadSave(String),
    #[error("the VM is waiting for input, not running")]
    NotRunning,
}

pub type Result<T> = std::result::Result<T, ZError>;

/// Why [`Vm::run`] stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The story wants a line of input. Answer with [`Vm::provide_line`].
    NeedLine { max_len: usize },
    /// The story wants to save. Write the Quetzal data, then answer with
    /// [`Vm::provide_save_result`].
    Save(Vec<u8>),
    /// The story wants to restore. Answer with [`Vm::provide_restore`].
    Restore,
    /// The story has ended.
    Quit,
}

/// The status line shown at the top of the screen in versions 1 to 3.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusLine {
    pub location: String,
    /// "Score: 10  Moves: 42" or "Time: 9:05 am".
    pub right: String,
}

/// One thing for the front end to do, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenOp {
    /// Print text to the given window (0 = lower, 1 = upper).
    Print { window: u8, text: String },
    /// Make the upper window this many lines tall (0 removes it).
    Split(u16),
    /// Clear the upper window and move its cursor to the top left.
    ClearUpper,
    /// Move the upper window's cursor to the top left.
    UpperHome,
    /// Clear the whole screen (after a restart).
    ClearAll,
}

/// Everything produced since the last call to [`Vm::take_output`].
#[derive(Debug, Default)]
pub struct Output {
    pub ops: Vec<ScreenOp>,
    /// The latest status line, if it changed.
    pub status: Option<StatusLine>,
    /// Text for the transcript file (game has scripting switched on).
    pub transcript: String,
}

impl Output {
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty() && self.status.is_none() && self.transcript.is_empty()
    }

    /// Text printed to the lower window, concatenated. Handy for tests and
    /// the headless runner.
    pub fn lower_text(&self) -> String {
        self.ops
            .iter()
            .filter_map(|op| match op {
                ScreenOp::Print { window: 0, text } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn print(&mut self, window: u8, s: &str) {
        if let Some(ScreenOp::Print { window: w, text }) = self.ops.last_mut()
            && *w == window
        {
            text.push_str(s);
            return;
        }
        self.ops.push(ScreenOp::Print { window, text: s.to_string() });
    }
}
