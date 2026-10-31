//! Header field offsets (Z-machine Standard, section 11).

pub const VERSION: usize = 0x00;
pub const FLAGS1: usize = 0x01;
pub const RELEASE: usize = 0x02;
pub const HIGH_MEMORY: usize = 0x04;
pub const INITIAL_PC: usize = 0x06;
pub const DICTIONARY: usize = 0x08;
pub const OBJECTS: usize = 0x0A;
pub const GLOBALS: usize = 0x0C;
pub const STATIC_MEMORY: usize = 0x0E;
pub const FLAGS2: usize = 0x10;
pub const SERIAL: usize = 0x12;
pub const ABBREVIATIONS: usize = 0x18;
pub const FILE_LENGTH: usize = 0x1A;
pub const CHECKSUM: usize = 0x1C;
pub const INTERPRETER_NUMBER: usize = 0x1E;
pub const INTERPRETER_VERSION: usize = 0x1F;
pub const SCREEN_HEIGHT: usize = 0x20;
pub const SCREEN_WIDTH: usize = 0x21;
pub const STANDARD_REVISION: usize = 0x32;

/// Flags 1, versions 1-3.
pub const F1_TIME_GAME: u8 = 0x02;
pub const F1_NO_STATUS_LINE: u8 = 0x10;
pub const F1_SPLIT_AVAILABLE: u8 = 0x20;
pub const F1_VARIABLE_PITCH_DEFAULT: u8 = 0x40;

/// Flags 2.
pub const F2_TRANSCRIPT: u8 = 0x01;
pub const F2_FIXED_PITCH: u8 = 0x02;

/// Summary of a story file's identity, as shown by `--dump`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryInfo {
    pub version: u8,
    pub release: u16,
    pub serial: String,
    pub checksum: u16,
    pub file_length: usize,
}

/// Packed addresses and file lengths scale by 2 in versions 1-3.
pub fn scale(version: u8) -> usize {
    if version <= 3 { 2 } else { 4 }
}
