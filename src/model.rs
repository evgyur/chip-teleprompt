//! UI-independent behavior ported from Chip Teleprompt v21.

pub const DEFAULT_TEXT: &str = "Paste text with Paste button or Ctrl+V.\r\n\r\nChip Teleprompt. Smooth pixel scroll. Drag text with mouse for manual scroll. Speed slider has fine control on the left.";

/// Keep link labels for reading aloud, without clipboard-exported URL targets.
pub fn paste_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('(') {
        result.push_str(&rest[..start]);
        rest = &rest[start..];
        let target = rest[1..].trim_start().trim_start_matches('<');
        let is_url = ["https://", "http://", "mailto:"].iter().any(|prefix| {
            target
                .get(..prefix.len())
                .is_some_and(|s| s.eq_ignore_ascii_case(prefix))
        });
        let mut depth = 0;
        let end = if is_url {
            rest.char_indices().find_map(|(index, ch)| {
                match ch {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => (),
                }
                (depth == 0).then_some(index)
            })
        } else {
            None
        };
        if let Some(end) = end {
            // Markdown [label](url), including nested brackets in the label.
            if result.ends_with(']') {
                let mut brackets = 0;
                let opening = result.char_indices().rev().find_map(|(index, ch)| {
                    match ch {
                        ']' => brackets += 1,
                        '[' => brackets -= 1,
                        _ => (),
                    }
                    (brackets == 0).then_some(index)
                });
                if let Some(opening) = opening {
                    result.pop();
                    result.remove(opening);
                    if opening > 0 && result.as_bytes()[opening - 1] == b'!' {
                        result.remove(opening - 1);
                    }
                }
            } else {
                // Plain-text exports commonly use "label (https://...)".
                result.truncate(result.trim_end_matches([' ', '\t']).len());
            }
            rest = &rest[end + 1..];
        } else {
            result.push('(');
            rest = &rest[1..];
        }
    }
    result.push_str(rest);
    result
}

/// Fine control at low speeds, with the same midpoint rounding as .NET.
pub fn speed_px(value: u32) -> f64 {
    let fraction = value.min(100) as f64 / 100.0;
    (3.0 + 177.0 * fraction * fraction).round_ties_even()
}

pub fn remove_blank_lines(text: &str) -> String {
    text.split('\n')
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\r\n")
}

#[derive(Debug)]
pub struct Playback {
    pub y: f64,
    pub running: bool,
}

impl Playback {
    pub fn new(top: f64) -> Self {
        Self {
            y: top,
            running: false,
        }
    }

    /// Pasting and the Top button reposition without changing playback state.
    pub fn reset(&mut self, top: f64) {
        self.y = top;
    }

    pub fn toggle(&mut self) {
        self.running = !self.running;
    }

    /// Autoscroll ends only once the entire text has left the reading stage.
    /// The caller measures elapsed time with a monotonic clock.
    pub fn tick(&mut self, seconds: f64, speed: f64, text_height: f64) -> bool {
        if !self.running || seconds <= 0.0 || !seconds.is_finite() {
            return false;
        }
        self.y -= speed * seconds;
        if self.y + text_height < 0.0 {
            self.running = false;
        }
        true
    }

    pub fn drag_to(
        &mut self,
        y: f64,
        viewport_height: f64,
        text_height: f64,
        top: f64,
        bottom: f64,
    ) {
        self.running = false;
        self.y = y;
        self.clamp(viewport_height, text_height, top, bottom);
    }

    /// Manual scrolling keeps the last line visible; short scripts stay at top.
    pub fn clamp(&mut self, viewport_height: f64, text_height: f64, top: f64, bottom: f64) {
        let lower = (viewport_height - bottom - text_height).min(top);
        self.y = self.y.clamp(lower, top);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_keeps_labels_without_link_targets() {
        assert_eq!(
            paste_text("[Google](https://google.com) и [пример](https://example.com/a_(b))!"),
            "Google и пример!"
        );
        assert_eq!(paste_text("Google (https://blog.google/innovation-and-ai/models-and-research/gemini-models/gemini-3-8-text-to-speech/)."), "Google.");
        assert_eq!(paste_text("[Документ](<https://example.com> \"Title\")\r\n![Фото](https://example.com/photo.png)"), "Документ\r\nФото");
        assert_eq!(
            paste_text("[имя [уточнение]](https://example.com)"),
            "имя [уточнение]"
        );
        assert_eq!(
            paste_text("Почта [автора](mailto:author@example.com)"),
            "Почта автора"
        );
    }

    #[test]
    fn paste_preserves_regular_text_and_incomplete_links() {
        for text in [
            "",
            "Текст (пояснение).\r\n\r\nСледующая строка",
            "https://example.com",
            "[название](https://example.com",
            "[1] и (обычный текст)",
        ] {
            assert_eq!(paste_text(text), text);
        }
    }

    #[test]
    fn speed_endpoints_default_and_monotonicity() {
        assert_eq!(speed_px(0), 3.0);
        assert_eq!(speed_px(28), 17.0);
        assert_eq!(speed_px(100), 180.0);
        assert_eq!(speed_px(101), 180.0);
        for value in 1..=100 {
            assert!(speed_px(value) >= speed_px(value - 1));
        }
    }

    #[test]
    fn playback_uses_elapsed_time_independent_of_tick_count() {
        let mut frequent = Playback::new(8.0);
        let mut delayed = Playback::new(8.0);
        frequent.toggle();
        delayed.toggle();
        for _ in 0..100 {
            assert!(frequent.tick(0.01, 17.0, 1000.0));
        }
        assert!(delayed.tick(1.0, 17.0, 1000.0));
        assert!((frequent.y - delayed.y).abs() < 1e-10);
        assert!((delayed.y - -9.0).abs() < 1e-10);
    }

    #[test]
    fn autoscroll_passes_manual_bottom_and_stops_only_after_text_exits() {
        let mut playback = Playback::new(8.0);
        playback.toggle();
        playback.tick(28.0, 10.0, 300.0);
        assert_eq!(playback.y, -272.0);
        assert!(playback.running);
        playback.tick(2.8, 10.0, 300.0);
        assert_eq!(playback.y, -300.0);
        assert!(playback.running);
        playback.tick(0.1, 10.0, 300.0);
        assert!(!playback.running);
        let end = playback.y;
        assert!(!playback.tick(1.0, 10.0, 300.0));
        assert_eq!(playback.y, end);
    }

    #[test]
    fn manual_drag_pauses_and_clamps_to_visible_text() {
        let mut playback = Playback::new(8.0);
        playback.toggle();
        playback.drag_to(-1000.0, 200.0, 300.0, 8.0, 12.0);
        assert!(!playback.running);
        assert_eq!(playback.y, -112.0);
        playback.drag_to(1000.0, 200.0, 300.0, 8.0, 12.0);
        assert_eq!(playback.y, 8.0);
        playback.drag_to(-1000.0, 200.0, 100.0, 8.0, 12.0);
        assert_eq!(playback.y, 8.0);
    }

    #[test]
    fn reset_and_clamp_preserve_running_state() {
        let mut playback = Playback::new(8.0);
        playback.toggle();
        playback.tick(1.0, 17.0, 300.0);
        playback.reset(8.0);
        assert_eq!(playback.y, 8.0);
        assert!(playback.running);
        playback.clamp(200.0, 300.0, 6.0, 10.0);
        assert_eq!(playback.y, 6.0);
        assert!(playback.running);
        playback.toggle();
        playback.reset(8.0);
        assert!(!playback.running);
    }

    #[test]
    fn blank_line_removal_preserves_indentation_and_russian_text() {
        assert_eq!(
            remove_blank_lines(
                "\r\n  Привет, мир!  \r\n\t \r\n\tВторая строка\t\n\u{2003}\nПоследняя\n"
            ),
            "  Привет, мир!\r\n\tВторая строка\r\nПоследняя"
        );
        assert_eq!(remove_blank_lines("\r\n \t\n"), "");
        assert_eq!(remove_blank_lines(""), "");
    }

    #[test]
    fn paused_or_invalid_time_does_not_move() {
        let mut playback = Playback::new(8.0);
        assert!(!playback.tick(1.0, 17.0, 300.0));
        playback.toggle();
        for seconds in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(!playback.tick(seconds, 17.0, 300.0));
        }
        assert_eq!(playback.y, 8.0);
    }
}
