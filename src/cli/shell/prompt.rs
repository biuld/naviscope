use nu_ansi_term::Color;
use reedline::{Prompt, PromptEditMode, PromptHistorySearch};
use std::borrow::Cow;

pub struct DefaultPrompt {
    current_node: Option<String>,
}

impl DefaultPrompt {
    pub fn new(current_node: Option<String>) -> Self {
        Self { current_node }
    }
}

impl Prompt for DefaultPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        let prefix = Color::LightBlue.bold().paint("naviscope");
        match &self.current_node {
            Some(node) => {
                let display_node = if node.len() > 30 {
                    shorten_fqn(node)
                } else {
                    node.clone()
                };
                let path = Color::Yellow.paint(display_node);
                Cow::Owned(format!("{} {} > ", prefix, path))
            }
            None => {
                let path = Color::Yellow.paint("/");
                Cow::Owned(format!("{} {} > ", prefix, path))
            }
        }
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_indicator(&self, _edit_mode: PromptEditMode) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
        Cow::Borrowed(".. ")
    }

    fn render_prompt_history_search_indicator(
        &self,
        _history_search: PromptHistorySearch,
    ) -> Cow<'_, str> {
        Cow::Borrowed("(search) ")
    }
}

fn shorten_fqn(fqn: &str) -> String {
    let separator = if fqn.contains("::") { "::" } else { "." };
    let parts: Vec<&str> = fqn.split(separator).collect();
    if parts.len() <= 2 {
        return fqn.to_string();
    }

    let mut result = String::new();
    // Abbreviate all but the last 2 parts
    for (i, part) in parts.iter().enumerate() {
        if i < parts.len() - 2 {
            if let Some(c) = part.chars().next() {
                result.push(c);
                result.push_str(separator);
            }
        } else {
            result.push_str(part);
            if i < parts.len() - 1 {
                result.push_str(separator);
            }
        }
    }
    result
}
