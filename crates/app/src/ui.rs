//! The window: a character grid painted with egui. Row 0 is the status line,
//! then the upper window (if the story split the screen), then the scrolling
//! lower window.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use eframe::egui::{self, Align2, Color32, FontId, Key, Pos2, Rect, Vec2, ViewportCommand};
use zmachine::{Event, Vm};

use crate::screen::{Screen, moves_label, status_text, wrap};
use crate::settings::{DEFAULT_FONT_SIZE, Settings};

const PAD: f32 = 8.0;
const START_GRID: (usize, usize) = (80, 25);
const MOD: &str = if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" };

pub fn run(story: Option<PathBuf>, seed: Option<u64>) -> Result<(), String> {
    // A rough size; the first frame corrects it to exactly 80x25 cells.
    let estimate = Vec2::new(
        START_GRID.0 as f32 * DEFAULT_FONT_SIZE * 0.6 + 2.0 * PAD,
        START_GRID.1 as f32 * DEFAULT_FONT_SIZE * 1.25 + 2.0 * PAD,
    );
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Rusty Z")
            .with_inner_size(estimate)
            .with_min_inner_size([320.0, 200.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native("Rusty Z", options, Box::new(move |cc| Ok(Box::new(App::new(cc, story, seed)))))
        .map_err(|e| e.to_string())
}

#[derive(PartialEq, Eq)]
enum Mode {
    NoStory,
    Input,
    Ended,
    Failed,
}

struct App {
    settings: Settings,
    vm: Option<Vm>,
    story_name: String,
    moves_label: &'static str,
    seed: Option<u64>,
    screen: Screen,
    mode: Mode,
    input: String,
    max_input: usize,
    history: Vec<String>,
    history_pos: Option<usize>,
    /// Paragraph where the output of the current turn starts (for [MORE]).
    turn_start: usize,
    more_seen: usize,
    more_pending: bool,
    /// Lines scrolled back from the bottom.
    scroll: usize,
    scroll_accum: f32,
    grid: (usize, usize),
    /// Resize the window to this many cells once font metrics are known.
    resize_to: Option<(usize, usize)>,
    transcript: Option<File>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, story: Option<PathBuf>, seed: Option<u64>) -> Self {
        let settings: Settings = cc.storage.and_then(|s| eframe::get_value(s, "settings")).unwrap_or_default();
        let mut app = Self {
            vm: None,
            story_name: String::new(),
            moves_label: "Moves",
            seed,
            screen: Screen::default(),
            mode: Mode::NoStory,
            input: String::new(),
            max_input: 0,
            history: Vec::new(),
            history_pos: None,
            turn_start: 0,
            more_seen: 0,
            more_pending: false,
            scroll: 0,
            scroll_accum: 0.0,
            grid: START_GRID,
            resize_to: Some(START_GRID),
            transcript: None,
            settings,
        };
        let path = story.or_else(|| app.settings.last_story.clone().filter(|p| p.exists()));
        if let Some(path) = path {
            app.open_story(&cc.egui_ctx, &path);
        }
        app
    }

    fn open_story(&mut self, ctx: &egui::Context, path: &Path) {
        match crate::load_story(path) {
            Ok(mut vm) => {
                if let Some(seed) = self.seed {
                    vm.seed_random(seed);
                }
                vm.set_screen_size(self.grid.0 as u16, self.grid.1 as u16);
                self.moves_label = moves_label(&vm.info());
                self.vm = Some(vm);
                self.screen = Screen::default();
                self.story_name = path.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
                self.settings.last_story = Some(path.to_path_buf());
                self.transcript = None;
                self.history.clear();
                self.input.clear();
                self.turn_start = 0;
                self.more_seen = 0;
                self.scroll = 0;
                ctx.send_viewport_cmd(ViewportCommand::Title(format!("{} — Rusty Z", self.story_name)));
                self.pump();
            }
            Err(e) => {
                self.screen.print(&format!("[{e}]\n"));
            }
        }
    }

    fn open_dialog(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new().set_title("Open story file").pick_file() {
            self.open_story(ctx, &path);
        }
    }

    /// Run the VM until it wants a line of input (or stops).
    fn pump(&mut self) {
        let Some(vm) = self.vm.as_mut() else { return };
        loop {
            let result = vm.run();
            let output = vm.take_output();
            write_transcript(&mut self.transcript, vm, &output.transcript, &self.story_name);
            self.screen.apply(output);
            match result {
                Ok(Event::NeedLine { max_len }) => {
                    self.max_input = max_len;
                    self.mode = Mode::Input;
                    return;
                }
                Ok(Event::Save(data)) => {
                    let ok = rfd::FileDialog::new()
                        .set_title("Save game")
                        .set_file_name(format!("{}.qzl", self.story_name))
                        .save_file()
                        .is_some_and(|path| std::fs::write(path, &data).is_ok());
                    if let Err(e) = vm.provide_save_result(ok) {
                        self.screen.print(&format!("\n[Interpreter error: {e}]"));
                        self.mode = Mode::Failed;
                        return;
                    }
                }
                Ok(Event::Restore) => {
                    let data = rfd::FileDialog::new()
                        .set_title("Restore game")
                        .pick_file()
                        .and_then(|path| std::fs::read(path).ok());
                    if let Err(e) = vm.provide_restore(data.as_deref()) {
                        self.screen.print(&format!("[{e}]\n"));
                    }
                }
                Ok(Event::Quit) => {
                    self.screen.print("\n[The story has ended. Press any key to close the window.]");
                    self.mode = Mode::Ended;
                    return;
                }
                Err(e) => {
                    self.screen.print(&format!("\n[Interpreter error: {e}]"));
                    self.mode = Mode::Failed;
                    return;
                }
            }
        }
    }

    fn submit(&mut self) {
        let line = std::mem::take(&mut self.input);
        if !line.trim().is_empty() && self.history.last() != Some(&line) {
            self.history.push(line.clone());
        }
        self.history_pos = None;
        self.screen.print(&format!("{line}\n"));
        self.turn_start = self.screen.current_para();
        self.more_seen = 0;
        self.scroll = 0;
        if let Some(vm) = self.vm.as_mut() {
            match vm.provide_line(&line) {
                Ok(()) => self.pump(),
                Err(e) => {
                    self.screen.print(&format!("\n[Interpreter error: {e}]"));
                    self.mode = Mode::Failed;
                }
            }
        }
    }

    fn history_step(&mut self, up: bool) {
        if self.history.is_empty() {
            return;
        }
        let pos = match (self.history_pos, up) {
            (None, true) => Some(self.history.len() - 1),
            (None, false) => None,
            (Some(p), true) => Some(p.saturating_sub(1)),
            (Some(p), false) if p + 1 < self.history.len() => Some(p + 1),
            (Some(_), false) => None,
        };
        self.history_pos = pos;
        self.input = pos.map_or_else(String::new, |p| self.history[p].clone());
    }

    fn set_font_size(&mut self, size: f32) {
        self.settings.font_size = size.clamp(8.0, 40.0);
        self.resize_to = Some(self.grid);
    }

    fn lower_rows(&self) -> usize {
        self.grid.1.saturating_sub(1 + self.screen.upper_rows().len()).max(1)
    }

    fn handle_input(&mut self, ctx: &egui::Context) {
        let (events, wheel) = ctx.input(|i| (i.events.clone(), i.smooth_scroll_delta.y));
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if let Some(path) = dropped.into_iter().next() {
            self.open_story(ctx, &path);
        }

        let page = self.lower_rows().saturating_sub(1).max(1);
        let row_h = ctx.fonts_mut(|f| f.row_height(&FontId::monospace(self.settings.font_size)));
        self.scroll_accum += wheel;
        let lines = (self.scroll_accum / row_h).trunc();
        if lines != 0.0 {
            self.scroll_accum -= lines * row_h;
            self.scroll = (self.scroll as isize + lines as isize).max(0) as usize;
        }

        let mut swallow_text = false;
        for event in events {
            match event {
                egui::Event::Key { key, pressed: true, modifiers, .. } if modifiers.command => match key {
                    Key::O => self.open_dialog(ctx),
                    Key::Plus | Key::Equals => self.set_font_size(self.settings.font_size + 1.0),
                    Key::Minus => self.set_font_size(self.settings.font_size - 1.0),
                    Key::Num0 => self.set_font_size(DEFAULT_FONT_SIZE),
                    Key::T => self.settings.theme = self.settings.theme.next(),
                    Key::Q if !cfg!(target_os = "macos") => ctx.send_viewport_cmd(ViewportCommand::Close),
                    _ => {}
                },
                // Ignore auto-repeat so a held Enter doesn't close before the
                // final message can be read.
                egui::Event::Key { pressed: true, repeat: false, .. } if self.mode == Mode::Ended => {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
                egui::Event::Key { key, pressed: true, .. } if self.mode == Mode::Input => {
                    if matches!(key, Key::PageUp | Key::PageDown) {
                        let delta = if key == Key::PageUp { page as isize } else { -(page as isize) };
                        self.scroll = (self.scroll as isize + delta).max(0) as usize;
                        continue;
                    }
                    self.scroll = 0;
                    if self.more_pending {
                        self.more_seen += page;
                        swallow_text = true;
                        continue;
                    }
                    match key {
                        Key::Enter => self.submit(),
                        Key::Backspace => {
                            self.input.pop();
                        }
                        Key::Escape => self.input.clear(),
                        Key::ArrowUp => self.history_step(true),
                        Key::ArrowDown => self.history_step(false),
                        _ => {}
                    }
                }
                egui::Event::Text(t) | egui::Event::Paste(t) if self.mode == Mode::Input && !swallow_text => {
                    if self.more_pending {
                        continue;
                    }
                    for c in t.lines().next().unwrap_or("").chars().filter(|c| !c.is_control()) {
                        if self.input.chars().count() < self.max_input {
                            self.input.push(c);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn paint(&mut self, ui: &egui::Ui, rect: Rect, font: &FontId, cw: f32, rh: f32) {
        let (fg, bg) = self.settings.theme.colors();
        let painter = ui.painter();
        painter.rect_filled(rect, 0.0, bg);
        let (cols, _) = self.grid;
        let origin = rect.min + Vec2::splat(PAD);
        let row_pos = |row: usize| Pos2::new(origin.x, origin.y + row as f32 * rh);
        let text = |row: usize, s: &str, color: Color32| {
            painter.text(row_pos(row), Align2::LEFT_TOP, s, font.clone(), color);
        };

        if self.mode == Mode::NoStory {
            let lines = [
                "RUSTY Z".to_string(),
                "A Z-machine interpreter".to_string(),
                String::new(),
                format!("Drop a story file here, or press {MOD}-O to open one."),
                String::new(),
                format!("{MOD} +/-  font size     {MOD}-T  colours     {MOD}-O  open"),
            ];
            let mut row = 2;
            for line in self.screen.paragraphs_from(0).iter().filter(|p| !p.is_empty()).chain(&lines) {
                for l in wrap(line, cols) {
                    text(row, &l, fg);
                    row += 1;
                }
            }
            return;
        }

        // Status line in reverse video.
        let status_rect = Rect::from_min_size(
            Pos2::new(rect.min.x, origin.y),
            Vec2::new(rect.width(), rh),
        );
        painter.rect_filled(status_rect, 0.0, fg);
        if let Some(status) = &self.screen.status {
            text(0, &status_text(status, self.moves_label, cols), bg);
        }

        // Upper window.
        let upper = self.screen.upper_rows();
        for (i, row) in upper.iter().enumerate() {
            let s: String = row.iter().take(cols).collect();
            text(1 + i, &s, fg);
        }

        // Lower window: earlier lines, then this turn's output (paged by [MORE]).
        let lower_rows = self.lower_rows();
        let input_mode = self.mode == Mode::Input;
        let mut turn: Vec<String> = self.screen.paragraphs_from(self.turn_start).to_vec();
        if input_mode && let Some(last) = turn.last_mut() {
            last.push_str(&self.input);
        }
        let turn_lines: Vec<String> = turn.iter().flat_map(|p| wrap(p, cols)).collect();
        let page = lower_rows.saturating_sub(1).max(1);
        self.more_pending =
            input_mode && turn_lines.len() > lower_rows && self.more_seen + lower_rows < turn_lines.len();
        let shown = if self.more_pending { self.more_seen + page } else { turn_lines.len() };
        let want = lower_rows + self.scroll;
        let mut lines = self.screen.tail_lines(self.turn_start, cols, want.saturating_sub(shown));
        lines.extend_from_slice(&turn_lines[..shown]);
        if self.more_pending {
            lines.push("[MORE]".to_string());
        }
        self.scroll = self.scroll.min(lines.len().saturating_sub(lower_rows));
        let end = lines.len() - self.scroll;
        let start = end.saturating_sub(lower_rows);
        let top = 1 + upper.len();
        for (i, line) in lines[start..end].iter().enumerate() {
            text(top + i, line, fg);
        }

        // Block cursor at the end of the input line.
        if input_mode && !self.more_pending && self.scroll == 0 && end > start {
            let row = top + (end - start) - 1;
            let col = lines[end - 1].chars().count().min(cols.saturating_sub(1));
            let pos = row_pos(row) + Vec2::new(col as f32 * cw, 0.0);
            painter.rect_filled(Rect::from_min_size(pos, Vec2::new(cw, rh)), 0.0, fg.gamma_multiply(0.7));
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let font = FontId::monospace(self.settings.font_size);
        let (cw, rh) = ctx.fonts_mut(|f| (f.glyph_width(&font, 'M'), f.row_height(&font)));

        if let Some((cols, rows)) = self.resize_to.take() {
            let size = Vec2::new(cols as f32 * cw + 2.0 * PAD, rows as f32 * rh + 2.0 * PAD);
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(size.ceil()));
        }

        let rect = ui.max_rect();
        let cols = (((rect.width() - 2.0 * PAD) / cw).floor() as usize).max(20);
        let rows = (((rect.height() - 2.0 * PAD) / rh).floor() as usize).max(4);
        if (cols, rows) != self.grid {
            self.grid = (cols, rows);
            if let Some(vm) = self.vm.as_mut() {
                vm.set_screen_size(cols as u16, rows as u16);
            }
        }

        self.handle_input(&ctx);
        self.paint(ui, rect, &font, cw, rh);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "settings", &self.settings);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        self.settings.theme.colors().1.to_normalized_gamma_f32()
    }
}

/// Append transcript text, asking for a file the first time. Cancelling the
/// dialog switches scripting off again, as a failed printer would have.
fn write_transcript(file: &mut Option<File>, vm: &mut Vm, text: &str, story: &str) {
    if !text.is_empty() {
        if file.is_none() {
            *file = rfd::FileDialog::new()
                .set_title("Save transcript")
                .set_file_name(format!("{story} transcript.txt"))
                .save_file()
                .and_then(|path| File::options().create(true).append(true).open(path).ok());
        }
        match file {
            Some(f) => {
                let _ = f.write_all(text.as_bytes());
            }
            None => vm.stop_transcript(),
        }
    }
    if !vm.transcript_on() {
        *file = None;
    }
}
