//! Quetzal save files (the standard portable format, version 1.4): an IFF
//! `FORM` of type `IFZS` with `IFhd`, `CMem` and `Stks` chunks.

use crate::{Result, ZError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedFrame {
    pub return_pc: usize,
    pub store: Option<u8>,
    pub args: u8,
    pub locals: Vec<u16>,
    pub stack: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveState {
    pub release: u16,
    pub serial: [u8; 6],
    pub checksum: u16,
    pub pc: usize,
    pub dynamic: Vec<u8>,
    /// Frame 0 is the dummy frame holding the main routine's stack.
    pub frames: Vec<SavedFrame>,
}

fn bad(msg: &str) -> ZError {
    ZError::BadSave(msg.to_string())
}

pub fn write(state: &SaveState, original: &[u8]) -> Vec<u8> {
    let mut ifhd = Vec::with_capacity(13);
    ifhd.extend(state.release.to_be_bytes());
    ifhd.extend(state.serial);
    ifhd.extend(state.checksum.to_be_bytes());
    ifhd.extend(&(state.pc as u32).to_be_bytes()[1..]);

    let mut stks = Vec::new();
    for f in &state.frames {
        stks.extend(&(f.return_pc as u32).to_be_bytes()[1..]);
        stks.push(f.locals.len() as u8 | if f.store.is_none() { 0x10 } else { 0 });
        stks.push(f.store.unwrap_or(0));
        stks.push(((1u16 << f.args) - 1) as u8);
        stks.extend((f.stack.len() as u16).to_be_bytes());
        for w in f.locals.iter().chain(&f.stack) {
            stks.extend(w.to_be_bytes());
        }
    }

    let mut body = b"IFZS".to_vec();
    for (id, data) in [(b"IFhd", ifhd), (b"CMem", compress(&state.dynamic, original)), (b"Stks", stks)] {
        body.extend(id);
        body.extend((data.len() as u32).to_be_bytes());
        body.extend(&data);
        if data.len() % 2 == 1 {
            body.push(0);
        }
    }
    let mut out = b"FORM".to_vec();
    out.extend((body.len() as u32).to_be_bytes());
    out.extend(body);
    out
}

pub fn read(data: &[u8], original: &[u8]) -> Result<SaveState> {
    if data.len() < 12 || &data[0..4] != b"FORM" || &data[8..12] != b"IFZS" {
        return Err(bad("not a Quetzal save file"));
    }
    let end = (12 + u32::from_be_bytes(data[4..8].try_into().unwrap()) as usize - 4).min(data.len());
    let (mut ifhd, mut mem, mut stks) = (None, None, None);
    let mut pos = 12;
    while pos + 8 <= end {
        let id = &data[pos..pos + 4];
        let len = u32::from_be_bytes(data[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let chunk = data.get(pos + 8..pos + 8 + len).ok_or_else(|| bad("truncated chunk"))?;
        match id {
            b"IFhd" => ifhd = Some(chunk),
            b"CMem" => mem = Some(decompress(chunk, original)?),
            b"UMem" if len == original.len() => mem = Some(chunk.to_vec()),
            b"Stks" => stks = Some(chunk),
            _ => {}
        }
        pos += 8 + len + len % 2;
    }
    let ifhd = ifhd.filter(|h| h.len() >= 13).ok_or_else(|| bad("missing header chunk"))?;
    let dynamic = mem.ok_or_else(|| bad("missing memory chunk"))?;
    let frames = read_frames(stks.ok_or_else(|| bad("missing stack chunk"))?)?;
    Ok(SaveState {
        release: u16::from_be_bytes([ifhd[0], ifhd[1]]),
        serial: ifhd[2..8].try_into().unwrap(),
        checksum: u16::from_be_bytes([ifhd[8], ifhd[9]]),
        pc: u32::from_be_bytes([0, ifhd[10], ifhd[11], ifhd[12]]) as usize,
        dynamic,
        frames,
    })
}

fn read_frames(mut s: &[u8]) -> Result<Vec<SavedFrame>> {
    let word = |b: &[u8], i: usize| u16::from_be_bytes([b[i], b[i + 1]]);
    let mut frames = Vec::new();
    while !s.is_empty() {
        if s.len() < 8 {
            return Err(bad("truncated stack frame"));
        }
        let return_pc = u32::from_be_bytes([0, s[0], s[1], s[2]]) as usize;
        let (flags, var, args) = (s[3], s[4], s[5]);
        let n_locals = (flags & 0x0F) as usize;
        let n_stack = word(s, 6) as usize;
        let need = 8 + 2 * (n_locals + n_stack);
        if s.len() < need {
            return Err(bad("truncated stack frame"));
        }
        let words: Vec<u16> = (0..n_locals + n_stack).map(|i| word(s, 8 + 2 * i)).collect();
        frames.push(SavedFrame {
            return_pc,
            store: if flags & 0x10 != 0 { None } else { Some(var) },
            args: args.trailing_ones() as u8,
            locals: words[..n_locals].to_vec(),
            stack: words[n_locals..].to_vec(),
        });
        s = &s[need..];
    }
    if frames.is_empty() {
        return Err(bad("empty stack"));
    }
    Ok(frames)
}

/// XOR with the original, then run-length encode the zero bytes.
fn compress(current: &[u8], original: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut zeros = 0usize;
    for (a, b) in current.iter().zip(original) {
        let x = a ^ b;
        if x == 0 {
            zeros += 1;
            continue;
        }
        while zeros > 0 {
            let run = zeros.min(256);
            out.extend([0, (run - 1) as u8]);
            zeros -= run;
        }
        out.push(x);
    }
    out // trailing zeros are implied
}

fn decompress(data: &[u8], original: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(original.len());
    let mut i = 0;
    while i < data.len() {
        if data[i] == 0 {
            let run = *data.get(i + 1).ok_or_else(|| bad("bad memory chunk"))? as usize + 1;
            out.extend(std::iter::repeat_n(0, run));
            i += 2;
        } else {
            out.push(data[i]);
            i += 1;
        }
    }
    if out.len() > original.len() {
        return Err(bad("memory chunk is too large"));
    }
    out.resize(original.len(), 0);
    Ok(out.iter().zip(original).map(|(x, o)| x ^ o).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let original: Vec<u8> = (0..600).map(|i| (i * 7) as u8).collect();
        let mut dynamic = original.clone();
        dynamic[3] ^= 0xFF;
        dynamic[400] = 0;
        let state = SaveState {
            release: 39,
            serial: *b"880501",
            checksum: 0xA083,
            pc: 0x12345,
            dynamic,
            frames: vec![
                SavedFrame { return_pc: 0, store: None, args: 0, locals: vec![], stack: vec![1, 2] },
                SavedFrame { return_pc: 0x4000, store: Some(3), args: 2, locals: vec![9, 8, 7], stack: vec![] },
                SavedFrame { return_pc: 0x5000, store: None, args: 0, locals: vec![], stack: vec![5] },
            ],
        };
        let bytes = write(&state, &original);
        assert_eq!(read(&bytes, &original).unwrap(), state);
    }

    #[test]
    fn long_zero_runs_compress() {
        let original = vec![1u8; 1000];
        let mut current = original.clone();
        current[999] = 2;
        let c = compress(&current, &original);
        assert_eq!(decompress(&c, &original).unwrap(), current);
        assert!(c.len() < 12);
    }

    #[test]
    fn rejects_garbage() {
        assert!(read(b"hello world, not a save", &[0; 10]).is_err());
    }
}
