//! Instruction decoding (Standard, section 4).

use crate::memory::Memory;
use crate::{Result, ZError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Op0,
    Op1,
    Op2,
    Var,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operand {
    Const(u16),
    Var(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Branch {
    pub on_true: bool,
    /// 0 and 1 mean "return false/true"; anything else is a jump offset.
    pub offset: i16,
}

#[derive(Debug, Clone)]
pub struct Instruction {
    pub addr: usize,
    pub kind: Kind,
    pub num: u8,
    operands: [Operand; 8],
    operand_count: usize,
    pub store: Option<u8>,
    pub branch: Option<Branch>,
    /// Where the branch bytes start (save files store this PC in versions 1-3).
    pub branch_addr: usize,
    /// Start of the inline string of `print` / `print_ret`.
    pub text_addr: usize,
    pub next: usize,
}

impl Instruction {
    pub fn operands(&self) -> &[Operand] {
        &self.operands[..self.operand_count]
    }

    /// The first byte, for error messages.
    pub fn opcode_byte(&self) -> u8 {
        match self.kind {
            Kind::Op0 => 0xB0 | self.num,
            Kind::Op1 => 0x80 | self.num,
            Kind::Op2 => self.num,
            Kind::Var => 0xE0 | self.num,
        }
    }
}

fn stores(kind: Kind, num: u8) -> bool {
    match kind {
        Kind::Op2 => matches!(num, 8 | 9 | 15..=24),
        Kind::Op1 => matches!(num, 1..=4 | 14 | 15),
        Kind::Op0 => false,
        Kind::Var => matches!(num, 0 | 7),
    }
}

fn branches(kind: Kind, num: u8) -> bool {
    match kind {
        Kind::Op2 => matches!(num, 1..=7 | 10),
        Kind::Op1 => matches!(num, 0..=2),
        Kind::Op0 => matches!(num, 5 | 6 | 13),
        Kind::Var => false,
    }
}

pub fn parse_branch(mem: &Memory, addr: usize) -> Result<(Branch, usize)> {
    let b1 = mem.u8(addr)?;
    let on_true = b1 & 0x80 != 0;
    if b1 & 0x40 != 0 {
        return Ok((Branch { on_true, offset: (b1 & 0x3F) as i16 }, addr + 1));
    }
    let raw = (((b1 & 0x3F) as u16) << 8) | mem.u8(addr + 1)? as u16;
    // Sign-extend the 14-bit offset.
    let offset = ((raw << 2) as i16) >> 2;
    Ok((Branch { on_true, offset }, addr + 2))
}

/// Length in bytes of the z-string at `addr`.
fn zstring_len(mem: &Memory, addr: usize) -> Result<usize> {
    let mut a = addr;
    while mem.u16(a)? & 0x8000 == 0 {
        a += 2;
    }
    Ok(a + 2 - addr)
}

pub fn decode(mem: &Memory, addr: usize) -> Result<Instruction> {
    let op = mem.u8(addr)?;
    let mut pc = addr + 1;
    let mut types = [3u8; 8]; // 3 = omitted
    let (kind, num) = match op {
        0x00..=0x7F => {
            types[0] = if op & 0x40 != 0 { 2 } else { 1 };
            types[1] = if op & 0x20 != 0 { 2 } else { 1 };
            (Kind::Op2, op & 0x1F)
        }
        0x80..=0xBF => {
            types[0] = (op >> 4) & 3;
            let kind = if types[0] == 3 { Kind::Op0 } else { Kind::Op1 };
            (kind, op & 0x0F)
        }
        0xC0..=0xFF => {
            let t = mem.u8(pc)?;
            pc += 1;
            for (i, ty) in types.iter_mut().take(4).enumerate() {
                *ty = (t >> (6 - 2 * i)) & 3;
            }
            let kind = if op & 0x20 == 0 { Kind::Op2 } else { Kind::Var };
            (kind, op & 0x1F)
        }
    };

    let mut operands = [Operand::Const(0); 8];
    let mut operand_count = 0;
    for &ty in types.iter().take_while(|&&t| t != 3) {
        operands[operand_count] = match ty {
            0 => {
                pc += 2;
                Operand::Const(mem.u16(pc - 2)?)
            }
            1 => {
                pc += 1;
                Operand::Const(mem.u8(pc - 1)? as u16)
            }
            _ => {
                pc += 1;
                Operand::Var(mem.u8(pc - 1)?)
            }
        };
        operand_count += 1;
    }

    let store = if stores(kind, num) {
        pc += 1;
        Some(mem.u8(pc - 1)?)
    } else {
        None
    };
    let branch_addr = pc;
    let branch = if branches(kind, num) {
        let (b, next) = parse_branch(mem, pc)?;
        pc = next;
        Some(b)
    } else {
        None
    };
    let text_addr = pc;
    if kind == Kind::Op0 && matches!(num, 2 | 3) {
        pc += zstring_len(mem, pc)?;
    }
    if kind == Kind::Op2 && num == 0 {
        return Err(ZError::IllegalOpcode(op, addr));
    }

    Ok(Instruction { addr, kind, num, operands, operand_count, store, branch, branch_addr, text_addr, next: pc })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem(code: &[u8]) -> Memory {
        let mut story = vec![0u8; 0x100];
        story[0x0F] = 0x40;
        story[0x80..0x80 + code.len()].copy_from_slice(code);
        Memory::new(story).unwrap()
    }

    #[test]
    fn long_form_add_with_store() {
        // add L01 #05 -> sp   (long form: first operand variable)
        let i = decode(&mem(&[0x54, 0x01, 0x05, 0x00]), 0x80).unwrap();
        assert_eq!((i.kind, i.num), (Kind::Op2, 20));
        assert_eq!(i.operands(), &[Operand::Var(1), Operand::Const(5)]);
        assert_eq!(i.store, Some(0));
        assert_eq!(i.next, 0x84);
    }

    #[test]
    fn short_form_jz_with_short_branch() {
        // jz sp ?~rfalse  (branch on false, offset 0 -> return false)
        let i = decode(&mem(&[0xA0, 0x00, 0x40]), 0x80).unwrap();
        assert_eq!((i.kind, i.num), (Kind::Op1, 0));
        assert_eq!(i.branch, Some(Branch { on_true: false, offset: 0 }));
        assert_eq!(i.next, 0x83);
    }

    #[test]
    fn long_branch_is_sign_extended() {
        // je #1 #2 ?(-2): 14-bit offset 0x3FFE
        let i = decode(&mem(&[0x01, 0x01, 0x02, 0xBF, 0xFE]), 0x80).unwrap();
        assert_eq!(i.branch, Some(Branch { on_true: true, offset: -2 }));
        assert_eq!(i.next, 0x85);
    }

    #[test]
    fn variable_form_call() {
        // call 0x1234 #07 -> L02  (types: large, small, omitted...)
        let i = decode(&mem(&[0xE0, 0x1F, 0x12, 0x34, 0x07, 0x02]), 0x80).unwrap();
        assert_eq!((i.kind, i.num), (Kind::Var, 0));
        assert_eq!(i.operands(), &[Operand::Const(0x1234), Operand::Const(7)]);
        assert_eq!(i.store, Some(2));
    }

    #[test]
    fn print_skips_inline_text() {
        let i = decode(&mem(&[0xB2, 0x11, 0x11, 0x94, 0xA5]), 0x80).unwrap();
        assert_eq!((i.kind, i.num), (Kind::Op0, 2));
        assert_eq!(i.text_addr, 0x81);
        assert_eq!(i.next, 0x85);
    }
}
