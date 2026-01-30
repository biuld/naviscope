use super::command::ShellCommand;
use super::context::{ResolveResult, ShellContext};
use naviscope::query::GraphQuery;
use naviscope::query::QueryEngine;

pub trait CommandHandler {
    fn handle(
        &self,
        cmd: &ShellCommand,
        context: &mut ShellContext,
    ) -> Result<String, Box<dyn std::error::Error>>;
}

pub struct CdHandler;
impl CommandHandler for CdHandler {
    fn handle(
        &self,
        cmd: &ShellCommand,
        context: &mut ShellContext,
    ) -> Result<String, Box<dyn std::error::Error>> {
        if let ShellCommand::Cd { path } = cmd {
            match context.resolve_node(path) {
                ResolveResult::Found(fqn) => {
                    let new_curr = if fqn.is_empty() { None } else { Some(fqn) };
                    context.set_current_fqn(new_curr);
                    Ok(String::new())
                }
                ResolveResult::Ambiguous(candidates) => {
                    let mut msg = format!("Ambiguous path '{}'. Candidates:\n", path);
                    for c in candidates.iter().take(10) {
                        msg.push_str(&format!("  - {}\n", c));
                    }
                    Err(msg.into())
                }
                ResolveResult::NotFound => Err(format!("Node '{}' not found.", path).into()),
            }
        } else {
            Ok(String::new())
        }
    }
}

pub struct CatHandler;
impl CommandHandler for CatHandler {
    fn handle(
        &self,
        cmd: &ShellCommand,
        context: &mut ShellContext,
    ) -> Result<String, Box<dyn std::error::Error>> {
        if let ShellCommand::Cat { target } = cmd {
            // First resolve the target to a concrete FQN
            let fqn = match context.resolve_node(target) {
                ResolveResult::Found(f) => f,
                ResolveResult::Ambiguous(candidates) => {
                    let mut msg =
                        format!("Ambiguous match for '{}'. Available options:\n\n", target);
                    for c in candidates {
                        msg.push_str(&format!("  - {}\n", c));
                    }
                    msg.push_str("\nPlease specify the full name.");
                    return Ok(msg);
                }
                ResolveResult::NotFound => {
                    return Ok(format!(
                        "Error: Target '{}' not found in current context.",
                        target
                    ));
                }
            };

            if fqn.is_empty() {
                return Err("Cannot cat root.".into());
            }

            let engine_guard = context.naviscope.read().unwrap();
            let engine = QueryEngine::new(engine_guard.graph());
            let query = GraphQuery::Cat { fqn };
            let result = engine.execute(&query)?;

            cmd.render(result)
        } else {
            Ok(String::new())
        }
    }
}

pub struct GenericQueryHandler;
impl CommandHandler for GenericQueryHandler {
    fn handle(
        &self,
        cmd: &ShellCommand,
        context: &mut ShellContext,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let current_node = context.current_fqn();

        // Resolve argument FQN if present
        let mut resolved_target_fqn = None;
        let resolved_cmd = match cmd {
            ShellCommand::Ls {
                fqn: Some(target),
                kind,
                modifiers,
                long,
            } => {
                resolved_target_fqn = match context.resolve_node(target) {
                    ResolveResult::Found(f) => Some(f),
                    _ => Some(target.clone()),
                };
                ShellCommand::Ls {
                    fqn: resolved_target_fqn.clone(),
                    kind: kind.clone(),
                    modifiers: modifiers.clone(),
                    long: *long,
                }
            }
            ShellCommand::Deps {
                fqn: Some(target),
                rev,
                edge_types,
            } => {
                resolved_target_fqn = match context.resolve_node(target) {
                    ResolveResult::Found(f) => Some(f),
                    _ => Some(target.clone()),
                };
                ShellCommand::Deps {
                    fqn: resolved_target_fqn.clone(),
                    rev: *rev,
                    edge_types: edge_types.clone(),
                }
            }
            _ => cmd.clone(),
        };

        let query = resolved_cmd.to_graph_query(&current_node)?;
        let engine_guard = context.naviscope.read().unwrap();
        let engine = QueryEngine::new(engine_guard.graph());

        let result = engine.execute(&query)?;

        if result.is_empty() {
            if let Some(target) = resolved_target_fqn {
                // Check if node itself exists in the graph
                if engine_guard.graph().fqn_map.contains_key(&target) {
                    return Ok(format!(
                        "Node '{}' exists but has no children/relationships matching your criteria.",
                        target
                    ));
                }
            }
            return Ok("NO RECORDS FOUND".to_string());
        }

        resolved_cmd.render(result)
    }
}

pub struct PwdHandler;
impl CommandHandler for PwdHandler {
    fn handle(
        &self,
        _cmd: &ShellCommand,
        context: &mut ShellContext,
    ) -> Result<String, Box<dyn std::error::Error>> {
        Ok(context.current_fqn().unwrap_or("/".to_string()))
    }
}

pub struct ClearHandler;
impl CommandHandler for ClearHandler {
    fn handle(
        &self,
        _cmd: &ShellCommand,
        _context: &mut ShellContext,
    ) -> Result<String, Box<dyn std::error::Error>> {
        // Clear is handled by the Reedline loop mostly, but we can return a marker if needed.
        // For now, simple print or empty string. The loop handles `line_editor.clear_screen()`.
        Ok(String::new())
    }
}

pub fn get_handler(cmd: &ShellCommand) -> Box<dyn CommandHandler> {
    match cmd {
        ShellCommand::Cd { .. } => Box::new(CdHandler),
        ShellCommand::Cat { .. } => Box::new(CatHandler),
        ShellCommand::Pwd => Box::new(PwdHandler),
        ShellCommand::Clear => Box::new(ClearHandler),
        _ => Box::new(GenericQueryHandler),
    }
}
