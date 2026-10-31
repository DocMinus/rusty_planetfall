//! The virtual machine: call frames, variables and the opcode set for
//! versions 1-3 (Standard, sections 5, 6 and 14-15).

use crate::decode::{self, Branch, Kind, Operand};
use crate::dictionary::Dictionary;
use crate::header::{self as h, StoryInfo};
use crate::memory::Memory;
use crate::objects::Objects;
use crate::quetzal::{self, SaveState, SavedFrame};
use crate::rng::Rng;
use crate::text;
use crate::{Event, Output, Result, ScreenOp, StatusLine, ZError};

const STACK_LIMIT: usize = 32 * 1024;
const FRAME_LIMIT: usize = 1024;

/// Score-style stories whose "moves" global is really a clock (release, serial).
/// Planetfall keeps its chronometer reading there, so label it "Time".
const CLOCK_IN_MOVES: &[(u16, &str)] =
    &[(20, "830708"), (26, "831014"), (29, "840118"), (37, "851003"), (39, "880501")];

struct Frame {
    return_pc: usize,
    store: Option<u8>,
    args: u8,
    locals: Vec<u16>,
    stack_base: usize,
}

impl Frame {
    fn main() -> Self {
        Self { return_pc: 0, store: None, args: 0, locals: Vec::new(), stack_base: 0 }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Running,
    Reading { text: usize, parse: usize },
    Saving { branch_addr: usize },
    Restoring { branch_addr: usize },
    Halted,
}

pub struct Vm {
    mem: Memory,
    version: u8,
    pc: usize,
    stack: Vec<u16>,
    frames: Vec<Frame>,
    globals: usize,
    abbreviations: usize,
    objects: Objects,
    dictionary: Dictionary,
    rng: Rng,
    output: Output,
    state: State,
    window: u8,
    screen_output: bool,
    /// Open memory streams (stream 3): table address and collected ZSCII.
    memory_streams: Vec<(usize, Vec<u8>)>,
    screen_size: (u8, u8),
}

impl Vm {
    pub fn new(story: Vec<u8>) -> Result<Self> {
        let version = *story.first().ok_or(ZError::BadStory)?;
        if !(1..=3).contains(&version) {
            return Err(ZError::UnsupportedVersion(version));
        }
        let mem = Memory::new(story)?;
        let word = |a| mem.u16(a).map(|w| w as usize);
        let mut vm = Self {
            version,
            pc: word(h::INITIAL_PC)?,
            stack: Vec::new(),
            frames: vec![Frame::main()],
            globals: word(h::GLOBALS)?,
            abbreviations: word(h::ABBREVIATIONS)?,
            objects: Objects::new(word(h::OBJECTS)?),
            dictionary: Dictionary::new(&mem, word(h::DICTIONARY)?)?,
            rng: Rng::new(),
            output: Output::default(),
            state: State::Running,
            window: 0,
            screen_output: true,
            memory_streams: Vec::new(),
            screen_size: (80, 25),
            mem,
        };
        vm.write_header()?;
        Ok(vm)
    }

    // ---- Public API -------------------------------------------------------

    pub fn info(&self) -> StoryInfo {
        let m = self.mem.original();
        StoryInfo {
            version: self.version,
            release: u16::from_be_bytes([m[h::RELEASE], m[h::RELEASE + 1]]),
            serial: String::from_utf8_lossy(&m[h::SERIAL..h::SERIAL + 6]).into_owned(),
            checksum: u16::from_be_bytes([m[h::CHECKSUM], m[h::CHECKSUM + 1]]),
            file_length: self.file_length(),
        }
    }

    /// Tell the story how big the screen is, in characters.
    pub fn set_screen_size(&mut self, cols: u16, rows: u16) {
        self.screen_size = (cols.min(255) as u8, rows.min(255) as u8);
        let _ = self.write_header();
    }

    /// Make `random` repeatable (for tests and replays).
    pub fn seed_random(&mut self, seed: u64) {
        self.rng.seed(seed);
    }

    pub fn take_output(&mut self) -> Output {
        std::mem::take(&mut self.output)
    }

    /// True if the game has scripting (transcript) switched on.
    pub fn transcript_on(&self) -> bool {
        self.mem.u8(h::FLAGS2).is_ok_and(|f| f & h::F2_TRANSCRIPT != 0)
    }

    /// Switch scripting off, e.g. when the user cancels the transcript file.
    pub fn stop_transcript(&mut self) {
        if let Ok(f) = self.mem.u8(h::FLAGS2) {
            let _ = self.mem.set_u8(h::FLAGS2, f & !h::F2_TRANSCRIPT);
        }
    }

    /// Number of objects and the name of each, for `--dump`.
    pub fn object_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        // The first property table marks the end of the object entries.
        let first_entry = self.objects_addr() + 62;
        let end = self.mem.u16(first_entry + 7).map_or(first_entry, |a| a as usize);
        let count = end.saturating_sub(first_entry) / 9;
        for obj in 1..=count.min(255) as u16 {
            names.push(self.objects.short_name(&self.mem, obj, self.version, self.abbreviations).unwrap_or_default());
        }
        names
    }

    /// Execute until the story needs the outside world.
    pub fn run(&mut self) -> Result<Event> {
        match self.state {
            State::Running => {}
            State::Halted => return Ok(Event::Quit),
            State::Reading { text, .. } => return Ok(Event::NeedLine { max_len: self.max_input(text)? }),
            _ => return Err(ZError::NotRunning),
        }
        loop {
            if let Some(event) = self.step()? {
                return Ok(event);
            }
        }
    }

    /// Answer [`Event::NeedLine`].
    pub fn provide_line(&mut self, line: &str) -> Result<()> {
        let State::Reading { text, parse } = self.state else {
            return Err(ZError::NotRunning);
        };
        let max = self.max_input(text)?;
        let input: Vec<u8> = line
            .chars()
            .filter_map(text::char_to_zscii)
            .map(|c| c.to_ascii_lowercase())
            .take(max)
            .collect();
        for (i, &c) in input.iter().enumerate() {
            self.mem.set_u8(text + 1 + i, c)?;
        }
        self.mem.set_u8(text + 1 + input.len(), 0)?;
        if self.transcript_on() {
            self.output.transcript.push_str(line);
            self.output.transcript.push('\n');
        }
        if parse != 0 {
            self.dictionary.tokenise(&mut self.mem, &input, 1, parse, self.version)?;
        }
        self.state = State::Running;
        Ok(())
    }

    /// Answer [`Event::Save`]: did writing the file succeed?
    pub fn provide_save_result(&mut self, ok: bool) -> Result<()> {
        let State::Saving { branch_addr } = self.state else {
            return Err(ZError::NotRunning);
        };
        self.state = State::Running;
        self.branch_at(branch_addr, ok)
    }

    /// Answer [`Event::Restore`] with the save file's contents, or `None` if
    /// the user cancelled. A bad file makes the restore fail in-game and
    /// returns the error so the front end can say why.
    pub fn provide_restore(&mut self, data: Option<&[u8]>) -> Result<()> {
        let State::Restoring { branch_addr } = self.state else {
            return Err(ZError::NotRunning);
        };
        self.state = State::Running;
        let Some(data) = data else {
            return self.branch_at(branch_addr, false);
        };
        match self.load_state(data) {
            // The restored PC points at the branch bytes of the original `save`.
            Ok(()) => self.branch_at(self.pc, true),
            Err(e) => {
                self.branch_at(branch_addr, false)?;
                Err(e)
            }
        }
    }

    /// Quetzal snapshot of the current state, with `pc` as the resume point.
    fn snapshot(&self, pc: usize) -> Vec<u8> {
        let m = self.mem.original();
        let frames = self
            .frames
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let end = self.frames.get(i + 1).map_or(self.stack.len(), |n| n.stack_base);
                SavedFrame {
                    return_pc: f.return_pc,
                    store: f.store,
                    args: f.args,
                    locals: f.locals.clone(),
                    stack: self.stack[f.stack_base..end].to_vec(),
                }
            })
            .collect();
        let state = SaveState {
            release: u16::from_be_bytes([m[h::RELEASE], m[h::RELEASE + 1]]),
            serial: m[h::SERIAL..h::SERIAL + 6].try_into().unwrap(),
            checksum: u16::from_be_bytes([m[h::CHECKSUM], m[h::CHECKSUM + 1]]),
            pc,
            dynamic: self.mem.dynamic().to_vec(),
            frames,
        };
        quetzal::write(&state, self.mem.original_dynamic())
    }

    fn load_state(&mut self, data: &[u8]) -> Result<()> {
        let s = quetzal::read(data, self.mem.original_dynamic())?;
        let info = self.info();
        if s.release != info.release || s.serial != info.serial.as_bytes() || s.checksum != info.checksum {
            return Err(ZError::BadSave("this save file is for a different game or release".into()));
        }
        let flags2 = self.mem.u8(h::FLAGS2)?;
        self.mem.set_dynamic(&s.dynamic);
        self.stack.clear();
        self.frames.clear();
        for f in s.frames {
            self.frames.push(Frame {
                return_pc: f.return_pc,
                store: f.store,
                args: f.args,
                locals: f.locals,
                stack_base: self.stack.len(),
            });
            self.stack.extend(f.stack);
        }
        self.pc = s.pc;
        self.keep_flags2(flags2)?;
        self.write_header()
    }

    // ---- Header -----------------------------------------------------------

    fn objects_addr(&self) -> usize {
        self.mem.u16(h::OBJECTS).unwrap_or(0) as usize
    }

    fn file_length(&self) -> usize {
        let m = self.mem.original();
        let len = u16::from_be_bytes([m[h::FILE_LENGTH], m[h::FILE_LENGTH + 1]]) as usize * h::scale(self.version);
        if len == 0 { m.len() } else { len.min(m.len()) }
    }

    fn write_header(&mut self) -> Result<()> {
        let f1 = self.mem.u8(h::FLAGS1)?;
        let f1 = (f1 & !(h::F1_NO_STATUS_LINE | h::F1_VARIABLE_PITCH_DEFAULT)) | h::F1_SPLIT_AVAILABLE;
        self.mem.set_u8(h::FLAGS1, f1)?;
        self.mem.set_u8(h::INTERPRETER_NUMBER, 6)?; // IBM PC
        self.mem.set_u8(h::INTERPRETER_VERSION, b'A')?;
        self.mem.set_u8(h::SCREEN_HEIGHT, self.screen_size.1)?;
        self.mem.set_u8(h::SCREEN_WIDTH, self.screen_size.0)?;
        self.mem.set_u16(h::STANDARD_REVISION, 0x0101)
    }

    /// Restart and restore keep the transcript and fixed-pitch bits.
    fn keep_flags2(&mut self, old: u8) -> Result<()> {
        let keep = h::F2_TRANSCRIPT | h::F2_FIXED_PITCH;
        let f2 = self.mem.u8(h::FLAGS2)?;
        self.mem.set_u8(h::FLAGS2, (f2 & !keep) | (old & keep))
    }

    fn restart(&mut self) -> Result<()> {
        let flags2 = self.mem.u8(h::FLAGS2)?;
        let original = self.mem.original_dynamic().to_vec();
        self.mem.set_dynamic(&original);
        self.keep_flags2(flags2)?;
        self.write_header()?;
        self.stack.clear();
        self.frames = vec![Frame::main()];
        self.pc = self.mem.u16(h::INITIAL_PC)? as usize;
        self.window = 0;
        self.screen_output = true;
        self.memory_streams.clear();
        self.output.ops.push(ScreenOp::ClearAll);
        Ok(())
    }

    /// Does the story file match its header checksum?
    pub fn verify(&self) -> bool {
        let m = self.mem.original();
        let sum = m[0x40..self.file_length()].iter().fold(0u16, |s, &b| s.wrapping_add(b as u16));
        sum == u16::from_be_bytes([m[h::CHECKSUM], m[h::CHECKSUM + 1]])
    }

    // ---- Variables and stack ----------------------------------------------

    fn frame(&self) -> &Frame {
        self.frames.last().expect("there is always a main frame")
    }

    fn push(&mut self, v: u16) -> Result<()> {
        if self.stack.len() >= STACK_LIMIT {
            return Err(ZError::StackOverflow);
        }
        self.stack.push(v);
        Ok(())
    }

    fn pop(&mut self) -> Result<u16> {
        if self.stack.len() <= self.frame().stack_base {
            return Err(ZError::StackUnderflow);
        }
        Ok(self.stack.pop().unwrap())
    }

    fn peek_mut(&mut self) -> Result<&mut u16> {
        if self.stack.len() <= self.frame().stack_base {
            return Err(ZError::StackUnderflow);
        }
        Ok(self.stack.last_mut().unwrap())
    }

    fn local_index(&self, var: u8) -> Result<usize> {
        let i = var as usize - 1;
        if i < self.frame().locals.len() { Ok(i) } else { Err(ZError::BadVariable(var)) }
    }

    fn global_addr(&self, var: u8) -> usize {
        self.globals + 2 * (var as usize - 16)
    }

    fn read_var(&mut self, var: u8) -> Result<u16> {
        match var {
            0 => self.pop(),
            1..=15 => Ok(self.frame().locals[self.local_index(var)?]),
            _ => self.mem.u16(self.global_addr(var)),
        }
    }

    fn write_var(&mut self, var: u8, v: u16) -> Result<()> {
        match var {
            0 => self.push(v),
            1..=15 => {
                let i = self.local_index(var)?;
                self.frames.last_mut().unwrap().locals[i] = v;
                Ok(())
            }
            _ => self.mem.set_u16(self.global_addr(var), v),
        }
    }

    /// Opcodes that take a variable *number* (inc, load, store, pull ...) use
    /// the top of the stack in place instead of pushing or popping.
    fn read_var_ref(&mut self, var: u8) -> Result<u16> {
        if var == 0 { self.peek_mut().map(|v| *v) } else { self.read_var(var) }
    }

    fn write_var_ref(&mut self, var: u8, v: u16) -> Result<()> {
        if var == 0 {
            *self.peek_mut()? = v;
            Ok(())
        } else {
            self.write_var(var, v)
        }
    }

    fn value(&mut self, op: Operand) -> Result<u16> {
        match op {
            Operand::Const(v) => Ok(v),
            Operand::Var(n) => self.read_var(n),
        }
    }

    // ---- Control flow -----------------------------------------------------

    fn call(&mut self, packed: u16, args: &[u16], store: Option<u8>) -> Result<()> {
        if packed == 0 {
            return match store {
                Some(s) => self.write_var(s, 0),
                None => Ok(()),
            };
        }
        if self.frames.len() >= FRAME_LIMIT {
            return Err(ZError::StackOverflow);
        }
        let addr = packed as usize * h::scale(self.version);
        let n = (self.mem.u8(addr)? as usize).min(15);
        let mut locals = (0..n).map(|i| self.mem.u16(addr + 1 + 2 * i)).collect::<Result<Vec<_>>>()?;
        let args = &args[..args.len().min(n)];
        locals[..args.len()].copy_from_slice(args);
        self.frames.push(Frame {
            return_pc: self.pc,
            store,
            args: args.len() as u8,
            locals,
            stack_base: self.stack.len(),
        });
        self.pc = addr + 1 + 2 * n;
        Ok(())
    }

    fn ret(&mut self, v: u16) -> Result<()> {
        if self.frames.len() <= 1 {
            return Err(ZError::ReturnFromMain);
        }
        let frame = self.frames.pop().unwrap();
        self.stack.truncate(frame.stack_base);
        self.pc = frame.return_pc;
        match frame.store {
            Some(s) => self.write_var(s, v),
            None => Ok(()),
        }
    }

    fn branch(&mut self, b: Option<Branch>, cond: bool) -> Result<()> {
        let Some(b) = b else { return Ok(()) };
        if cond != b.on_true {
            return Ok(());
        }
        match b.offset {
            0 => self.ret(0),
            1 => self.ret(1),
            off => {
                self.pc = (self.pc as isize + off as isize - 2) as usize;
                Ok(())
            }
        }
    }

    /// Branch using the branch bytes at `addr` (resuming save/restore).
    fn branch_at(&mut self, addr: usize, cond: bool) -> Result<()> {
        let (b, next) = decode::parse_branch(&self.mem, addr)?;
        self.pc = next;
        self.branch(Some(b), cond)
    }

    // ---- Output -----------------------------------------------------------

    fn print(&mut self, s: &str) {
        if let Some((_, buf)) = self.memory_streams.last_mut() {
            buf.extend(s.chars().filter_map(|c| if c == '\n' { Some(13) } else { text::char_to_zscii(c) }));
            return;
        }
        if self.screen_output {
            self.output.print(self.window, s);
        }
        if self.window == 0 && self.transcript_on() {
            self.output.transcript.push_str(s);
        }
    }

    fn print_zstring(&mut self, addr: usize) -> Result<()> {
        let (s, _) = text::decode(&self.mem, addr, self.version, self.abbreviations)?;
        self.print(&s);
        Ok(())
    }

    fn object_name(&self, obj: u16) -> Result<String> {
        self.objects.short_name(&self.mem, obj, self.version, self.abbreviations)
    }

    fn show_status(&mut self) -> Result<()> {
        let location = self.object_name(self.mem.u16(self.global_addr(16))?)?;
        let (a, b) = (self.mem.u16(self.global_addr(17))?, self.mem.u16(self.global_addr(18))?);
        let right = if self.mem.u8(h::FLAGS1)? & h::F1_TIME_GAME != 0 {
            let (hour, ampm) = match a % 24 {
                0 => (12, "am"),
                h @ 1..=11 => (h, "am"),
                12 => (12, "pm"),
                h => (h - 12, "pm"),
            };
            format!("Time: {hour}:{b:02} {ampm}")
        } else {
            let info = self.info();
            let label = if CLOCK_IN_MOVES.contains(&(info.release, info.serial.as_str())) { "Time" } else { "Moves" };
            format!("Score: {}  {label}: {}", a as i16, b)
        };
        self.output.status = Some(StatusLine { location, right });
        Ok(())
    }

    fn output_stream(&mut self, n: i16, table: u16) -> Result<()> {
        match n {
            1 => self.screen_output = true,
            -1 => self.screen_output = false,
            2 | -2 => {
                let f2 = self.mem.u8(h::FLAGS2)?;
                let f2 = if n > 0 { f2 | h::F2_TRANSCRIPT } else { f2 & !h::F2_TRANSCRIPT };
                self.mem.set_u8(h::FLAGS2, f2)?;
            }
            3 if self.memory_streams.len() < 16 => self.memory_streams.push((table as usize, Vec::new())),
            -3 => {
                if let Some((addr, buf)) = self.memory_streams.pop() {
                    self.mem.set_u16(addr, buf.len() as u16)?;
                    for (i, &c) in buf.iter().enumerate() {
                        self.mem.set_u8(addr + 2 + i, c)?;
                    }
                }
            }
            _ => {} // stream 4 (command recording) is the front end's business
        }
        Ok(())
    }

    fn max_input(&self, text: usize) -> Result<usize> {
        // Versions 1-4: byte 0 holds the buffer size, one more than the letters allowed.
        Ok((self.mem.u8(text)? as usize).saturating_sub(1))
    }

    // ---- Execution --------------------------------------------------------

    fn step(&mut self) -> Result<Option<Event>> {
        let ins = decode::decode(&self.mem, self.pc)?;
        self.pc = ins.next;
        let mut v = [0u16; 8];
        let n = ins.operands().len();
        for (i, &op) in ins.operands().iter().enumerate() {
            v[i] = self.value(op)?;
        }
        let a = v[0];
        let b = v[1];
        let illegal = || ZError::IllegalOpcode(ins.opcode_byte(), ins.addr);
        let store = |vm: &mut Self, value: u16| vm.write_var(ins.store.unwrap_or(0), value);

        match (ins.kind, ins.num) {
            // 2OP
            (Kind::Op2, 1) => self.branch(ins.branch, v[1..n.max(2)].contains(&a))?,
            (Kind::Op2, 2) => self.branch(ins.branch, (a as i16) < (b as i16))?,
            (Kind::Op2, 3) => self.branch(ins.branch, (a as i16) > (b as i16))?,
            (Kind::Op2, 4) => {
                let x = (self.read_var_ref(a as u8)? as i16).wrapping_sub(1);
                self.write_var_ref(a as u8, x as u16)?;
                self.branch(ins.branch, x < b as i16)?;
            }
            (Kind::Op2, 5) => {
                let x = (self.read_var_ref(a as u8)? as i16).wrapping_add(1);
                self.write_var_ref(a as u8, x as u16)?;
                self.branch(ins.branch, x > b as i16)?;
            }
            (Kind::Op2, 6) => {
                let parent = self.objects.parent(&self.mem, a)?;
                self.branch(ins.branch, parent == b)?;
            }
            (Kind::Op2, 7) => self.branch(ins.branch, a & b == b)?,
            (Kind::Op2, 8) => store(self, a | b)?,
            (Kind::Op2, 9) => store(self, a & b)?,
            (Kind::Op2, 10) => {
                let set = self.objects.attr(&self.mem, a, b)?;
                self.branch(ins.branch, set)?;
            }
            (Kind::Op2, 11) => self.objects.set_attr(&mut self.mem, a, b, true)?,
            (Kind::Op2, 12) => self.objects.set_attr(&mut self.mem, a, b, false)?,
            (Kind::Op2, 13) => self.write_var_ref(a as u8, b)?,
            (Kind::Op2, 14) => self.objects.insert(&mut self.mem, a, b)?,
            (Kind::Op2, 15) => {
                let w = self.mem.u16(a.wrapping_add(b.wrapping_mul(2)) as usize)?;
                store(self, w)?;
            }
            (Kind::Op2, 16) => {
                let byte = self.mem.u8(a.wrapping_add(b) as usize)?;
                store(self, byte as u16)?;
            }
            (Kind::Op2, 17) => {
                let p = self.objects.get_prop(&self.mem, a, b)?;
                store(self, p)?;
            }
            (Kind::Op2, 18) => {
                let p = self.objects.get_prop_addr(&self.mem, a, b)?;
                store(self, p)?;
            }
            (Kind::Op2, 19) => {
                let p = self.objects.next_prop(&self.mem, a, b)?;
                store(self, p)?;
            }
            (Kind::Op2, 20) => store(self, a.wrapping_add(b))?,
            (Kind::Op2, 21) => store(self, a.wrapping_sub(b))?,
            (Kind::Op2, 22) => store(self, a.wrapping_mul(b))?,
            (Kind::Op2, 23 | 24) => {
                if b == 0 {
                    return Err(ZError::DivisionByZero);
                }
                let (x, y) = (a as i16, b as i16);
                let r = if ins.num == 23 { x.wrapping_div(y) } else { x.wrapping_rem(y) };
                store(self, r as u16)?;
            }

            // 1OP
            (Kind::Op1, 0) => self.branch(ins.branch, a == 0)?,
            (Kind::Op1, 1 | 2) => {
                let o = if ins.num == 1 {
                    self.objects.sibling(&self.mem, a)?
                } else {
                    self.objects.child(&self.mem, a)?
                };
                store(self, o)?;
                self.branch(ins.branch, o != 0)?;
            }
            (Kind::Op1, 3) => {
                let p = self.objects.parent(&self.mem, a)?;
                store(self, p)?;
            }
            (Kind::Op1, 4) => {
                let len = Objects::prop_len(&self.mem, a)?;
                store(self, len)?;
            }
            (Kind::Op1, 5 | 6) => {
                let x = self.read_var_ref(a as u8)?;
                let x = if ins.num == 5 { x.wrapping_add(1) } else { x.wrapping_sub(1) };
                self.write_var_ref(a as u8, x)?;
            }
            (Kind::Op1, 7) => self.print_zstring(a as usize)?,
            (Kind::Op1, 9) => self.objects.remove(&mut self.mem, a)?,
            (Kind::Op1, 10) => {
                let name = self.object_name(a)?;
                self.print(&name);
            }
            (Kind::Op1, 11) => self.ret(a)?,
            (Kind::Op1, 12) => self.pc = (self.pc as isize + a as i16 as isize - 2) as usize,
            (Kind::Op1, 13) => self.print_zstring(a as usize * h::scale(self.version))?,
            (Kind::Op1, 14) => {
                let x = self.read_var_ref(a as u8)?;
                store(self, x)?;
            }
            (Kind::Op1, 15) => store(self, !a)?,

            // 0OP
            (Kind::Op0, 0) => self.ret(1)?,
            (Kind::Op0, 1) => self.ret(0)?,
            (Kind::Op0, 2) => self.print_zstring(ins.text_addr)?,
            (Kind::Op0, 3) => {
                self.print_zstring(ins.text_addr)?;
                self.print("\n");
                self.ret(1)?;
            }
            (Kind::Op0, 4) => {}
            (Kind::Op0, 5) => {
                self.state = State::Saving { branch_addr: ins.branch_addr };
                return Ok(Some(Event::Save(self.snapshot(ins.branch_addr))));
            }
            (Kind::Op0, 6) => {
                self.state = State::Restoring { branch_addr: ins.branch_addr };
                return Ok(Some(Event::Restore));
            }
            (Kind::Op0, 7) => self.restart()?,
            (Kind::Op0, 8) => {
                let x = self.pop()?;
                self.ret(x)?;
            }
            (Kind::Op0, 9) => {
                self.pop()?;
            }
            (Kind::Op0, 10) => {
                self.state = State::Halted;
                return Ok(Some(Event::Quit));
            }
            (Kind::Op0, 11) => self.print("\n"),
            (Kind::Op0, 12) => self.show_status()?,
            (Kind::Op0, 13) => {
                let ok = self.verify();
                self.branch(ins.branch, ok)?;
            }

            // VAR
            (Kind::Var, 0) => self.call(a, &v[1..n.max(1)], ins.store)?,
            (Kind::Var, 1) => self.mem.set_u16(a.wrapping_add(b.wrapping_mul(2)) as usize, v[2])?,
            (Kind::Var, 2) => self.mem.set_u8(a.wrapping_add(b) as usize, v[2] as u8)?,
            (Kind::Var, 3) => self.objects.put_prop(&mut self.mem, a, b, v[2])?,
            (Kind::Var, 4) => {
                self.show_status()?;
                let (text, parse) = (a as usize, b as usize);
                self.state = State::Reading { text, parse };
                return Ok(Some(Event::NeedLine { max_len: self.max_input(text)? }));
            }
            (Kind::Var, 5) => {
                if let Some(c) = text::zscii_to_char(a) {
                    self.print(c.encode_utf8(&mut [0; 4]));
                }
            }
            (Kind::Var, 6) => self.print(&(a as i16).to_string()),
            (Kind::Var, 7) => {
                let r = match a as i16 {
                    x if x > 0 => self.rng.next(x as u16),
                    0 => {
                        self.rng.reseed_random();
                        0
                    }
                    x => {
                        self.rng.seed_predictable(x.unsigned_abs());
                        0
                    }
                };
                store(self, r)?;
            }
            (Kind::Var, 8) => self.push(a)?,
            (Kind::Var, 9) => {
                let x = self.pop()?;
                self.write_var_ref(a as u8, x)?;
            }
            (Kind::Var, 10) => {
                self.output.ops.push(ScreenOp::Split(a));
                self.output.ops.push(ScreenOp::ClearUpper);
            }
            (Kind::Var, 11) => {
                self.window = (a as u8).min(1);
                if self.window == 1 {
                    self.output.ops.push(ScreenOp::UpperHome);
                }
            }
            (Kind::Var, 19) => self.output_stream(a as i16, b)?,
            (Kind::Var, 20 | 21) => {} // input_stream, sound_effect: nothing to do

            _ => return Err(illegal()),
        }
        Ok(None)
    }
}
