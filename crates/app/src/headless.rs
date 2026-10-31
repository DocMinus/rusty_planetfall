//! Plain stdin/stdout mode: handy for scripted testing and for playing in an
//! ordinary terminal.

use std::io::{self, BufRead, IsTerminal, Write};

use zmachine::{Event, Vm};

use crate::screen::{Screen, wrap};

const COLS: usize = 80;

pub fn run(mut vm: Vm) -> Result<(), String> {
    vm.set_screen_size(COLS as u16, 25);
    let mut screen = Screen::default();
    let mut printed = 0; // absolute index of the first paragraph not yet printed
    let echo = !io::stdin().is_terminal();
    let mut stdin = io::stdin().lock();
    let mut out = io::stdout().lock();

    let mut read_line = |prompt: &str, out: &mut io::StdoutLock| -> Option<String> {
        let _ = write!(out, "{prompt}");
        let _ = out.flush();
        let mut line = String::new();
        match stdin.read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => {
                let line = line.trim_end_matches(['\r', '\n']).to_string();
                if echo {
                    let _ = writeln!(out, "{line}");
                }
                Some(line)
            }
        }
    };

    loop {
        let event = vm.run().map_err(|e| e.to_string())?;
        screen.apply(vm.take_output());
        let current = screen.current_para();
        for para in screen.paragraphs_from(printed).iter().take(current - printed.min(current)) {
            for line in wrap(para, COLS) {
                let _ = writeln!(out, "{line}");
            }
        }
        printed = current;

        match event {
            Event::NeedLine { .. } => {
                let open = &screen.paragraphs_from(current)[0];
                let mut lines = wrap(open, COLS);
                let last = lines.pop().unwrap_or_default();
                for line in lines {
                    let _ = writeln!(out, "{line}");
                }
                let Some(line) = read_line(&last, &mut out) else { return Ok(()) };
                screen.print(&format!("{line}\n"));
                printed = screen.current_para();
                vm.provide_line(&line).map_err(|e| e.to_string())?;
            }
            Event::Save(data) => {
                let ok = read_line("Save to file: ", &mut out)
                    .filter(|p| !p.is_empty())
                    .is_some_and(|path| std::fs::write(path, &data).is_ok());
                vm.provide_save_result(ok).map_err(|e| e.to_string())?;
            }
            Event::Restore => {
                let data = read_line("Restore from file: ", &mut out).and_then(|p| std::fs::read(p).ok());
                if let Err(e) = vm.provide_restore(data.as_deref()) {
                    let _ = writeln!(out, "[{e}]");
                }
            }
            Event::Quit => {
                let open = &screen.paragraphs_from(current)[0];
                if !open.is_empty() {
                    let _ = writeln!(out, "{open}");
                }
                return Ok(());
            }
        }
    }
}
