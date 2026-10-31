//! Dictionary lookup and input tokenising (Standard, section 13).

use crate::Result;
use crate::memory::Memory;
use crate::text;

pub struct Dictionary {
    separators: Vec<u8>,
    entry_len: usize,
    entries_start: usize,
    count: usize,
}

impl Dictionary {
    pub fn new(mem: &Memory, addr: usize) -> Result<Self> {
        let n = mem.u8(addr)? as usize;
        let separators = (0..n).map(|i| mem.u8(addr + 1 + i)).collect::<Result<_>>()?;
        let entry_len = mem.u8(addr + 1 + n)? as usize;
        // A negative count means "unsorted"; the magnitude is what matters.
        let count = (mem.u16(addr + 2 + n)? as i16).unsigned_abs() as usize;
        Ok(Self { separators, entry_len, entries_start: addr + 4 + n, count })
    }

    /// Address of the entry for this encoded word, or 0.
    fn lookup(&self, mem: &Memory, encoded: &[u8]) -> Result<u16> {
        for i in 0..self.count {
            let addr = self.entries_start + i * self.entry_len;
            if (0..encoded.len()).all(|k| mem.u8(addr + k).is_ok_and(|b| b == encoded[k])) {
                return Ok(addr as u16);
            }
        }
        Ok(0)
    }

    /// Split `input` (lower-case ZSCII, as stored in the text buffer starting at
    /// buffer offset `offset`) into words and write the parse buffer.
    pub fn tokenise(&self, mem: &mut Memory, input: &[u8], offset: usize, parse: usize, version: u8) -> Result<()> {
        let max = mem.u8(parse)? as usize;
        let mut words: Vec<(usize, usize)> = Vec::new(); // (start, len)
        let mut start = None;
        for (i, &c) in input.iter().enumerate() {
            let is_sep = self.separators.contains(&c);
            if c == b' ' || is_sep {
                if let Some(s) = start.take() {
                    words.push((s, i - s));
                }
                if is_sep {
                    words.push((i, 1));
                }
            } else if start.is_none() {
                start = Some(i);
            }
        }
        if let Some(s) = start {
            words.push((s, input.len() - s));
        }
        words.truncate(max);

        mem.set_u8(parse + 1, words.len() as u8)?;
        for (n, &(s, len)) in words.iter().enumerate() {
            let encoded = text::encode(&input[s..s + len], version);
            let entry = self.lookup(mem, &encoded)?;
            let at = parse + 2 + 4 * n;
            mem.set_u16(at, entry)?;
            mem.set_u8(at + 2, len as u8)?;
            mem.set_u8(at + 3, (s + offset) as u8)?;
        }
        Ok(())
    }
}
