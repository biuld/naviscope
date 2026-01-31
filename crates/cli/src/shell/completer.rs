use super::context::ShellContext;
use naviscope_core::query::GraphQuery;
use reedline::{Completer, Suggestion};

pub struct NaviscopeCompleter<'a> {
    pub commands: Vec<String>,
    pub context: ShellContext,
    pub _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> NaviscopeCompleter<'a> {
    pub fn new(commands: Vec<String>, context: ShellContext) -> Self {
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
            return self
                .commands
                .iter()
                .filter(|cmd| cmd.starts_with(trimmed))
                .map(|cmd| Suggestion {
                    value: cmd.clone(),
                    description: None,
                    style: None,
                    extra: None,
                    span: reedline::Span {
                        start: pos - trimmed.len(),
                        end: pos,
                    },
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
                let last_word = if line.ends_with(' ') {
                    ""
                } else {
                    parts.last().unwrap_or(&"")
                };
                let span_start = pos - last_word.len();

                // Get current context
                let parent_fqn = self.context.current_fqn();

                let graph = self.context.graph();
                let mut suggestions = Vec::new();

                // Case A: Global FQN completion
                // We only do this if:
                // 1. The word already contains navigation markers ('.' or '::')
                // 2. OR we are at root and the word is NOT empty (to avoid listing all FQNs on empty tab)
                if last_word.contains('.')
                    || last_word.contains("::")
                    || (parent_fqn.is_none() && !last_word.is_empty())
                {
                    // Find potential FQNs starting with last_word from the global map
                    // Limit results to avoid performance issues
                    let matches: Vec<String> = graph
                        .fqn_map()
                        .keys()
                        .filter(|fqn| fqn.starts_with(last_word))
                        .take(20) // Reduced from 50 to 20 for global search
                        .map(|s| s.to_string())
                        .collect();

                    for fqn in matches {
                        suggestions.push(Suggestion {
                            value: fqn,
                            description: None,
                            style: None,
                            extra: None,
                            span: reedline::Span {
                                start: span_start,
                                end: pos,
                            },
                            append_whitespace: true,
                            match_indices: None,
                        });
                    }
                }

                // Case B: Relative completion from current context (or root)
                let query = GraphQuery::Ls {
                    fqn: parent_fqn.clone(),
                    kind: vec![],
                    modifiers: vec![],
                };

                if let Ok(result) = self.context.execute_query(&query) {
                    for node in result.nodes {
                        let name = node.name();
                        if name.starts_with(last_word) {
                            // De-duplicate if already added by Case A
                            if suggestions.iter().any(|s| s.value == name) {
                                continue;
                            }

                            suggestions.push(Suggestion {
                                value: name.to_string(),
                                description: Some(node.kind().to_string()),
                                style: None,
                                extra: None,
                                span: reedline::Span {
                                    start: span_start,
                                    end: pos,
                                },
                                append_whitespace: true,
                                match_indices: None,
                            });
                        }
                    }
                }

                // Sort suggestions: Relative first (shorter names that aren't FQNs usually)
                // Then by length
                suggestions.sort_by(|a, b| {
                    let a_is_fqn = a.value.contains('.') || a.value.contains("::");
                    let b_is_fqn = b.value.contains('.') || b.value.contains("::");
                    if a_is_fqn != b_is_fqn {
                        a_is_fqn.cmp(&b_is_fqn) // Non-FQN first
                    } else {
                        a.value.len().cmp(&b.value.len())
                    }
                });

                // Final limit to total suggestions to keep UI clean
                suggestions.truncate(50);

                return suggestions;
            }
        }

        vec![]
    }
}
