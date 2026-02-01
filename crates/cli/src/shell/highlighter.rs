use nu_ansi_term::{Color, Style};
use reedline::{Highlighter, StyledText};

pub struct NaviscopeHighlighter {
    commands: Vec<String>,
}

impl NaviscopeHighlighter {
    pub fn new(commands: Vec<String>) -> Self {
        Self { commands }
    }
}

impl Highlighter for NaviscopeHighlighter {
    fn highlight(&self, line: &str, _cursor: usize) -> StyledText {
        let mut styled_text = StyledText::new();
        // If the line ends with whitespace, we need to handle that to keep the highlighting accurate
        // But for simple word-based highlighting, we can just iterate over splits and gaps.

        let mut current_pos = 0;
        let words = line.split_inclusive(char::is_whitespace);

        for word in words {
            let trimmed = word.trim();
            if trimmed.is_empty() {
                styled_text.push((Style::new(), word.to_string()));
                current_pos += word.len();
                continue;
            }

            let style = if current_pos == 0 || self.is_at_start_of_command(line, current_pos) {
                if self.commands.contains(&trimmed.to_string()) {
                    Style::new().fg(Color::LightGreen).bold()
                } else {
                    Style::new()
                }
            } else if trimmed.starts_with('-') {
                Style::new().fg(Color::Cyan)
            } else if trimmed.contains('.') || trimmed.contains("::") || trimmed.contains('/') {
                Style::new().fg(Color::Yellow)
            } else {
                Style::new()
            };

            // Calculate trailing whitespace
            let word_to_push = word.to_string();
            styled_text.push((style, word_to_push));
            current_pos += word.len();
        }

        styled_text
    }
}

impl NaviscopeHighlighter {
    fn is_at_start_of_command(&self, line: &str, pos: usize) -> bool {
        let prefix = &line[..pos];
        prefix.trim().is_empty()
    }
}
