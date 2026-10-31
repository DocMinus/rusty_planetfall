use crate::{Result, ZError};

/// Story file memory: dynamic memory (writable) followed by static and high
/// memory (read-only). Keeps the original image for restart, verify and
/// save-file compression.
#[derive(Clone)]
pub struct Memory {
    bytes: Vec<u8>,
    original: Vec<u8>,
    static_base: usize,
}

impl Memory {
    pub fn new(story: Vec<u8>) -> Result<Self> {
        if story.len() < 64 {
            return Err(ZError::BadStory);
        }
        let static_base = u16::from_be_bytes([story[0x0E], story[0x0F]]) as usize;
        if static_base < 64 || static_base > story.len() {
            return Err(ZError::BadStory);
        }
        Ok(Self { bytes: story.clone(), original: story, static_base })
    }

    pub fn original(&self) -> &[u8] {
        &self.original
    }

    pub fn dynamic(&self) -> &[u8] {
        &self.bytes[..self.static_base]
    }

    pub fn original_dynamic(&self) -> &[u8] {
        &self.original[..self.static_base]
    }

    /// Replace dynamic memory wholesale (restart / restore).
    pub fn set_dynamic(&mut self, data: &[u8]) {
        self.bytes[..self.static_base].copy_from_slice(&data[..self.static_base]);
    }

    pub fn u8(&self, addr: usize) -> Result<u8> {
        self.bytes.get(addr).copied().ok_or(ZError::BadAddress(addr))
    }

    pub fn u16(&self, addr: usize) -> Result<u16> {
        Ok(u16::from_be_bytes([self.u8(addr)?, self.u8(addr + 1)?]))
    }

    pub fn set_u8(&mut self, addr: usize, value: u8) -> Result<()> {
        if addr >= self.static_base {
            return Err(ZError::ReadOnlyWrite(addr));
        }
        self.bytes[addr] = value;
        Ok(())
    }

    pub fn set_u16(&mut self, addr: usize, value: u16) -> Result<()> {
        let [hi, lo] = value.to_be_bytes();
        self.set_u8(addr, hi)?;
        self.set_u8(addr + 1, lo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Memory {
        let mut story = vec![0u8; 256];
        story[0x0F] = 128; // static memory starts at 128
        Memory::new(story).unwrap()
    }

    #[test]
    fn words_are_big_endian() {
        let mut m = mem();
        m.set_u16(100, 0x1234).unwrap();
        assert_eq!(m.u8(100).unwrap(), 0x12);
        assert_eq!(m.u16(100).unwrap(), 0x1234);
    }

    #[test]
    fn static_memory_is_read_only() {
        let mut m = mem();
        assert!(m.set_u8(127, 1).is_ok());
        assert!(matches!(m.set_u8(128, 1), Err(ZError::ReadOnlyWrite(128))));
        assert!(matches!(m.u8(256), Err(ZError::BadAddress(256))));
    }
}
