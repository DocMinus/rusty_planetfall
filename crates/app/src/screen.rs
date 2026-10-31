//! The text screen: scrollback for the lower window, a small character grid
//! for the upper window, and the status line. Text is stored as paragraphs
//! and word-wrapped to the current width when drawn, so resizing reflows.

use zmachine::header::StoryInfo;
use zmachine::{Output, Progress, ScreenOp, StatusLine};

const MAX_PARAGRAPHS: usize = 5000;
/// Trim this many at once so the front of the Vec isn't shifted on every line.
const TRIM_PARAGRAPHS: usize = 1000;

pub struct Screen {
    /// Lower-window paragraphs; the last one is the line still being printed.
    paras: Vec<String>,
    /// Index of the first paragraph not dropped from the front (for callers
    /// that remember paragraph positions).
    dropped: usize,
    pub status: Option<StatusLine>,
    upper: Vec<Vec<char>>,
    upper_cursor: (usize, usize),
}

impl Default for Screen {
    fn default() -> Self {
        Self { paras: vec![String::new()], dropped: 0, status: None, upper: Vec::new(), upper_cursor: (0, 0) }
    }
}

impl Screen {
    pub fn clear(&mut self) {
        self.dropped += self.paras.len();
        self.paras = vec![String::new()];
        self.upper.iter_mut().for_each(|row| row.clear());
        self.upper_cursor = (0, 0);
    }

    /// Absolute index of the current (open) paragraph.
    pub fn current_para(&self) -> usize {
        self.dropped + self.paras.len() - 1
    }

    /// Paragraphs from absolute index `from` (clamped to what is kept).
    pub fn paragraphs_from(&self, from: usize) -> &[String] {
        &self.paras[from.saturating_sub(self.dropped).min(self.paras.len())..]
    }

    pub fn upper_rows(&self) -> &[Vec<char>] {
        &self.upper
    }

    pub fn print(&mut self, s: &str) {
        let mut parts = s.split('\n');
        if let Some(first) = parts.next() {
            self.paras.last_mut().unwrap().push_str(first);
        }
        for part in parts {
            self.paras.push(part.to_string());
        }
        if self.paras.len() > MAX_PARAGRAPHS {
            let excess = self.paras.len() - MAX_PARAGRAPHS + TRIM_PARAGRAPHS;
            self.paras.drain(..excess);
            self.dropped += excess;
        }
    }

    fn print_upper(&mut self, s: &str) {
        for c in s.chars() {
            let (row, col) = self.upper_cursor;
            if c == '\n' {
                self.upper_cursor = (row + 1, 0);
                continue;
            }
            if let Some(line) = self.upper.get_mut(row) {
                if line.len() <= col {
                    line.resize(col + 1, ' ');
                }
                line[col] = c;
            }
            self.upper_cursor = (row, col + 1);
        }
    }

    pub fn apply(&mut self, output: Output) {
        if let Some(status) = output.status {
            self.status = Some(status);
        }
        for op in output.ops {
            match op {
                ScreenOp::Print { window: 0, text } => self.print(&text),
                ScreenOp::Print { text, .. } => self.print_upper(&text),
                ScreenOp::Split(n) => self.upper.resize(n as usize, Vec::new()),
                ScreenOp::ClearUpper => {
                    self.upper.iter_mut().for_each(|row| row.clear());
                    self.upper_cursor = (0, 0);
                }
                ScreenOp::UpperHome => self.upper_cursor = (0, 0),
                ScreenOp::ClearAll => self.clear(),
            }
        }
    }

    /// The last `n` wrapped lines of paragraphs before absolute index `end`.
    pub fn tail_lines(&self, end: usize, cols: usize, n: usize) -> Vec<String> {
        let end = end.saturating_sub(self.dropped).min(self.paras.len());
        let mut lines = Vec::new();
        for para in self.paras[..end].iter().rev() {
            let mut wrapped = wrap(para, cols);
            wrapped.reverse();
            lines.extend(wrapped);
            if lines.len() >= n {
                break;
            }
        }
        lines.truncate(n);
        lines.reverse();
        lines
    }
}

/// Word-wrap one paragraph to `cols` columns. Long words are split.
pub fn wrap(para: &str, cols: usize) -> Vec<String> {
    let cols = cols.max(1);
    let chars: Vec<char> = para.chars().collect();
    let mut lines = Vec::new();
    let mut start = 0;
    while chars.len() - start > cols {
        let window = &chars[start..=start + cols];
        match window.iter().rposition(|&c| c == ' ') {
            Some(p) if p > 0 => {
                lines.push(chars[start..start + p].iter().collect());
                start += p + 1;
            }
            _ => {
                lines.push(chars[start..start + cols].iter().collect());
                start += cols;
            }
        }
    }
    lines.push(chars[start..].iter().collect());
    lines
}

/// Score-style stories whose "moves" global is really a clock (release, serial).
/// Planetfall keeps its chronometer reading there, so it is labelled "Time".
const CLOCK_IN_MOVES: &[(u16, &str)] =
    &[(20, "830708"), (26, "831014"), (29, "840118"), (37, "851003"), (39, "880501")];

/// The label for the "moves" number on this story's status line.
pub fn moves_label(info: &StoryInfo) -> &'static str {
    if CLOCK_IN_MOVES.contains(&(info.release, info.serial.as_str())) { "Time" } else { "Moves" }
}

/// The status line padded to exactly `cols` characters.
pub fn status_text(status: &StatusLine, moves_label: &str, cols: usize) -> String {
    let left: String = format!(" {}", status.location);
    let progress = match status.progress {
        Progress::Score { score, moves } => format!("Score: {score}  {moves_label}: {moves}"),
        Progress::Time { hours, minutes } => {
            let (hour, ampm) = match hours % 24 {
                0 => (12, "am"),
                h @ 1..=11 => (h, "am"),
                12 => (12, "pm"),
                h => (h - 12, "pm"),
            };
            format!("Time: {hour}:{minutes:02} {ampm}")
        }
    };
    let right = format!("{progress}   ");
    let gap = cols.saturating_sub(left.chars().count() + right.chars().count()).max(1);
    let line = format!("{left}{}{right}", " ".repeat(gap));
    let mut line: String = line.chars().take(cols).collect();
    while line.chars().count() < cols {
        line.push(' ');
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_at_spaces() {
        assert_eq!(wrap("the quick brown fox", 10), ["the quick", "brown fox"]);
        assert_eq!(wrap("short", 10), ["short"]);
        assert_eq!(wrap("", 10), [""]);
    }

    #[test]
    fn splits_long_words() {
        assert_eq!(wrap("abcdefghij", 4), ["abcd", "efgh", "ij"]);
    }

    #[test]
    fn exact_fit_breaks_on_following_space() {
        assert_eq!(wrap("abcd efgh", 4), ["abcd", "efgh"]);
    }

    #[test]
    fn printing_splits_paragraphs_and_reflows() {
        let mut s = Screen::default();
        s.print("one two three\nfour");
        s.print(" five\n>");
        assert_eq!(s.paragraphs_from(0), ["one two three", "four five", ">"]);
        assert_eq!(s.tail_lines(2, 8, 10), ["one two", "three", "four", "five"]);
        assert_eq!(s.tail_lines(2, 8, 2), ["four", "five"]);
    }

    #[test]
    fn status_line_is_padded() {
        let st = StatusLine { location: "Deck Nine".into(), progress: Progress::Score { score: 0, moves: 1 } };
        let line = status_text(&st, "Moves", 40);
        assert_eq!(line.chars().count(), 40);
        assert!(line.starts_with(" Deck Nine"));
        assert!(line.trim_end().ends_with("Score: 0  Moves: 1"));
        let st = StatusLine { location: "Bridge".into(), progress: Progress::Time { hours: 13, minutes: 5 } };
        assert!(status_text(&st, "Moves", 40).trim_end().ends_with("Time: 1:05 pm"));
    }

    #[test]
    fn planetfall_moves_are_time() {
        let info = |release, serial: &str| StoryInfo {
            version: 3,
            release,
            serial: serial.into(),
            checksum: 0,
            file_length: 0,
        };
        assert_eq!(moves_label(&info(39, "880501")), "Time");
        assert_eq!(moves_label(&info(88, "840726")), "Moves");
    }
}
