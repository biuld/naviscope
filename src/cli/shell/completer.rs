use naviscope::query::{GraphQuery, QueryEngine};
use reedline::{Completer, Suggestion};
use super::context::ShellContext;

pub struct NaviscopeCompleter<'a> {
    pub commands: Vec<String>,
    pub context: ShellContext,
    pub _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> NaviscopeCompleter<'a> {
    pub fn new(
        commands: Vec<String>,
        context: ShellContext,
    ) -> Self {
        Self {
            commands,
            context,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<'a> Completer for NaviscopeCompleter<'a> {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        let trimmed = line.trim_start();
        
        // 1. Command completion (at start of line)
        if !trimmed.contains(' ') {
            return self.commands.iter()
                .filter(|cmd| cmd.starts_with(trimmed))
                .map(|cmd| Suggestion {
                    value: cmd.clone(),
                    description: None,
                    style: None,
                    extra: None,
                    span: reedline::Span { start: pos - trimmed.len(), end: pos },
                    append_whitespace: true,
                    match_indices: None,
                })
                .collect();
        }

        // 2. Argument completion (for cd, ls, cat, deps)
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() >= 1 {
            let cmd = parts[0];
            if matches!(cmd, "cd" | "ls" | "cat" | "deps") {
                // Determine the partial FQN being typed
                let last_word = if line.ends_with(' ') { "" } else { parts.last().unwrap_or(&"") };
                let span_start = pos - last_word.len();

                // Get current context
                let parent_fqn = self.context.current_fqn();
                
                // Query graph for children of current context (or partial match)
                // If last_word contains dots, we might need to resolve relative to root or parent
                // For simplicity: list children of current_node, filtering by last_word
                
                let search_fqn = if let Some(parent) = &parent_fqn {
                    if last_word.is_empty() {
                         Some(parent.clone())
                    } else {
                         // Naive: just look for children of parent that start with last_word
                         Some(parent.clone())
                    }
                } else {
                    None // Root
                };

                // Use engine to list children
                let query = GraphQuery::Ls { 
                    fqn: search_fqn, 
                    kind: vec![], 
                    modifiers: vec![]
                };
                
                if let Ok(naviscope) = self.context.naviscope.read() {
                    let engine = QueryEngine::new(naviscope.graph());
                    
                    if let Ok(result) = engine.execute(&query) {
                         return result.nodes.iter()
                            .map(|node| node.name())
                            .filter(|name| name.starts_with(last_word))
                            .map(|name| Suggestion {
                                value: name.to_string(),
                                description: None,
                                style: None,
                                extra: None,
                                span: reedline::Span { start: span_start, end: pos },
                                append_whitespace: true,
                                match_indices: None,
                            })
                            .collect();
                    }
                }
            }
        }

        vec![]
    }
}
