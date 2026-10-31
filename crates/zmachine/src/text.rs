//! Z-character strings and ZSCII (Standard, section 3).

use crate::Result;
use crate::memory::Memory;

const A0: &[u8; 26] = b"abcdefghijklmnopqrstuvwxyz";
const A1: &[u8; 26] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
/// A2 from z-char 7 onward. Version 1 has '<' and no newline.
const A2_V1: &[u8; 25] = b"0123456789.,!?_#'\"/\\<-:()";
const A2: &[u8; 25] = b"\n0123456789.,!?_#'\"/\\-:()";

/// ZSCII 155-223: the default extra-characters table (Standard 3.8.5.3).
const EXTRA: [char; 69] = [
    'ä', 'ö', 'ü', 'Ä', 'Ö', 'Ü', 'ß', '»', '«', 'ë', 'ï', 'ÿ', 'Ë', 'Ï', 'á', 'é', 'í', 'ó', 'ú',
    'ý', 'Á', 'É', 'Í', 'Ó', 'Ú', 'Ý', 'à', 'è', 'ì', 'ò', 'ù', 'À', 'È', 'Ì', 'Ò', 'Ù', 'â', 'ê',
    'î', 'ô', 'û', 'Â', 'Ê', 'Î', 'Ô', 'Û', 'å', 'Å', 'ø', 'Ø', 'ã', 'ñ', 'õ', 'Ã', 'Ñ', 'Õ', 'æ',
    'Æ', 'ç', 'Ç', 'þ', 'ð', 'Þ', 'Ð', '£', 'œ', 'Œ', '¡', '¿',
];

/// ZSCII code to a printable char. `None` for codes with no output.
pub fn zscii_to_char(code: u16) -> Option<char> {
    match code {
        13 => Some('\n'),
        32..=126 => Some(code as u8 as char),
        155..=223 => Some(EXTRA[(code - 155) as usize]),
        0 => None,
        _ => Some('?'),
    }
}

/// A typed char to ZSCII, for input. `None` if it has no ZSCII equivalent.
pub fn char_to_zscii(c: char) -> Option<u8> {
    match c {
        ' '..='~' => Some(c as u8),
        _ => EXTRA.iter().position(|&e| e == c).map(|i| 155 + i as u8),
    }
}

/// Decode the z-string at `addr`. Returns the text and the address just past it.
pub fn decode(mem: &Memory, addr: usize, version: u8, abbreviations: usize) -> Result<(String, usize)> {
    let (zchars, end) = read_zchars(mem, addr)?;
    let mut out = String::new();
    decode_zchars(mem, &zchars, version, abbreviations, true, &mut out)?;
    Ok((out, end))
}

fn read_zchars(mem: &Memory, mut addr: usize) -> Result<(Vec<u8>, usize)> {
    let mut zchars = Vec::new();
    loop {
        let word = mem.u16(addr)?;
        addr += 2;
        zchars.extend([(word >> 10) as u8 & 0x1F, (word >> 5) as u8 & 0x1F, word as u8 & 0x1F]);
        if word & 0x8000 != 0 {
            return Ok((zchars, addr));
        }
    }
}

fn decode_zchars(
    mem: &Memory,
    zchars: &[u8],
    version: u8,
    abbreviations: usize,
    allow_abbrev: bool,
    out: &mut String,
) -> Result<()> {
    let mut lock = 0u8; // shift-locked alphabet (versions 1-2 only)
    let mut alphabet = 0u8; // alphabet for the next z-char
    let mut i = 0;
    while i < zchars.len() {
        let z = zchars[i];
        i += 1;
        let current = alphabet;
        alphabet = lock;
        match z {
            0 => out.push(' '),
            1 if version == 1 => out.push('\n'),
            1..=3 if version >= 3 || (version == 2 && z == 1) => {
                let Some(&x) = zchars.get(i) else { break };
                i += 1;
                if allow_abbrev {
                    let entry = abbreviations + 2 * (32 * (z as usize - 1) + x as usize);
                    let (sub, _) = read_zchars(mem, mem.u16(entry)? as usize * 2)?;
                    decode_zchars(mem, &sub, version, abbreviations, false, out)?;
                }
            }
            2 | 3 if version <= 2 => alphabet = (lock + z - 1) % 3,
            4 | 5 if version <= 2 => {
                lock = (lock + z - 3) % 3;
                alphabet = lock;
            }
            4 | 5 => alphabet = z - 3,
            6 if current == 2 => {
                let (Some(&hi), Some(&lo)) = (zchars.get(i), zchars.get(i + 1)) else { break };
                i += 2;
                if let Some(c) = zscii_to_char(((hi as u16) << 5) | lo as u16) {
                    out.push(c);
                }
            }
            _ => {
                let idx = z as usize - 6;
                out.push(match current {
                    0 => A0[idx] as char,
                    1 => A1[idx] as char,
                    _ if version == 1 => A2_V1[idx - 1] as char,
                    _ => A2[idx - 1] as char,
                });
            }
        }
    }
    Ok(())
}

/// Encode a (lower-case ZSCII) word for dictionary lookup: 6 z-chars in
/// versions 1-3, packed into 4 bytes.
pub fn encode(word: &[u8], version: u8) -> Vec<u8> {
    let len = if version <= 3 { 6 } else { 9 };
    let a2 = if version == 1 { &A2_V1[..] } else { &A2[..] };
    let a2_shift = if version <= 2 { 3 } else { 5 };
    let mut z = Vec::with_capacity(len + 3);
    for &c in word {
        if let Some(i) = A0.iter().position(|&a| a == c) {
            z.push(i as u8 + 6);
        } else if let Some(i) = a2.iter().position(|&a| a == c).filter(|_| c != b'\n') {
            z.extend([a2_shift, i as u8 + 7]);
        } else {
            z.extend([a2_shift, 6, c >> 5, c & 0x1F]);
        }
        if z.len() >= len {
            break;
        }
    }
    z.resize(len, 5);
    let mut bytes = Vec::with_capacity(len / 3 * 2);
    for (n, chunk) in z.chunks(3).enumerate() {
        let mut word = ((chunk[0] as u16) << 10) | ((chunk[1] as u16) << 5) | chunk[2] as u16;
        if n == len / 3 - 1 {
            word |= 0x8000;
        }
        bytes.extend(word.to_be_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny fake story: an encoded string at 0x40, static memory after it.
    fn story_with(bytes: &[u8]) -> Memory {
        let mut story = vec![0u8; 0x100];
        story[0x0F] = 0x80;
        story[0x40..0x40 + bytes.len()].copy_from_slice(bytes);
        Memory::new(story).unwrap()
    }

    #[test]
    fn encode_then_decode_round_trips() {
        for word in ["lamp", "floyd", "n", "kalamontee"] {
            let enc = encode(word.as_bytes(), 3);
            assert_eq!(enc.len(), 4);
            let (text, end) = decode(&story_with(&enc), 0x40, 3, 0).unwrap();
            let expect: String = word.chars().take(6).collect();
            assert_eq!(text, expect);
            assert_eq!(end, 0x44);
        }
    }

    #[test]
    fn punctuation_uses_a2_and_escape() {
        let enc = encode(b"a.b", 3);
        let (text, _) = decode(&story_with(&enc), 0x40, 3, 0).unwrap();
        assert_eq!(text, "a.b");
        // '@' is not in any alphabet: shift, escape, two 5-bit halves = 4 z-chars.
        let enc = encode(b"@", 3);
        let (text, _) = decode(&story_with(&enc), 0x40, 3, 0).unwrap();
        assert_eq!(text, "@");
    }

    #[test]
    fn abbreviations_expand() {
        // Abbreviation 0 points (word address 0x30 -> byte 0x60) at "the".
        let mut story = vec![0u8; 0x100];
        story[0x0F] = 0x80;
        story[0x50..0x52].copy_from_slice(&0x30u16.to_be_bytes());
        let the = encode(b"the", 3);
        story[0x60..0x64].copy_from_slice(&the);
        // z-chars: 1,0 (abbrev 0), 0 (space) | 6 ('a'), 5, 5 (end).
        #[allow(clippy::identity_op)] // spelled out to match the z-chars above
        let w1: u16 = (1 << 10) | (0 << 5) | 0;
        let w2: u16 = 0x8000 | (6 << 10) | (5 << 5) | 5;
        story[0x40..0x42].copy_from_slice(&w1.to_be_bytes());
        story[0x42..0x44].copy_from_slice(&w2.to_be_bytes());
        let mem = Memory::new(story).unwrap();
        let (text, _) = decode(&mem, 0x40, 3, 0x50).unwrap();
        assert_eq!(text, "the a");
    }

    #[test]
    fn zscii_extras() {
        assert_eq!(zscii_to_char(155), Some('ä'));
        assert_eq!(char_to_zscii('ä'), Some(155));
        assert_eq!(zscii_to_char(13), Some('\n'));
        assert_eq!(char_to_zscii('€'), None);
    }
}
