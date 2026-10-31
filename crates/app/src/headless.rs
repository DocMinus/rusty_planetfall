//! Plain stdin/stdout mode: handy for scripted testing and for playing in an
//! ordinary terminal.

use std::fs::File;
use std::io::{self, BufRead, IsTerminal, StdoutLock, Write};

use zmachine::{Event, ScreenOp, Vm};

use crate::screen::wrap;

const COLS: usize = 80;

struct Terminal {
    out: StdoutLock<'static>,
    /// The lower-window line still being printed (e.g. the prompt).
    pending: String,
    echo: bool,
}

impl Terminal {
    fn print(&mut self, text: &str) {
        let mut parts = text.split('\n');
        self.pending.push_str(parts.next().unwrap_or(""));
        for part in parts {
            for line in wrap(&self.pending, COLS) {
                let _ = writeln!(self.out, "{line}");
            }
            self.pending = part.to_string();
        }
    }

    /// Show the pending line as a prompt and read a line of input.
    fn read_line(&mut self, prompt: &str) -> Option<String> {
        self.pending.push_str(prompt);
        let mut lines = wrap(&std::mem::take(&mut self.pending), COLS);
        let last = lines.pop().unwrap_or_default();
        for line in lines {
            let _ = writeln!(self.out, "{line}");
        }
        let _ = write!(self.out, "{last}");
        let _ = self.out.flush();
        let mut line = String::new();
        match io::stdin().lock().read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => {
                let line = line.trim_end_matches(['\r', '\n']).to_string();
                if self.echo {
                    let _ = writeln!(self.out, "{line}");
                }
                Some(line)
            }
        }
    }

    /// An interpreter question (file names), kept apart from the game's
    /// pending output so that is still shown afterwards.
    fn ask(&mut self, prompt: &str) -> Option<String> {
        let pending = std::mem::take(&mut self.pending);
        let answer = self.read_line(prompt);
        self.pending = pending;
        answer
    }
}

pub fn run(mut vm: Vm) -> Result<(), String> {
    vm.set_screen_size(COLS as u16, 25);
    let mut term = Terminal { out: io::stdout().lock(), pending: String::new(), echo: !io::stdin().is_terminal() };
    let mut transcript: Option<File> = None;

    loop {
        let event = vm.run().map_err(|e| e.to_string())?;
        let output = vm.take_output();
        for op in &output.ops {
            if let ScreenOp::Print { window: 0, text } = op {
                term.print(text);
            }
        }
        if !output.transcript.is_empty() {
            if transcript.is_none() {
                transcript = term
                    .ask("Transcript file: ")
                    .filter(|p| !p.is_empty())
                    .and_then(|p| File::options().create(true).append(true).open(p).ok());
            }
            match transcript.as_mut() {
                Some(f) => {
                    let _ = f.write_all(output.transcript.as_bytes());
                }
                None => vm.stop_transcript(),
            }
        }
        if !vm.transcript_on() {
            transcript = None;
        }

        match event {
            Event::NeedLine { .. } => {
                let Some(line) = term.read_line("") else { return Ok(()) };
                vm.provide_line(&line).map_err(|e| e.to_string())?;
            }
            Event::Save(data) => {
                let ok = term
                    .ask("Save to file: ")
                    .filter(|p| !p.is_empty())
                    .is_some_and(|path| std::fs::write(path, &data).is_ok());
                vm.provide_save_result(ok).map_err(|e| e.to_string())?;
            }
            Event::Restore => {
                let data = term.ask("Restore from file: ").and_then(|p| std::fs::read(p).ok());
                if let Err(e) = vm.provide_restore(data.as_deref()) {
                    term.print(&format!("[{e}]\n"));
                }
            }
            Event::Quit => {
                if !term.pending.is_empty() {
                    term.print("\n");
                }
                return Ok(());
            }
        }
    }
}
