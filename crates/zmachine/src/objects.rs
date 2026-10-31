//! The object table, versions 1-3 layout (Standard, section 12): 31 default
//! property words, then 9-byte entries of 32 attribute bits, parent, sibling,
//! child and a property-table address.

use crate::memory::Memory;
use crate::text;
use crate::{Result, ZError};

const DEFAULTS: usize = 31;
const ENTRY: usize = 9;
const MAX_OBJECT: u16 = 255;

#[derive(Clone, Copy)]
enum Link {
    Parent = 4,
    Sibling = 5,
    Child = 6,
}

pub struct Objects {
    table: usize,
}

impl Objects {
    pub fn new(table: usize) -> Self {
        Self { table }
    }

    fn entry(&self, obj: u16) -> Result<usize> {
        if obj == 0 || obj > MAX_OBJECT {
            return Err(ZError::BadObject(obj));
        }
        Ok(self.table + DEFAULTS * 2 + (obj as usize - 1) * ENTRY)
    }

    pub fn attr(&self, mem: &Memory, obj: u16, attr: u16) -> Result<bool> {
        if obj == 0 || attr > 31 {
            return Ok(false);
        }
        let byte = mem.u8(self.entry(obj)? + attr as usize / 8)?;
        Ok(byte & (0x80 >> (attr % 8)) != 0)
    }

    pub fn set_attr(&self, mem: &mut Memory, obj: u16, attr: u16, on: bool) -> Result<()> {
        if obj == 0 || attr > 31 {
            return Ok(());
        }
        let addr = self.entry(obj)? + attr as usize / 8;
        let bit = 0x80 >> (attr % 8);
        let byte = mem.u8(addr)?;
        mem.set_u8(addr, if on { byte | bit } else { byte & !bit })
    }

    fn link(&self, mem: &Memory, obj: u16, link: Link) -> Result<u16> {
        if obj == 0 {
            return Ok(0);
        }
        Ok(mem.u8(self.entry(obj)? + link as usize)? as u16)
    }

    fn set_link(&self, mem: &mut Memory, obj: u16, link: Link, value: u16) -> Result<()> {
        mem.set_u8(self.entry(obj)? + link as usize, value as u8)
    }

    pub fn parent(&self, mem: &Memory, obj: u16) -> Result<u16> {
        self.link(mem, obj, Link::Parent)
    }

    pub fn sibling(&self, mem: &Memory, obj: u16) -> Result<u16> {
        self.link(mem, obj, Link::Sibling)
    }

    pub fn child(&self, mem: &Memory, obj: u16) -> Result<u16> {
        self.link(mem, obj, Link::Child)
    }

    /// Detach `obj` from its parent, keeping its own children.
    pub fn remove(&self, mem: &mut Memory, obj: u16) -> Result<()> {
        if obj == 0 {
            return Ok(());
        }
        let parent = self.parent(mem, obj)?;
        if parent == 0 {
            return Ok(());
        }
        let next = self.sibling(mem, obj)?;
        let first = self.child(mem, parent)?;
        if first == obj {
            self.set_link(mem, parent, Link::Child, next)?;
        } else {
            let mut cur = first;
            while cur != 0 {
                let sib = self.sibling(mem, cur)?;
                if sib == obj {
                    self.set_link(mem, cur, Link::Sibling, next)?;
                    break;
                }
                cur = sib;
            }
        }
        self.set_link(mem, obj, Link::Parent, 0)?;
        self.set_link(mem, obj, Link::Sibling, 0)
    }

    /// Make `obj` the first child of `dest`.
    pub fn insert(&self, mem: &mut Memory, obj: u16, dest: u16) -> Result<()> {
        if obj == 0 || dest == 0 {
            return Ok(());
        }
        self.remove(mem, obj)?;
        let first = self.child(mem, dest)?;
        self.set_link(mem, obj, Link::Sibling, first)?;
        self.set_link(mem, obj, Link::Parent, dest)?;
        self.set_link(mem, dest, Link::Child, obj)
    }

    fn prop_table(&self, mem: &Memory, obj: u16) -> Result<usize> {
        Ok(mem.u16(self.entry(obj)? + 7)? as usize)
    }

    pub fn short_name(&self, mem: &Memory, obj: u16, version: u8, abbreviations: usize) -> Result<String> {
        if obj == 0 {
            return Ok(String::new());
        }
        let table = self.prop_table(mem, obj)?;
        if mem.u8(table)? == 0 {
            return Ok(String::new());
        }
        Ok(text::decode(mem, table + 1, version, abbreviations)?.0)
    }

    /// Address of the first property's size byte.
    fn first_prop(&self, mem: &Memory, obj: u16) -> Result<usize> {
        let table = self.prop_table(mem, obj)?;
        Ok(table + 1 + 2 * mem.u8(table)? as usize)
    }

    /// Finds a property: (address of its data, length), or None.
    fn find(&self, mem: &Memory, obj: u16, prop: u16) -> Result<Option<(usize, usize)>> {
        let mut addr = self.first_prop(mem, obj)?;
        loop {
            let size = mem.u8(addr)?;
            if size == 0 {
                return Ok(None);
            }
            let (num, len) = ((size & 0x1F) as u16, (size >> 5) as usize + 1);
            if num == prop {
                return Ok(Some((addr + 1, len)));
            }
            if num < prop {
                return Ok(None); // properties are stored in descending order
            }
            addr += 1 + len;
        }
    }

    pub fn get_prop(&self, mem: &Memory, obj: u16, prop: u16) -> Result<u16> {
        match self.find(mem, obj, prop)? {
            Some((addr, 1)) => Ok(mem.u8(addr)? as u16),
            Some((addr, _)) => mem.u16(addr),
            None if (1..=DEFAULTS as u16).contains(&prop) => mem.u16(self.table + 2 * (prop as usize - 1)),
            None => Err(ZError::NoProperty(obj, prop)),
        }
    }

    pub fn get_prop_addr(&self, mem: &Memory, obj: u16, prop: u16) -> Result<u16> {
        if obj == 0 {
            return Ok(0);
        }
        Ok(self.find(mem, obj, prop)?.map_or(0, |(addr, _)| addr as u16))
    }

    /// Length of the property whose data starts at `addr` (0 for address 0).
    pub fn prop_len(mem: &Memory, addr: u16) -> Result<u16> {
        if addr == 0 {
            return Ok(0);
        }
        Ok((mem.u8(addr as usize - 1)? >> 5) as u16 + 1)
    }

    /// The property after `prop` (or the first if `prop` is 0); 0 at the end.
    pub fn next_prop(&self, mem: &Memory, obj: u16, prop: u16) -> Result<u16> {
        if obj == 0 {
            return Ok(0);
        }
        let addr = if prop == 0 {
            self.first_prop(mem, obj)?
        } else {
            let (data, len) = self.find(mem, obj, prop)?.ok_or(ZError::NoProperty(obj, prop))?;
            data + len
        };
        Ok((mem.u8(addr)? & 0x1F) as u16)
    }

    pub fn put_prop(&self, mem: &mut Memory, obj: u16, prop: u16, value: u16) -> Result<()> {
        match self.find(mem, obj, prop)? {
            Some((addr, 1)) => mem.set_u8(addr, value as u8),
            Some((addr, _)) => mem.set_u16(addr, value),
            None => Err(ZError::NoProperty(obj, prop)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Objects 1-3 with tiny property tables; static memory well beyond.
    fn setup() -> (Memory, Objects) {
        let mut story = vec![0u8; 0x400];
        story[0x0E] = 0x03; // static base 0x300
        let table = 0x40;
        let mut m = story;
        for obj in 1..=3usize {
            let e = table + 62 + (obj - 1) * 9;
            let props = 0x200 + obj * 0x10;
            m[e + 7..e + 9].copy_from_slice(&(props as u16).to_be_bytes());
            m[props] = 0; // no name
            m[props + 1] = (1 << 5) | 10; // prop 10, length 2
            m[props + 2..props + 4].copy_from_slice(&0xBEEFu16.to_be_bytes());
            m[props + 4] = 5; // prop 5, length 1
            m[props + 5] = 7;
            m[props + 6] = 0;
        }
        m[table..table + 2].copy_from_slice(&42u16.to_be_bytes()); // default prop 1
        (Memory::new(m).unwrap(), Objects::new(table))
    }

    #[test]
    fn tree_insert_and_remove() {
        let (mut m, o) = setup();
        o.insert(&mut m, 2, 1).unwrap();
        o.insert(&mut m, 3, 1).unwrap();
        assert_eq!(o.child(&m, 1).unwrap(), 3);
        assert_eq!(o.sibling(&m, 3).unwrap(), 2);
        assert_eq!(o.parent(&m, 2).unwrap(), 1);
        o.remove(&mut m, 2).unwrap();
        assert_eq!(o.sibling(&m, 3).unwrap(), 0);
        assert_eq!(o.parent(&m, 2).unwrap(), 0);
        o.remove(&mut m, 3).unwrap();
        assert_eq!(o.child(&m, 1).unwrap(), 0);
    }

    #[test]
    fn attributes() {
        let (mut m, o) = setup();
        assert!(!o.attr(&m, 1, 31).unwrap());
        o.set_attr(&mut m, 1, 31, true).unwrap();
        o.set_attr(&mut m, 1, 0, true).unwrap();
        assert!(o.attr(&m, 1, 31).unwrap() && o.attr(&m, 1, 0).unwrap());
        o.set_attr(&mut m, 1, 0, false).unwrap();
        assert!(!o.attr(&m, 1, 0).unwrap());
    }

    #[test]
    fn properties() {
        let (mut m, o) = setup();
        assert_eq!(o.get_prop(&m, 1, 10).unwrap(), 0xBEEF);
        assert_eq!(o.get_prop(&m, 1, 5).unwrap(), 7);
        assert_eq!(o.get_prop(&m, 1, 1).unwrap(), 42); // default
        let addr = o.get_prop_addr(&m, 1, 10).unwrap();
        assert_eq!(Objects::prop_len(&m, addr).unwrap(), 2);
        assert_eq!(o.next_prop(&m, 1, 0).unwrap(), 10);
        assert_eq!(o.next_prop(&m, 1, 10).unwrap(), 5);
        assert_eq!(o.next_prop(&m, 1, 5).unwrap(), 0);
        o.put_prop(&mut m, 1, 5, 0x1FF).unwrap();
        assert_eq!(o.get_prop(&m, 1, 5).unwrap(), 0xFF);
    }
}
